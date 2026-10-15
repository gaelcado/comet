//! Widget refresh end to end: a click starts one agent run in the widget's
//! archived chat, the run stages content through `update_widget`, and the
//! widget changes only when that run's turn completes. Errors, cancellation,
//! deletion and restarts keep the last good content.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};
use zeron_engine::{EngineCore, HarnessRegistry};
use zeron_harness::{Harness, HarnessError, RunControls};
use zeron_mcp::{Origin, Tools, Zeron};
use zeron_proto::{
    AgentEvent, DoneStatus, HarnessId, McpCatalogSnapshot, Model, ProjectWidget,
    ProjectWidgetCancelOutcome, ProjectWidgetDetail, ProjectWidgetRefresh,
    ProjectWidgetRefreshAdmission, ProjectWidgetRefreshCancellation,
    ProjectWidgetRefreshState as State, ProjectWidgetWrite, ReasoningLevel, RunRequest,
    SteeringMode,
};
use zeron_rpc::{RpcClient, RpcError, capability_errors as codes, memory_client, methods};

const DEVICE: &str = "device-a";
const SPACE: &str = "space-a";
const OLD: &str = "return ui.text 'Sessions: 3'";
const NEW: &str = "return ui.text 'Sessions: 8'";

/// One turn the test drives: what the agent was asked, and its end.
struct Turn {
    prompt: String,
    finish: oneshot::Sender<DoneStatus>,
}

/// An agent whose every turn waits for the test to end it (or a stop).
struct GatedHarness {
    turns: mpsc::UnboundedSender<Turn>,
}

#[async_trait]
impl Harness for GatedHarness {
    fn id(&self) -> HarnessId {
        HarnessId::Mock
    }
    fn display_name(&self) -> &str {
        "Gated"
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
        controls: RunControls,
    ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
        let (finish, finished) = oneshot::channel();
        let _ = self.turns.send(Turn {
            prompt: request.prompt.clone(),
            finish,
        });
        let interrupt = controls.interrupt.clone();
        let head = futures::stream::iter([
            Ok(AgentEvent::SessionStarted {
                harness: HarnessId::Mock,
                model: "mock-1".into(),
                tools: vec![],
                cwd: request.cwd.clone(),
                session_id: "sess-refresh".into(),
                assistant_message_id: "a-1".into(),
            }),
            Ok(AgentEvent::TextDelta {
                text: "collecting".into(),
            }),
        ]);
        let tail = futures::stream::once(async move {
            let status = tokio::select! {
                status = finished => status.unwrap_or(DoneStatus::Interrupted),
                _ = interrupt.cancelled() => DoneStatus::Interrupted,
            };
            Ok(AgentEvent::Done {
                status,
                result: None,
                error: None,
                session_id: Some("sess-refresh".into()),
            })
        });
        Ok(head.chain(tail).boxed())
    }
}

struct Engine {
    core: EngineCore,
    client: Arc<RpcClient>,
    turns: mpsc::UnboundedReceiver<Turn>,
}

fn assemble(data_dir: &std::path::Path) -> Engine {
    let (tx, turns) = mpsc::unbounded_channel();
    let registry = HarnessRegistry::new();
    registry.register(Arc::new(GatedHarness { turns: tx }));
    let core = EngineCore::assemble(data_dir, Arc::new(registry), HarnessId::Mock, None).unwrap();
    // Runs carry the Zeron MCP once the engine serves IPC.
    core.sessions.set_ipc_port(1);
    let client = Arc::new(memory_client(core.rpc_service()));
    Engine {
        core,
        client,
        turns,
    }
}

fn setup() -> (tempfile::TempDir, Engine) {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let project = tmp.path().join("project");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(data_dir.join("device-id"), DEVICE).unwrap();
    let engine = assemble(&data_dir);
    engine
        .core
        .workspace
        .create_space(SPACE, DEVICE, &project.to_string_lossy(), None, false)
        .unwrap();
    (tmp, engine)
}

async fn call<T: serde::de::DeserializeOwned>(
    client: &RpcClient,
    method: &str,
    params: Value,
) -> T {
    let value = Box::pin(client.call(method, params)).await.unwrap();
    serde_json::from_value(value).unwrap()
}

async fn refused(client: &RpcClient, method: &str, params: Value) -> RpcError {
    Box::pin(client.call(method, params)).await.unwrap_err()
}

