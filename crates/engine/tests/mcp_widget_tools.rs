//! The agent widget tools end to end: an agent in a project chat keeps a
//! visual with an asset file as a widget, reads it back and changes it
//! against its revision, through the bridge and the real engine RPC, under
//! the same tool policy as every other tool.

use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures::future::BoxFuture;
use serde_json::{Value, json};
use zeron_engine::{EngineCore, default_registry};
use zeron_mcp::{Origin, Tools, Zeron};
use zeron_proto::{HarnessId, McpCatalogSnapshot};
use zeron_rpc::{capability_errors as codes, memory_client, methods};

const DEVICE: &str = "device-a";
const SPACE: &str = "space-a";
const SCRIPT: &str = "return ui.column { ui.text 'Sessions', ui.text 'Events' }";
const DATA: &[u8] = b"{\"sessions\":[3,5,8]}";

/// A tool call boxed where it is made: the dispatch future is large in debug
/// builds, and a test body holding one per call site overflows its stack.
fn call<'a>(tools: &'a Tools, name: &'a str, args: Value) -> BoxFuture<'a, Result<Value, String>> {
    Box::pin(tools.call(name, args))
}

async fn ok(tools: &Tools, name: &str, args: Value) -> Value {
    call(tools, name, args).await.unwrap()
}

async fn fails(tools: &Tools, name: &str, args: Value) -> String {
    call(tools, name, args).await.unwrap_err()
}

async fn widgets(tools: &Tools) -> Value {
    ok(tools, "list_widgets", json!({})).await
}

