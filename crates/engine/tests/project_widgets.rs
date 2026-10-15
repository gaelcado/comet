//! Project widgets end to end through the engine RPC: a visual shown in a
//! project chat is saved once, outlives its chat and an engine restart, and
//! concurrent clients never lose an update silently.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures::StreamExt;
use futures::stream::BoxStream;
use serde_json::{Value, json};
use zeron_doc::{MessagePart, SessionCommandPayload};
use zeron_engine::{EngineCore, HarnessRegistry};
use zeron_harness::{Harness, HarnessError, RunControls};
use zeron_proto::{
    AgentEvent, DoneStatus, HarnessId, Model, PROJECT_WIDGET_SCRIPT_ASSET, ProjectWidgetDetail,
    ProjectWidgetWrite, ProjectWidgetWriteOutcome, ProjectWidgetsSnapshot, ReasoningLevel,
    RunRequest, SandboxLevel, SteeringMode,
};
use zeron_rpc::{RpcClient, RpcError, capability_errors as codes, memory_client, methods};

const DEVICE: &str = "device-a";
const SPACE: &str = "space-a";
const CHAT: &str = "chat-a";
const SCRIPT: &str = "return ui.column { ui.text 'Sessions', ui.text 'Events' }";
const DATA: &[u8] = b"{\"sessions\":[3,5,8]}";

/// Keeps its turn open until released, so `PublishVisual` finds a live run.
struct HeldHarness {
    release: Arc<tokio::sync::Semaphore>,
}

#[async_trait]
impl Harness for HeldHarness {
    fn id(&self) -> HarnessId {
        HarnessId::Mock
    }
    fn display_name(&self) -> &str {
        "Held"
    }
    fn supports_steering(&self) -> bool {
        false
    }
    fn steering_mode(&self) -> SteeringMode {
        SteeringMode::TurnBoundary
    }
    fn reasoning_levels(&self) -> &[ReasoningLevel] {
        &[ReasoningLevel::Medium]
    }
    async fn models(&self) -> Result<Vec<Model>, HarnessError> {
        Ok(vec![])
    }
    async fn run(
        &self,
        request: RunRequest,
        _controls: RunControls,
    ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
        let release = self.release.clone();
        let started = AgentEvent::SessionStarted {
            harness: HarnessId::Mock,
            model: "mock-1".into(),
            tools: vec![],
            cwd: request.cwd,
            session_id: "sess".into(),
            assistant_message_id: "a-1".into(),
        };
        let done = async move {
            release.acquire().await.unwrap().forget();
            Ok(AgentEvent::Done {
                status: DoneStatus::Completed,
                result: None,
                error: None,
                session_id: Some("sess".into()),
            })
        };
        Ok(futures::stream::iter([Ok(started)])
            .chain(futures::stream::once(done))
            .boxed())
    }
}

fn assemble(dir: &std::path::Path, release: Arc<tokio::sync::Semaphore>) -> EngineCore {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("device-id"), DEVICE).unwrap();
    let registry = HarnessRegistry::new();
    registry.register(Arc::new(HeldHarness { release }));
    EngineCore::assemble(dir, Arc::new(registry), HarnessId::Mock, None).unwrap()
}

async fn wait_for(mut done: impl FnMut() -> bool, what: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(tokio::time::Instant::now() < deadline, "timed out: {what}");
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
}

fn code(err: &RpcError) -> &str {
    match err {
        RpcError::Capability(code) => code,
        other => panic!("expected a capability error, got {other}"),
    }
}

async fn write(
    client: &RpcClient,
    method: &str,
    params: Value,
) -> Result<ProjectWidgetWrite, RpcError> {
    Ok(serde_json::from_value(client.call(method, params).await?).unwrap())
}