async fn create(client: &RpcClient, key: &str) -> ProjectWidget {
    let write: ProjectWidgetWrite = call(
        client,
        methods::CREATE_PROJECT_WIDGET,
        json!({
            "spaceId": SPACE, "creationKey": key, "operationId": format!("create-{key}"),
            "title": "Sessions", "content": { "script": OLD },
            "refreshRecipe": { "instruction": "Recount the sessions of the last 30 days.", "harness": "mock" },
        }),
    )
    .await;
    write.widget
}

async fn widget(client: &RpcClient, id: &str) -> ProjectWidget {
    let detail: ProjectWidgetDetail = call(
        client,
        methods::GET_PROJECT_WIDGET,
        json!({ "widgetId": id }),
    )
    .await;
    detail.widget
}

async fn script(client: &RpcClient, widget: &ProjectWidget) -> String {
    let chunk: zeron_proto::ProjectWidgetAssetChunk = call(
        client,
        methods::READ_PROJECT_WIDGET_ASSET_CHUNK,
        json!({ "widgetId": widget.id, "contentRevision": widget.content_revision, "assetName": "@script" }),
    )
    .await;
    use base64::Engine as _;
    String::from_utf8(
        base64::engine::general_purpose::STANDARD
            .decode(chunk.data)
            .unwrap(),
    )
    .unwrap()
}

async fn refresh(
    client: &RpcClient,
    w: &ProjectWidget,
    request: &str,
) -> ProjectWidgetRefreshAdmission {
    call(
        client,
        methods::REFRESH_PROJECT_WIDGET,
        json!({ "widgetId": w.id, "expectedRevision": w.revision, "requestId": request }),
    )
    .await
}

async fn cancel(
    client: &RpcClient,
    id: &str,
    refresh_id: &str,
) -> ProjectWidgetRefreshCancellation {
    call(
        client,
        methods::CANCEL_PROJECT_WIDGET_REFRESH,
        json!({ "widgetId": id, "refreshId": refresh_id, "requestId": new_id() }),
    )
    .await
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// The widget's refresh once it is no longer `queued`/`running` …
async fn settled(client: &RpcClient, id: &str) -> (ProjectWidget, ProjectWidgetRefresh) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let w = widget(client, id).await;
        let refresh = w.refresh.clone().unwrap();
        if !matches!(
            refresh.state,
            State::Queued | State::Running | State::AwaitingInput
        ) {
            return (w, refresh);
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "refresh never ended: {refresh:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn next_turn(engine: &mut Engine) -> Turn {
    tokio::time::timeout(Duration::from_secs(10), engine.turns.recv())
        .await
        .expect("the refresh run starts")
        .unwrap()
}

/// The agent of a chat, through the Zeron MCP bridge.
fn agent(engine: &Engine, chat: &str) -> Tools {
    Tools::new(Arc::new(Zeron::with_client(
        memory_client(engine.core.rpc_service()),
        Origin {
            chat_id: Some(chat.into()),
            device_id: Some(DEVICE.into()),
        },
    )))
}

fn update<'a>(tools: &'a Tools, args: Value) -> BoxFuture<'a, Result<Value, String>> {
    Box::pin(tools.call("update_widget", args))
}

async fn stage(
    tools: &Tools,
    w: &ProjectWidget,
    refresh_id: &str,
    script: &str,
) -> Result<Value, String> {
    update(
        tools,
        json!({
            "widgetId": w.id, "expectedRevision": w.revision, "operationId": new_id(),
            "script": script, "refreshId": refresh_id,
        }),
    )
    .await
}