/// `get_widget` with its content, and the bytes of its one asset.
async fn read(tools: &Tools, id: &str) -> (Value, Vec<u8>) {
    let got = ok(
        tools,
        "get_widget",
        json!({ "widgetId": id, "includeContent": true }),
    )
    .await;
    let data = BASE64
        .decode(got["assets"]["data.json"].as_str().unwrap())
        .unwrap();
    (got, data)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agents_keep_read_and_update_widgets_against_their_revision() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let project = tmp.path().join("project");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(data_dir.join("device-id"), DEVICE).unwrap();
    // The agent's own file; the bridge reads it and sends its bytes.
    let asset = tmp.path().join("data.json");
    std::fs::write(&asset, DATA).unwrap();
    let asset = asset.to_string_lossy().into_owned();

    let core = EngineCore::assemble(
        &data_dir,
        Arc::new(default_registry()),
        HarnessId::Mock,
        None,
    )
    .unwrap();
    core.workspace
        .create_space(SPACE, DEVICE, &project.to_string_lossy(), None, false)
        .unwrap();
    core.workspace
        .create_chat("origin", Some(SPACE), Some(DEVICE), None, None)
        .unwrap();
    let bridge = |origin: Origin| {
        Tools::new(Arc::new(Zeron::with_client(
            memory_client(core.rpc_service()),
            origin,
        )))
    };
    let agent = bridge(Origin {
        chat_id: Some("origin".into()),
        device_id: Some(DEVICE.into()),
    });
    let terminal = bridge(Origin::default());

    // ── Create, then replay the same creation key and payload. ──
    let create = json!({
        "project": SPACE, "creationKey": "traffic", "operationId": "op-create",
        "title": "Traffic", "script": SCRIPT, "assets": { "data.json": asset },
    });
    let created = ok(&agent, "create_widget", create.clone()).await;
    assert_eq!(created["outcome"], "created", "{created}");
    assert_eq!(created["revision"], 1);
    assert_eq!(created["contentRevision"], 1);
    let id = created["widgetId"].as_str().unwrap().to_owned();
    let mut retry = create.clone();
    retry["operationId"] = json!("op-create-retry");
    let replayed = ok(&agent, "create_widget", retry.clone()).await;
    assert_eq!(replayed["outcome"], "replayed");
    assert_eq!(replayed["widgetId"], id.as_str());
    retry["operationId"] = json!("op-create-other");
    retry["title"] = json!("Other traffic");
    let reused = fails(&agent, "create_widget", retry).await;
    assert!(reused.starts_with(codes::WIDGETS_KEY_REUSED), "{reused}");

    // ── List from the chat's own project: metadata only. ──
    let list = widgets(&agent).await;
    assert_eq!(list["widgets"].as_array().unwrap().len(), 1, "{list}");
    assert_eq!(list["widgets"][0]["id"], id.as_str());
    assert!(
        !list.to_string().contains("Sessions"),
        "no script in a list: {list}"
    );

    // ── Read back the exact bytes. ──
    let (got, data) = read(&agent, &id).await;
    assert_eq!(got["script"], SCRIPT);
    assert_eq!(data, DATA);

    // ── Title only: content kept, revision bumped. A stale one conflicts. ──
    let renamed = ok(
        &agent,
        "update_widget",
        json!({ "widgetId": id, "expectedRevision": 1, "operationId": "op-rename", "title": "Sessions" }),
    )
    .await;
    assert_eq!(renamed["revision"], 2);
    assert_eq!(renamed["contentRevision"], 1);
    let stale = fails(
        &agent,
        "update_widget",
        json!({ "widgetId": id, "expectedRevision": 1, "operationId": "op-stale", "title": "Old" }),
    )
    .await;
    assert!(stale.starts_with(codes::WIDGETS_CONFLICT), "{stale}");

    // ── New script: assets are kept only when asked, never dropped silently. ──
    let script = "return ui.column { ui.text 'Sessions this week' }";
    let mut content = json!({ "widgetId": id, "expectedRevision": 2, "operationId": "op-content", "script": script });
    let ambiguous = fails(&agent, "update_widget", content.clone()).await;
    assert!(ambiguous.contains("keepExistingAssets"), "{ambiguous}");
    content["keepExistingAssets"] = json!(true);
    let updated = ok(&agent, "update_widget", content.clone()).await;
    assert_eq!(updated["revision"], 3);
    assert_eq!(updated["contentRevision"], 2);
    // Retrying after a lost reply re-reads the widget yet gets the first answer.
    let again = ok(&agent, "update_widget", content).await;
    assert_eq!(
        (again["outcome"].as_str(), again["revision"].as_u64()),
        (Some("replayed"), Some(3))
    );
    let (got, data) = read(&agent, &id).await;
    assert_eq!(
        (got["script"].as_str(), data.as_slice()),
        (Some(script), DATA)
    );

    // ── Switched off in Settings: refused like any other tool, no effect. ──
    let settings = memory_client(core.rpc_service());
    let mut watch = settings
        .subscribe(methods::WATCH_ZERON_MCP_CATALOG, json!({}))
        .await
        .unwrap();
    let policy: McpCatalogSnapshot = serde_json::from_value(watch.recv().await.unwrap()).unwrap();
    settings
        .call(
            methods::SET_ZERON_MCP_TOOL_OVERRIDE,
            json!({
                "name": "update_widget", "expectedPolicyRevision": policy.policy_revision,
                "operationId": "policy-1", "enabled": false,
            }),
        )
        .await
        .unwrap();
    let refused = fails(
        &agent,
        "update_widget",
        json!({ "widgetId": id, "expectedRevision": 3, "operationId": "op-off", "title": "Off" }),
    )
    .await;
    assert!(refused.starts_with(codes::MCP_TOOL_DISABLED), "{refused}");
    assert_eq!(widgets(&agent).await["widgets"][0]["revision"], 3);

    // ── Outside a chat, no project means nothing is created. ──
    let mut orphan = create;
    orphan.as_object_mut().unwrap().remove("project");
    orphan["creationKey"] = json!("orphan");
    orphan["operationId"] = json!("op-orphan");
    let missing = fails(&terminal, "create_widget", orphan).await;
    assert!(missing.contains("project"), "{missing}");
    let missing = fails(&terminal, "list_widgets", json!({})).await;
    assert!(missing.contains("project is required"), "{missing}");
    assert_eq!(
        widgets(&agent).await["widgets"].as_array().unwrap().len(),
        1
    );
    core.shutdown().await;
}