async fn read_all(client: &RpcClient, widget: &str, revision: u64, name: &str) -> Vec<u8> {
    let (mut bytes, mut offset) = (Vec::new(), 0);
    loop {
        let chunk = client
            .call(
                methods::READ_PROJECT_WIDGET_ASSET_CHUNK,
                json!({ "widgetId": widget, "contentRevision": revision, "assetName": name, "offset": offset }),
            )
            .await
            .unwrap();
        bytes.extend(BASE64.decode(chunk["data"].as_str().unwrap()).unwrap());
        offset = chunk["nextOffset"].as_u64().unwrap();
        if chunk["done"].as_bool().unwrap() {
            return bytes;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn saved_visuals_outlive_their_chat_and_writes_never_cross_silently() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let project = tmp.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    let release = Arc::new(tokio::sync::Semaphore::new(0));

    // ── A visual with an asset, shown in a project chat's live turn. ──
    let widget_id = {
        let core = assemble(&data_dir, release.clone());
        core.workspace
            .create_space(SPACE, DEVICE, &project.to_string_lossy(), None, false)
            .unwrap();
        core.workspace
            .create_chat(CHAT, Some(SPACE), Some(DEVICE), None, None)
            .unwrap();
        core.workspace.rename_chat(CHAT, "Metrics").unwrap();
        core.doc_host
            .queue_command(
                CHAT,
                SessionCommandPayload::Run {
                    request: RunRequest {
                        mcp: None,
                        prompt: "show the metrics".into(),
                        harness: None,
                        model: None,
                        reasoning: None,
                        model_options: Default::default(),
                        cwd: project.to_string_lossy().into_owned(),
                        sandbox: SandboxLevel::WorkspaceWrite,
                        auto_approve: true,
                        attachments: Vec::new(),
                        worktree: None,
                        resume: None,
                    },
                    message_id: "msg-1".into(),
                },
            )
            .unwrap();
        wait_for(|| core.sessions.has_run(CHAT), "the turn to start").await;
        let agent = memory_client(core.rpc_service());
        let published = agent
            .call(
                methods::PUBLISH_VISUAL,
                json!({
                    "chatId": CHAT, "title": "Traffic", "script": SCRIPT,
                    "assets": { "data.json": BASE64.encode(DATA) },
                }),
            )
            .await
            .unwrap();
        let visual_id = published["id"].as_str().unwrap().to_owned();
        release.add_permits(1);
        let visual_in_doc = || {
            core.doc_host
                .open(CHAT)
                .ok()
                .and_then(|h| h.doc().read_entries().ok())
                .is_some_and(|entries| {
                    entries.iter().flat_map(|e| &e.parts).any(
                        |part| matches!(part, MessagePart::Visual { id, .. } if *id == visual_id),
                    )
                })
        };
        wait_for(visual_in_doc, "the visual in the transcript").await;
        wait_for(|| !core.sessions.has_run(CHAT), "the turn to end").await;

        // Two windows save the same visual: one widget, no live run needed.
        let (first_window, second_window) = (
            memory_client(core.rpc_service()),
            memory_client(core.rpc_service()),
        );
        let save = |operation: &str| json!({ "spaceId": SPACE, "sourceChatId": CHAT, "sourceVisualId": visual_id, "operationId": operation });
        let created = write(&first_window, methods::SAVE_PROJECT_VISUAL, save("save-1"))
            .await
            .unwrap();
        assert_eq!(created.outcome, ProjectWidgetWriteOutcome::Created);
        assert_eq!(created.widget.title, "Traffic");
        assert_eq!(
            created.widget.content.render_seed,
            zeron_visual::seed_for(&visual_id),
            "renders with the chat's random data"
        );
        let again = write(&second_window, methods::SAVE_PROJECT_VISUAL, save("save-2"))
            .await
            .unwrap();
        assert_eq!(again.outcome, ProjectWidgetWriteOutcome::Existing);
        assert_eq!(again.widget.id, created.widget.id);
        let retried = write(&first_window, methods::SAVE_PROJECT_VISUAL, save("save-1"))
            .await
            .unwrap();
        assert_eq!(
            (retried.outcome, retried.widget.id.as_str()),
            (
                ProjectWidgetWriteOutcome::Replayed,
                created.widget.id.as_str()
            )
        );

        // The source chat goes; the widget does not.
        first_window
            .call(
                methods::MUTATE,
                json!({ "op": "deleteChat", "chatId": CHAT }),
            )
            .await
            .unwrap();
        core.shutdown().await;
        created.widget.id
    };

    // ── After a restart: same bytes, the source reported missing. ──
    let core = assemble(&data_dir, release.clone());
    let (a, b) = (
        memory_client(core.rpc_service()),
        memory_client(core.rpc_service()),
    );
    let detail: ProjectWidgetDetail = serde_json::from_value(
        a.call(
            methods::GET_PROJECT_WIDGET,
            json!({ "widgetId": widget_id }),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert!(detail.source_missing);
    let widget = detail.widget;
    let revision = widget.content_revision;
    assert_eq!(
        read_all(&a, &widget_id, revision, PROJECT_WIDGET_SCRIPT_ASSET).await,
        SCRIPT.as_bytes()
    );
    assert_eq!(read_all(&b, &widget_id, revision, "data.json").await, DATA);

    // ── Two clients: conflicts are explicit, replays add nothing. ──
    let mut watch = b
        .subscribe(methods::WATCH_PROJECT_WIDGETS, json!({ "spaceId": SPACE }))
        .await
        .unwrap();
    let mut next_snapshot = async || -> ProjectWidgetsSnapshot {
        serde_json::from_value(watch.recv().await.unwrap()).unwrap()
    };
    let initial = next_snapshot().await;
    assert_eq!(initial.widgets.len(), 1);
    let renamed = write(
        &a,
        methods::UPDATE_PROJECT_WIDGET,
        json!({ "widgetId": widget_id, "expectedRevision": widget.revision, "operationId": "rename-a", "title": "Weekly traffic" }),
    )
    .await
    .unwrap();
    assert_eq!(renamed.widget.revision, widget.revision + 1);
    assert_eq!(
        renamed.widget.content_revision, revision,
        "a title change keeps the content"
    );
    let stale = write(
        &b,
        methods::UPDATE_PROJECT_WIDGET,
        json!({ "widgetId": widget_id, "expectedRevision": widget.revision, "operationId": "rename-b", "title": "Mine" }),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&stale), codes::WIDGETS_CONFLICT);
    assert_eq!(next_snapshot().await.widgets[0].title, "Weekly traffic");

    let create = |title: &str, operation: &str| {
        json!({
            "spaceId": SPACE, "creationKey": "kpi-board", "operationId": operation, "title": title,
            "content": { "script": "return ui.text 'KPIs'" },
        })
    };
    let board = write(
        &a,
        methods::CREATE_PROJECT_WIDGET,
        create("KPIs", "create-1"),
    )
    .await
    .unwrap();
    assert_eq!(board.outcome, ProjectWidgetWriteOutcome::Created);
    let replay = write(
        &b,
        methods::CREATE_PROJECT_WIDGET,
        create("KPIs", "create-2"),
    )
    .await
    .unwrap();
    assert_eq!(
        (replay.outcome, replay.widget.id.as_str()),
        (
            ProjectWidgetWriteOutcome::Replayed,
            board.widget.id.as_str()
        )
    );
    for (title, operation) in [("Other KPIs", "create-3"), ("Other KPIs", "create-1")] {
        let reused = write(&b, methods::CREATE_PROJECT_WIDGET, create(title, operation))
            .await
            .unwrap_err();
        assert_eq!(code(&reused), codes::WIDGETS_KEY_REUSED);
    }
    let listed = next_snapshot().await;
    assert_eq!(listed.widgets.len(), 2);

    let reorder = |ids: Vec<&str>, expected: u64| {
        json!({
            "spaceId": SPACE, "widgetIds": ids, "expectedCollectionRevision": expected,
            "operationId": uuid::Uuid::new_v4().to_string(),
        })
    };
    let partial = a
        .call(
            methods::REORDER_PROJECT_WIDGETS,
            reorder(vec![&board.widget.id], listed.collection_revision),
        )
        .await
        .unwrap_err();
    assert!(partial.to_string().starts_with("bad params: "), "{partial}");
    let stale = a
        .call(
            methods::REORDER_PROJECT_WIDGETS,
            reorder(
                vec![&board.widget.id, &widget_id],
                initial.collection_revision,
            ),
        )
        .await
        .unwrap_err();
    assert_eq!(code(&stale), codes::WIDGETS_CONFLICT);
    let reordered: ProjectWidgetsSnapshot = serde_json::from_value(
        a.call(
            methods::REORDER_PROJECT_WIDGETS,
            reorder(
                vec![&board.widget.id, &widget_id],
                listed.collection_revision,
            ),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(reordered.widgets[0].id, board.widget.id);
    assert_eq!(next_snapshot().await, reordered);

    // A deletion against a stale revision changes nothing; then it is seen.
    let delete = |revision: u64| json!({ "widgetId": widget_id, "expectedRevision": revision, "operationId": format!("delete-{revision}") });
    let stale = a
        .call(methods::DELETE_PROJECT_WIDGET, delete(widget.revision))
        .await
        .unwrap_err();
    assert_eq!(code(&stale), codes::WIDGETS_CONFLICT);
    a.call(
        methods::DELETE_PROJECT_WIDGET,
        delete(renamed.widget.revision),
    )
    .await
    .unwrap();
    let after = next_snapshot().await;
    assert_eq!(after.widgets.len(), 1);
    assert_eq!(after.widgets[0].id, board.widget.id);
    let gone = b
        .call(
            methods::GET_PROJECT_WIDGET,
            json!({ "widgetId": widget_id }),
        )
        .await
        .unwrap_err();
    assert_eq!(code(&gone), codes::WIDGETS_NOT_FOUND);

    // ── Only this device's live spaces are served. ──
    core.workspace
        .create_space("space-b", "device-b", "/elsewhere", None, false)
        .unwrap();
    for (method, params) in [
        (
            methods::GET_PROJECT_WIDGET,
            json!({ "widgetId": board.widget.id, "targetDeviceId": "device-b" }),
        ),
        (
            methods::SAVE_PROJECT_VISUAL,
            json!({ "spaceId": "space-b", "sourceChatId": CHAT, "sourceVisualId": "v:x", "operationId": "s-b" }),
        ),
        (
            methods::CREATE_PROJECT_WIDGET,
            json!({ "spaceId": "no-such-space", "creationKey": "k", "operationId": "c-x", "title": "t", "content": { "script": "return ui.text 'x'" } }),
        ),
    ] {
        let refused = a.call(method, params).await.unwrap_err();
        assert_eq!(code(&refused), codes::WIDGETS_OWNER_UNAVAILABLE, "{method}");
    }
    let Err(foreign) = a
        .subscribe_checked(
            methods::WATCH_PROJECT_WIDGETS,
            json!({ "spaceId": "space-b" }),
        )
        .await
    else {
        panic!("another device's space was watched here");
    };
    assert_eq!(code(&foreign), codes::WIDGETS_OWNER_UNAVAILABLE);

    // Deleting the space removes its widgets on the owner.
    a.call(
        methods::MUTATE,
        json!({ "op": "deleteSpace", "spaceId": SPACE }),
    )
    .await
    .unwrap();
    let removed = core.project_widgets.clone();
    let board_id = board.widget.id.clone();
    // Tombstoned (not merely unserved because the space is gone).
    wait_for(
        || {
            matches!(
                removed.get(&board_id),
                Err(zeron_engine::WidgetsError::NotFound)
            )
        },
        "the deleted space's widgets to go",
    )
    .await;
    assert_eq!(
        core.project_widgets.reconcile_deleted_spaces().unwrap(),
        0,
        "already reconciled"
    );
    core.shutdown().await;
}