async fn idle(engine: &Engine, chat: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while engine.core.sessions.turn_in_flight(chat) {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the run never stopped"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refresh_publishes_only_after_its_turn_completes() {
    let (_tmp, mut engine) = setup();
    let client = engine.client.clone();
    let w = create(&client, "sessions").await;
    let created_at = w.last_successful_at;

    // ── One run per click, however often it is replayed. ──
    let first = refresh(&client, &w, "click-1").await;
    assert!(!first.existing);
    let id = first.refresh.refresh_id.clone();
    let chat = first.refresh.execution_chat_id.clone().unwrap();
    assert_eq!(refresh(&client, &w, "click-1").await.refresh.refresh_id, id);
    let retry = refresh(&client, &w, "click-1-retry").await;
    assert!(retry.existing, "an active attempt is joined, not doubled");
    assert_eq!(retry.refresh.refresh_id, id);
    let turn = next_turn(&mut engine).await;
    assert!(
        turn.prompt.contains(&id) && turn.prompt.contains(&w.id),
        "{}",
        turn.prompt
    );
    assert!(turn.prompt.contains(OLD), "the current script is quoted");
    let row = engine.core.workspace.chat(&chat).unwrap().unwrap();
    assert!(row.archived, "the refresh chat stays out of the chat list");

    // ── The run stages; the widget is unchanged until the turn ends. ──
    let run = agent(&engine, &chat);
    let staged = stage(&run, &w, &id, NEW).await.unwrap();
    assert_eq!(staged["outcome"], "staged", "{staged}");
    let invalid = stage(&run, &w, &id, "return ui.text(").await.unwrap_err();
    assert!(
        invalid.starts_with(codes::WIDGETS_INVALID_VISUAL),
        "{invalid}"
    );
    let around = update(
        &run,
        json!({ "widgetId": w.id, "expectedRevision": w.revision, "operationId": new_id(), "script": "return ui.text 'direct'" }),
    )
    .await
    .unwrap_err();
    assert!(around.starts_with(codes::WIDGETS_REFRESH_SCOPE), "{around}");
    let elsewhere = stage(&agent(&engine, "another-chat"), &w, &id, NEW)
        .await
        .unwrap_err();
    assert!(
        elsewhere.starts_with(codes::WIDGETS_REFRESH_INACTIVE),
        "{elsewhere}"
    );
    assert_eq!(widget(&client, &w.id).await.content_revision, 1);

    turn.finish.send(DoneStatus::Completed).unwrap();
    let (w, done) = settled(&client, &w.id).await;
    assert_eq!(done.state, State::Succeeded, "{done:?}");
    assert_eq!((w.revision, w.content_revision), (2, 2));
    assert!(w.last_successful_at > created_at);
    assert_eq!(
        script(&client, &w).await,
        NEW,
        "the last valid candidate won"
    );
    let late = cancel(&client, &w.id, &id).await;
    assert_eq!(late.outcome, ProjectWidgetCancelOutcome::AlreadyCompleted);
    let succeeded_at = w.last_successful_at;

    // ── A candidate, then an errored turn: nothing changes. ──
    let second = refresh(&client, &w, "click-2").await;
    assert_ne!(second.refresh.refresh_id, id);
    assert_eq!(
        second.refresh.execution_chat_id.as_deref(),
        Some(chat.as_str()),
        "the chat is reused"
    );
    let turn = next_turn(&mut engine).await;
    stage(
        &run,
        &w,
        &second.refresh.refresh_id,
        "return ui.text 'broken run'",
    )
    .await
    .unwrap();
    turn.finish.send(DoneStatus::Errored).unwrap();
    let (after, failed) = settled(&client, &w.id).await;
    assert_eq!(
        (failed.state, failed.error_code.as_deref()),
        (State::Failed, Some("agent_error"))
    );
    assert_eq!(
        (after.content_revision, after.last_successful_at),
        (2, succeeded_at)
    );
    assert_eq!(script(&client, &after).await, NEW);

    // ── A completed turn that sent nothing is no result. ──
    refresh(&client, &w, "click-3").await;
    next_turn(&mut engine)
        .await
        .finish
        .send(DoneStatus::Completed)
        .unwrap();
    let (_, empty) = settled(&client, &w.id).await;
    assert_eq!(
        (empty.state, empty.error_code.as_deref()),
        (State::Failed, Some("no_result"))
    );

    // ── Cancel before the end: the run stops and can no longer publish. ──
    let fourth = refresh(&client, &w, "click-4").await.refresh.refresh_id;
    let _held = next_turn(&mut engine).await;
    stage(&run, &w, &fourth, "return ui.text 'cancelled run'")
        .await
        .unwrap();
    let cancelled = cancel(&client, &w.id, &fourth).await;
    assert_eq!(cancelled.outcome, ProjectWidgetCancelOutcome::Cancelled);
    assert_eq!(cancelled.refresh.state, State::Cancelled);
    let late = stage(&run, &w, &fourth, "return ui.text 'too late'")
        .await
        .unwrap_err();
    assert!(late.starts_with(codes::WIDGETS_REFRESH_INACTIVE), "{late}");
    assert_eq!(
        cancel(&client, &w.id, &fourth).await.outcome,
        ProjectWidgetCancelOutcome::AlreadyCancelled
    );
    idle(&engine, &chat).await;
    let after = widget(&client, &w.id).await;
    assert_eq!(
        (after.content_revision, after.last_successful_at),
        (2, succeeded_at)
    );

    // ── Deleting the widget cancels its running refresh and stops the run. ──
    let fifth = refresh(&client, &w, "click-5").await.refresh.refresh_id;
    let _held = next_turn(&mut engine).await;
    let _: Value = call(
        &client,
        methods::DELETE_PROJECT_WIDGET,
        json!({ "widgetId": w.id, "expectedRevision": w.revision, "operationId": "delete-1" }),
    )
    .await;
    let gone = cancel(&client, &w.id, &fifth).await;
    assert_eq!(gone.outcome, ProjectWidgetCancelOutcome::AlreadyCancelled);
    assert_eq!(gone.refresh.error_code.as_deref(), Some("deleted"));
    idle(&engine, &chat).await;
    assert!(engine.core.workspace.chat(&chat).unwrap().unwrap().archived);

    // ── With update_widget switched off, a refresh cannot start. ──
    let other = create(&client, "other").await;
    let mut policy = client
        .subscribe(methods::WATCH_ZERON_MCP_CATALOG, json!({}))
        .await
        .unwrap();
    let policy: McpCatalogSnapshot = serde_json::from_value(policy.recv().await.unwrap()).unwrap();
    let _: Value = call(
        &client,
        methods::SET_ZERON_MCP_TOOL_OVERRIDE,
        json!({
            "name": "update_widget", "expectedPolicyRevision": policy.policy_revision,
            "operationId": "policy-off", "enabled": false,
        }),
    )
    .await;
    let blocked = refused(
        &client,
        methods::REFRESH_PROJECT_WIDGET,
        json!({ "widgetId": other.id, "expectedRevision": other.revision, "requestId": "click-off" }),
    )
    .await;
    assert!(
        matches!(&blocked, RpcError::Failed(m) if m.starts_with(codes::WIDGETS_REFRESH_UNAVAILABLE) && m.contains("update_widget")),
        "{blocked:?}"
    );
    assert!(widget(&client, &other.id).await.refresh.is_none());
    engine.core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refresh_cut_short_by_a_restart_is_interrupted_and_never_resumed() {
    let (tmp, mut engine) = setup();
    let client = engine.client.clone();
    let w = create(&client, "sessions").await;
    let admitted = refresh(&client, &w, "click-1").await.refresh;
    let chat = admitted.execution_chat_id.unwrap();
    let _held = next_turn(&mut engine).await;
    engine.core.shutdown().await;
    drop(engine);

    // A crash mid-turn: the run's journal never got its end.
    let journal = walk(tmp.path(), &format!("{chat}.jsonl")).expect("the run's journal");
    let kept: Vec<String> = std::fs::read_to_string(&journal)
        .unwrap()
        .lines()
        .filter(|line| {
            serde_json::from_str::<Value>(line).is_ok_and(|v| v["event"]["type"] != "done")
        })
        .map(str::to_owned)
        .collect();
    std::fs::write(&journal, kept.join("\n") + "\n").unwrap();
    // … and its reply was still streaming: what boot recovery would resume.
    let store = zeron_sync::DocsStore::open(journal.parent().unwrap().parent().unwrap()).unwrap();
    let doc = loro::LoroDoc::new();
    doc.import(&store.load_snapshot(&chat).unwrap().unwrap())
        .unwrap();
    let doc = zeron_doc::SessionDoc::from_doc(doc);
    doc.push_message(&zeron_doc::SessionMessageEntry {
        id: "streaming-reply".into(),
        role: zeron_doc::MessageRole::Assistant,
        parts: vec![],
        created_at: chrono::Utc::now().timestamp_millis(),
        device_id: DEVICE.into(),
        status: Some(zeron_doc::MessageStatus::Streaming),
        continuation_of: None,
        duration_ms: None,
    })
    .unwrap();
    store
        .save_snapshot(&chat, &doc.export_snapshot().unwrap())
        .unwrap();
    drop(store);

    let mut engine = assemble(&tmp.path().join("data"));
    let after = widget(&engine.client, &w.id).await;
    let refresh = after.refresh.unwrap();
    assert_eq!(refresh.refresh_id, admitted.refresh_id);
    assert_eq!(
        (refresh.state, refresh.error_code.as_deref()),
        (State::Interrupted, Some("engine_restarted"))
    );
    assert_eq!(
        (after.content_revision, after.last_successful_at),
        (1, w.last_successful_at)
    );
    let resumed = tokio::time::timeout(Duration::from_millis(1500), engine.turns.recv()).await;
    assert!(
        resumed.is_err(),
        "a paid refresh run is never resumed on its own"
    );
    engine.core.shutdown().await;
}

fn walk(dir: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = walk(&path, name) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|n| n == name) {
            return Some(path);
        }
    }
    None
}
