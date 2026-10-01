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
    assert_eq!(eligibility.reason, Some(VoiceRejection::Unsupported));
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

#[cfg(unix)]
fn native_package(root: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../harness/tests/fixtures");
    let binary = root.join("bin/codex");
    let helper = root.join("codex-resources/voice/bin/codex-voice-host");
    std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
    std::fs::create_dir_all(helper.parent().unwrap()).unwrap();
    std::fs::write(
        root.join("codex-package.json"),
        r#"{"layoutVersion":1,"version":"0.159.0"}"#,
    )
    .unwrap();
    for (source, target) in [
        ("fake-codex-voice-native.py", &binary),
        ("fake-codex-voice-host.py", &helper),
    ] {
        std::fs::copy(fixture.join(source), target).unwrap();
        std::fs::set_permissions(target, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    binary
}
#[cfg(unix)]
async fn native_core(temp: &tempfile::TempDir) -> EngineCore {
    let package = temp.path().join("codex-package");
    let binary = native_package(&package);
    let registry = std::sync::Arc::new(HarnessRegistry::new());
    registry.register(std::sync::Arc::new(
        zeron_harness::CodexHarness::new().with_executable(binary),
    ));
    let core = EngineCore::assemble_with_profile(
        EngineProfile::local(&temp.path().join("data")).unwrap(),
        registry,
        HarnessId::Codex,
        None,
    )
    .unwrap();
    let config =
        serde_json::from_value(json!({"harness":"codex","sandbox":"danger-full-access"})).unwrap();
    core.workspace
        .create_chat(
            "native-voice",
            None,
            Some(&core.device_id),
            Some(config),
            Some(package.display().to_string()),
        )
        .unwrap();
    core
}
#[cfg(unix)]
#[tokio::test]
async fn native_voice_signals_audio_owner_transcripts_stop_and_restart_on_same_runtime() {
    use zeron_proto::voice::{MuteVoice, VoiceEvent, VoiceLease, VoicePhase};
    let temp = tempfile::tempdir().unwrap();
    let core = native_core(&temp).await;
    let client = zeron_rpc::memory_client(core.rpc_service());
    let request = json!({"chatId":"native-voice","hostDeviceId":core.device_id});
    let eligibility: VoiceEligibility = client
        .call_as(methods::VOICE_ELIGIBILITY, request.clone())
        .await
        .unwrap();
    assert!(eligibility.available && eligibility.native_webrtc);
    assert!(!eligibility.credits_excluded);
    assert_eq!(eligibility.voices, vec!["juniper", "ember"]);
    for iteration in 0..2 {
        let lease: VoiceLease = client
            .call_as(methods::START_VOICE, request.clone())
            .await
            .unwrap();
        let mut owner = client
            .subscribe_scoped(methods::OWN_VOICE, serde_json::to_value(&lease).unwrap())
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(10),async {
            while let Some(v)=owner.recv().await {if matches!(serde_json::from_value::<VoiceEvent>(v).unwrap(),VoiceEvent::Snapshot{snapshot} if snapshot.phase==VoicePhase::Active){return;}}
            panic!("owner ended before active");
        }).await.unwrap();
        client
            .call(
                methods::MUTE_VOICE,
                serde_json::to_value(MuteVoice {
                    lease: lease.clone(),
                    muted: true,
                })
                .unwrap(),
            )
            .await
            .unwrap();
        let handle = core.doc_host.open("native-voice").unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if handle
                    .doc()
                    .read_entries()
                    .unwrap()
                    .iter()
                    .filter(|e| e.id.starts_with("voice:"))
                    .count()
                    == (iteration + 1) * 2
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(client.call(methods::APPEND_VOICE, json!({})).await.is_err());
        client
            .call(methods::STOP_VOICE, serde_json::to_value(&lease).unwrap())
            .await
            .unwrap();
        drop(owner);
    }
    let wire = std::fs::read_to_string(temp.path().join("codex-package/voice-wire.jsonl")).unwrap();
    assert_eq!(
        wire.lines()
            .filter(|s| s.contains("\"method\": \"thread/start\""))
            .count(),
        1,
        "restart must reuse the same thread"
    );
    assert_eq!(
        wire.lines()
            .filter(|s| s.contains("\"method\": \"thread/realtime/start\""))
            .count(),
        2
    );
    assert!(!wire.contains("appendAudio"));
    assert!(!wire.contains("\"method\": \"turn/start\""));
    assert!(
        core.sessions.turn_in_flight("native-voice"),
        "ending voice must preserve native delegated work"
    );
    core.sessions.shutdown().await;
}
#[cfg(unix)]
#[tokio::test]
async fn native_voice_rejects_api_auth_before_creating_call_and_unowned_start_never_opens_devices()
{
    let temp = tempfile::tempdir().unwrap();
    let core = native_core(&temp).await;
    let client = zeron_rpc::memory_client(core.rpc_service());
    let request = json!({"chatId":"native-voice","hostDeviceId":core.device_id});
    std::fs::write(temp.path().join("codex-package/account-mode"), "apiKey").unwrap();
    let eligible: VoiceEligibility = client
        .call_as(methods::VOICE_ELIGIBILITY, request.clone())
        .await
        .unwrap();
    assert_eq!(eligible.reason, Some(VoiceRejection::ChatgptRequired));
    assert!(
        client
            .call(methods::START_VOICE, request.clone())
            .await
            .is_err()
    );
    std::fs::write(temp.path().join("codex-package/account-mode"), "chatgpt").unwrap();
    let _: zeron_proto::voice::VoiceLease =
        client.call_as(methods::START_VOICE, request).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let wire = std::fs::read_to_string(temp.path().join("codex-package/voice-wire.jsonl")).unwrap();
    assert!(!wire.contains("\"method\": \"thread/realtime/start\""));
    assert!(!temp.path().join("codex-package/helper-wire.jsonl").exists());
    core.sessions.shutdown().await;
}

#[cfg(unix)]
async fn active_owner(
    client: &RpcClient,
    device: &str,
) -> (zeron_proto::voice::VoiceLease, zeron_rpc::RpcSubscription) {
    use zeron_proto::voice::{VoiceEvent, VoicePhase};
    let lease = client
        .call_as(
            methods::START_VOICE,
            json!({"chatId":"native-voice","hostDeviceId":device}),
        )
        .await
        .unwrap();
    let mut owner = client
        .subscribe_scoped(methods::OWN_VOICE, serde_json::to_value(&lease).unwrap())
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10),async {
        while let Some(v)=owner.recv().await {if matches!(serde_json::from_value::<VoiceEvent>(v).unwrap(),VoiceEvent::Snapshot{snapshot} if snapshot.phase==VoicePhase::Active){return;}}
        panic!("voice ended before active");
    }).await.unwrap();
    (lease, owner)
}
#[cfg(target_os = "linux")]
#[tokio::test]
async fn owner_loss_kills_microphone_even_when_native_controls_are_stalled() {
    let temp = tempfile::tempdir().unwrap();
    let core = native_core(&temp).await;
    let client = zeron_rpc::memory_client(core.rpc_service());
    let (lease, owner) = active_owner(&client, &core.device_id).await;
    let package = temp.path().join("codex-package");
    std::fs::write(package.join("helper-mode"), "stall-controls").unwrap();
    let rpc = zeron_rpc::memory_client(core.rpc_service());
    let mute = tokio::spawn(async move {
        rpc.call(
            methods::MUTE_VOICE,
            serde_json::to_value(zeron_proto::voice::MuteVoice { lease, muted: true }).unwrap(),
        )
        .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while !package.join("controls-stalled").exists() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let pid = std::fs::read_to_string(package.join("helper.pid")).unwrap();
    drop(owner);
    tokio::time::timeout(std::time::Duration::from_millis(500), async {
        while std::path::Path::new(&format!("/proc/{pid}")).exists() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("native capture must stop independently of the blocked actor");
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(8), mute)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    core.sessions.shutdown().await;
}
#[cfg(unix)]
#[tokio::test]
async fn device_open_failure_releases_lease_and_preserves_text_runtime() {
    use zeron_proto::voice::{VoiceEvent, VoiceLease};
    let temp = tempfile::tempdir().unwrap();
    let core = native_core(&temp).await;
    let client = zeron_rpc::memory_client(core.rpc_service());
    std::fs::write(temp.path().join("codex-package/helper-mode"), "exit-open").unwrap();
    let request = json!({"chatId":"native-voice","hostDeviceId":core.device_id});
    let lease: VoiceLease = client
        .call_as(methods::START_VOICE, request.clone())
        .await
        .unwrap();
    let mut owner = client
        .subscribe_scoped(methods::OWN_VOICE, serde_json::to_value(&lease).unwrap())
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while let Some(v) = owner.recv().await {
            if let VoiceEvent::Closed { reason, .. } = serde_json::from_value(v).unwrap() {
                assert_eq!(reason, Some(VoiceRejection::DeviceUnavailable));
                return;
            }
        }
        panic!("missing device failure");
    })
    .await
    .unwrap();
    drop(owner);
    std::fs::remove_file(temp.path().join("codex-package/helper-mode")).unwrap();
    let (_lease, owner) = active_owner(&client, &core.device_id).await;
    drop(owner);
    assert!(core.sessions.turn_in_flight("native-voice"));
    core.sessions.shutdown().await;
}

#[cfg(unix)]
#[tokio::test]
async fn delegated_subscription_policy_survives_voice_stop_and_propagates_to_children() {
    let temp = tempfile::tempdir().unwrap();
    let core = native_core(&temp).await;
    let client = zeron_rpc::memory_client(core.rpc_service());
    let (lease, owner) = active_owner(&client, &core.device_id).await;
    client
        .call(methods::STOP_VOICE, serde_json::to_value(&lease).unwrap())
        .await
        .unwrap();
    drop(owner);
    let create = json!({"op":"createChat", "chatId":"voice-child", "originChatId":"native-voice", "deviceId":core.device_id, "config":{"harness":"codex","sandbox":"workspace-write"}});
    client.call(methods::MUTATE, create.clone()).await.unwrap();
    let policy = client
        .call(methods::VOICE_TASK_POLICY, json!({"chatId":"voice-child"}))
        .await
        .unwrap();
    assert_eq!(policy["subscriptionBacked"], true);
    let mut forbidden = create;
    forbidden["chatId"] = json!("claude-child");
    forbidden["originChatId"] = json!("voice-child");
    forbidden["config"]["harness"] = json!("claude-code");
    assert!(client.call(methods::MUTATE, forbidden).await.is_err());
    assert!(core.workspace.chat("claude-child").unwrap().is_none());
    client.call(methods::MUTATE, json!({"op":"setChatConfig", "chatId":"voice-child", "config":{"harness":"claude-code","sandbox":"workspace-write"}})).await.unwrap();
    assert!(
        client
            .call(
                methods::QUEUE_MESSAGE,
                json!({"originChatId":"native-voice", "chatId":"voice-child", "text":"go"})
            )
            .await
            .is_err()
    );
    assert!(client.call(methods::QUEUE_MESSAGE, json!({"originChatId":"voice-child", "chatId":"native-voice", "text":"go", "targetDeviceId":"remote"})).await.is_err());
    core.sessions.shutdown().await;
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn account_update_kills_stalled_native_media_at_reader_boundary() {
    let temp = tempfile::tempdir().unwrap();
    let core = native_core(&temp).await;
    let client = zeron_rpc::memory_client(core.rpc_service());
    let (lease, owner) = active_owner(&client, &core.device_id).await;
    let package = temp.path().join("codex-package");
    std::fs::write(package.join("helper-mode"), "stall-controls").unwrap();
    let rpc = zeron_rpc::memory_client(core.rpc_service());
    let mute = tokio::spawn(async move {
        rpc.call(
            methods::MUTE_VOICE,
            serde_json::to_value(zeron_proto::voice::MuteVoice { lease, muted: true }).unwrap(),
        )
        .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while !package.join("controls-stalled").exists() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let pid = std::fs::read_to_string(package.join("helper.pid")).unwrap();
    std::fs::write(package.join("identity-updated"), "changed").unwrap();
    tokio::time::timeout(std::time::Duration::from_millis(500), async {
        while std::path::Path::new(&format!("/proc/{pid}")).exists() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("auth changes must kill capture without waiting for native controls");
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(8), mute)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    drop(owner);
    core.sessions.shutdown().await;
}
