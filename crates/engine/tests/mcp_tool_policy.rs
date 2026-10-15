//! The built-in Zeron MCP tool policy end to end: Settings changes it through
//! the engine RPC, and every bridge joined to that engine follows it at call
//! time, whatever list its agent saw earlier.

use std::sync::Arc;

use serde_json::{Value, json};
use zeron_engine::{EngineCore, default_registry};
use zeron_mcp::{Origin, Tools, Zeron, catalog};
use zeron_proto::{HarnessId, McpCatalogSnapshot};
use zeron_rpc::{RpcClient, RpcError, capability_errors, memory_client, methods};

async fn set(
    settings: &RpcClient,
    revision: u64,
    name: &str,
    change: Value,
) -> Result<McpCatalogSnapshot, RpcError> {
    let mut params = json!({
        "name": name,
        "expectedPolicyRevision": revision,
        "operationId": uuid::Uuid::new_v4().to_string(),
    });
    params
        .as_object_mut()
        .unwrap()
        .extend(change.as_object().unwrap().clone());
    let reply = settings
        .call(methods::SET_ZERON_MCP_TOOL_OVERRIDE, params)
        .await?;
    Ok(serde_json::from_value(reply).unwrap())
}

async fn chat_count(tools: &Tools) -> u64 {
    tools
        .call("list_chats", json!({ "include_archived": true }))
        .await
        .unwrap()["total"]
        .as_u64()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn switched_off_tools_are_refused_before_any_effect_on_every_bridge() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("device-id"), "device-a").unwrap();
    let core = EngineCore::assemble(
        dir.path(),
        Arc::new(default_registry()),
        HarnessId::Mock,
        None,
    )
    .unwrap();
    for chat in ["origin", "target"] {
        core.workspace
            .create_chat(chat, None, Some("device-a"), None, None)
            .unwrap();
    }
    let bridge = |origin: Origin| {
        Tools::new(Arc::new(Zeron::with_client(
            memory_client(core.rpc_service()),
            origin,
        )))
    };
    let in_chat = bridge(Origin {
        chat_id: Some("origin".into()),
        device_id: Some("device-a".into()),
    });
    let standalone = bridge(Origin::default());
    let settings = memory_client(core.rpc_service());
    let mut watch = settings
        .subscribe(methods::WATCH_ZERON_MCP_CATALOG, json!({}))
        .await
        .unwrap();
    let initial: McpCatalogSnapshot = serde_json::from_value(watch.recv().await.unwrap()).unwrap();
    assert_eq!(initial.tools.len(), catalog::catalog().len());

    // The in-chat agent lists every tool once and keeps that list.
    let cached = in_chat.list().await.unwrap();
    assert!(cached.tools.iter().any(|t| t.name == "archive_chat"));
    assert!(!standalone.has("show_visual").await.unwrap());

    let revision = set(
        &settings,
        initial.policy_revision,
        "archive_chat",
        json!({ "enabled": false }),
    )
    .await
    .unwrap()
    .policy_revision;
    let seen: McpCatalogSnapshot = serde_json::from_value(watch.recv().await.unwrap()).unwrap();
    assert_eq!(seen.policy_revision, revision);
    // A Settings window holding the old revision changes nothing.
    let stale = set(
        &settings,
        initial.policy_revision,
        "archive_chat",
        json!({ "enabled": true }),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(&stale, RpcError::Capability(code) if code == capability_errors::MCP_POLICY_CONFLICT),
        "{stale}"
    );

    // Calling from the old list is a business error, and nothing changes.
    let refused = zeron_mcp::handle_request(
        &in_chat,
        json!(1),
        "tools/call",
        json!({ "name": "archive_chat", "arguments": { "chat": "target" } }),
    )
    .await;
    assert_eq!(refused["result"]["isError"], true, "{refused}");
    assert_eq!(
        refused["result"]["structuredContent"]["error"]["code"],
        capability_errors::MCP_TOOL_DISABLED
    );
    assert!(!core.workspace.chat("target").unwrap().unwrap().archived);
    assert!(!standalone.has("archive_chat").await.unwrap());

    // Batch items are admitted as their unit tool.
    let revision = set(
        &settings,
        revision,
        "create_chat",
        json!({ "enabled": false }),
    )
    .await
    .unwrap()
    .policy_revision;
    let before = chat_count(&standalone).await;
    let batch = in_chat
        .call(
            "create_chats",
            json!({ "requests": [{ "kind": "chat" }, { "kind": "chat", "title": "x" }] }),
        )
        .await
        .unwrap();
    for item in batch["results"].as_array().unwrap() {
        assert_eq!(
            item["code"],
            capability_errors::MCP_TOOL_DISABLED,
            "{batch}"
        );
    }
    assert_eq!(chat_count(&standalone).await, before);
    // Switching off only the batch leaves the unit tool to its own checks.
    let revision = set(
        &settings,
        revision,
        "create_chat",
        json!({ "enabled": true }),
    )
    .await
    .unwrap()
    .policy_revision;
    let revision = set(
        &settings,
        revision,
        "create_chats",
        json!({ "enabled": false }),
    )
    .await
    .unwrap()
    .policy_revision;
    let unit = standalone
        .call("create_chat", json!({ "harness": "no-such-harness" }))
        .await
        .unwrap_err();
    assert!(unit.contains("unknown harness"), "{unit}");

    // An override changes the offered text only, never the schema.
    let original = catalog::catalog()
        .into_iter()
        .find(|def| def.name == "list_chats")
        .unwrap();
    let revision = set(
        &settings,
        revision,
        "list_chats",
        json!({ "descriptionPatch": { "kind": "set", "text": "Chats, newest first." } }),
    )
    .await
    .unwrap()
    .policy_revision;
    let listed = standalone.list().await.unwrap();
    let offered = listed
        .tools
        .iter()
        .find(|t| t.name == "list_chats")
        .unwrap();
    assert_eq!(offered.description, "Chats, newest first.");
    assert_eq!(offered.input_schema, original.input_schema);
    assert_eq!(listed.policy_revision, revision);

    // Switched back on, the same call from the same old list takes effect.
    set(
        &settings,
        revision,
        "archive_chat",
        json!({ "enabled": true }),
    )
    .await
    .unwrap();
    in_chat
        .call("archive_chat", json!({ "chat": "target" }))
        .await
        .unwrap();
    assert!(core.workspace.chat("target").unwrap().unwrap().archived);
    core.shutdown().await;
}
