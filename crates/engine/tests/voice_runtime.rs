//! Offline boundary tests: rejected voice must not launch a harness or task.
use serde_json::json;
use zeron_engine::{EngineCore, EngineProfile, HarnessRegistry};
use zeron_proto::{
    HarnessId,
    voice::{VoiceEligibility, VoiceRejection},
};
use zeron_rpc::{RpcClient, methods};

async fn rejects_without_launching(client: &RpcClient, device: &str) {
    let request = json!({"chatId":"voice-chat", "hostDeviceId":device});
    let eligibility: VoiceEligibility = client
        .call_as(methods::VOICE_ELIGIBILITY, request.clone())
        .await
        .unwrap();
    assert!(!eligibility.available);
    assert_eq!(
        eligibility.reason,
        Some(VoiceRejection::CreditExclusionUnverified)
    );
    assert!(!eligibility.credits_excluded);
    assert_eq!(eligibility.ordinary_usage_allowed, None);
    assert!(
        client
            .call(methods::START_VOICE, request.clone())
            .await
            .is_err()
    );
    assert!(
        client
            .call(
                methods::START_VOICE,
                json!({"chatId":"voice-chat", "hostDeviceId":device, "apiKey":"invalid"})
            )
            .await
            .is_err()
    );
    let remote: VoiceEligibility = client
        .call_as(
            methods::VOICE_ELIGIBILITY,
            json!({"chatId":"voice-chat", "hostDeviceId":"other-device"}),
        )
        .await
        .unwrap();
    assert_eq!(remote.reason, Some(VoiceRejection::RemoteHost));
    assert!(client.call(methods::START_VOICE, json!({"chatId":"voice-chat", "hostDeviceId":device, "targetDeviceId":"other-device"})).await.is_err());
}

#[tokio::test]
async fn voice_billing_gate_is_identical_in_memory_and_loopback() {
    let temp = tempfile::tempdir().unwrap();
    // Empty registry: even an accidentally requested harness cannot be started.
    let core = EngineCore::assemble_with_profile(
        EngineProfile::local(temp.path()).unwrap(),
        std::sync::Arc::new(HarnessRegistry::new()),
        HarnessId::Codex,
        None,
    )
    .unwrap();
    let config = serde_json::from_value(
        json!({"harness":"codex", "model":null, "reasoning":null, "sandbox":"danger-full-access"}),
    )
    .unwrap();
    core.workspace
        .create_chat(
            "voice-chat",
            None,
            Some(&core.device_id),
            Some(config),
            None,
        )
        .unwrap();
    rejects_without_launching(
        &zeron_rpc::memory_client(core.rpc_service()),
        &core.device_id,
    )
    .await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(zeron_rpc::serve_ws_listener(listener, core.rpc_service()));
    let client = zeron_rpc::connect_ws(&format!("ws://{address}"))
        .await
        .unwrap();
    rejects_without_launching(&client, &core.device_id).await;
    assert!(core.sessions.last_request("voice-chat").is_none());
    assert!(core.sessions.watch_sessions().borrow().is_empty());
    server.abort();
    core.sessions.shutdown().await;
}
