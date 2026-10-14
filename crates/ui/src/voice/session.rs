//! The engine owns Codex's native WebRTC helper. This viewport receives only
//! ephemeral controls, meters and transcripts, never PCM or SDP credentials.
use crate::state::EngineHandle;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use zeron_proto::voice::*;
use zeron_rpc::methods;
struct ProviderGuard {
    engine: EngineHandle,
    lease: VoiceLease,
}
impl Drop for ProviderGuard {
    fn drop(&mut self) {
        let engine = self.engine.clone();
        let lease = self.lease.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = tokio::time::timeout(
                    Duration::from_secs(8),
                    engine
                        .client()
                        .call(methods::STOP_VOICE, serde_json::to_value(lease).unwrap()),
                )
                .await;
            });
        }
    }
}
/// Removes a conversation made for the voice chat unless voice actually
/// started in it.
struct ChatGuard {
    engine: EngineHandle,
    chat_id: String,
    armed: bool,
}
impl Drop for ChatGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let engine = self.engine.clone();
        let delete = serde_json::json!({"op":"deleteChat","chatId":self.chat_id});
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = tokio::time::timeout(
                    Duration::from_secs(8),
                    engine.client().call(methods::MUTATE, delete),
                )
                .await;
            });
        }
    }
}

/// Logs how long one step of placing a call took on this side of the engine,
/// beside the harness's own `voice start` stages.
fn stage(name: &'static str, since: &mut std::time::Instant) {
    tracing::info!(
        target: "zeron_ui::voice",
        stage = name,
        ms = since.elapsed().as_millis() as u64,
        "voice start"
    );
    *since = std::time::Instant::now();
}

async fn mutate(engine: &EngineHandle, params: serde_json::Value) -> Result<(), VoiceRejection> {
    tokio::time::timeout(
        Duration::from_secs(30),
        engine.client().call(methods::MUTATE, params),
    )
    .await
    .map_err(|_| VoiceRejection::Protocol)?
    .map(drop)
    .map_err(|_| VoiceRejection::Protocol)
}

/// `create` is the mutation for a conversation made for this voice chat;
/// `None` continues an existing one, which is never renamed or removed.
pub(super) async fn run(
    engine: EngineHandle,
    create: Option<serde_json::Value>,
    request: StartVoice,
    cancel: CancellationToken,
    events: mpsc::Sender<VoiceEvent>,
    mut controls: mpsc::Receiver<super::VoiceControl>,
) -> Result<(), VoiceRejection> {
    let mut since = std::time::Instant::now();
    tokio::select! {
        biased;
        _ = cancel.cancelled() => return Ok(()),
        result = super::permissions::microphone() => result?,
    }
    stage("microphone permission", &mut since);
    let mut chat = ChatGuard {
        engine: engine.clone(),
        chat_id: request.chat_id.clone(),
        armed: create.is_some(),
    };
    // The media connection and the conversation do not depend on each
    // other: both are opened at once.
    let prepared = async {
        let created = async {
            let Some(create) = create else { return Ok(()) };
            let rename = serde_json::json!({"op":"renameChat","chatId":request.chat_id,"title":super::VOICE_CHAT_TITLE});
            mutate(&engine, create).await?;
            mutate(&engine, rename).await
        };
        let (client, created) = tokio::join!(engine.media_client(), created);
        created?;
        client.map_err(|_| VoiceRejection::Protocol)
    };
    let owner_client = tokio::select! {biased;
        _ = cancel.cancelled() => return Ok(()),
        result = prepared => result?,
    };
    stage("conversation ready", &mut since);
    let lease: VoiceLease = tokio::select! {biased;_=cancel.cancelled()=>return Ok(()),result=tokio::time::timeout(Duration::from_secs(60),engine.client().call_as(methods::START_VOICE,serde_json::to_value(request).unwrap()))=>result.map_err(|_|VoiceRejection::Protocol)?.map_err(rejection)?};
    chat.armed = false;
    stage("call started", &mut since);
    let _provider = ProviderGuard {
        engine: engine.clone(),
        lease: lease.clone(),
    };
    let mut owner = owner_client
        .subscribe_scoped(methods::OWN_VOICE, serde_json::to_value(&lease).unwrap())
        .await
        .map_err(|_| VoiceRejection::Protocol)?;
    stage("owner attached", &mut since);
    let (results, mut result_rx) = mpsc::channel(1);
    let mut pending = false;
    // Mute I/O cannot block receipt of owner events. Dropping this guard aborts an unfinished control.
    struct Pending(Option<tokio::task::JoinHandle<()>>);
    impl Drop for Pending {
        fn drop(&mut self) {
            if let Some(t) = self.0.take() {
                t.abort();
            }
        }
    }
    let mut control_task = Pending(None);
    loop {
        tokio::select! {biased;
            _=cancel.cancelled()=>break,
            _=events.closed()=>break,
            control=controls.recv(),if !pending=>match control{
                Some(super::VoiceControl::Mute(muted))=>{
                    pending=true;let engine=engine.clone();let lease=lease.clone();let results=results.clone();
                    control_task.0=Some(tokio::spawn(async move{let result=tokio::time::timeout(Duration::from_secs(8),engine.client().call(methods::MUTE_VOICE,serde_json::to_value(MuteVoice{lease,muted}).unwrap())).await;
                        let _=results.send(result.map_err(|_|VoiceRejection::Protocol).and_then(|r|r.map(|_|()).map_err(|_|VoiceRejection::Protocol))).await;
                    }));
                },None=>break,
            },
            result=result_rx.recv(),if pending=>{result.ok_or(VoiceRejection::Protocol)??;pending=false;control_task.0=None;},
            event=owner.recv()=>{
                let event:VoiceEvent=serde_json::from_value(event.ok_or(VoiceRejection::Protocol)?).map_err(|_|VoiceRejection::Protocol)?;
                let generation=match &event{VoiceEvent::Snapshot{snapshot}=>snapshot.generation,VoiceEvent::Partial{generation,..}|VoiceEvent::Levels{generation,..}|VoiceEvent::Closed{generation,..}|VoiceEvent::InvalidatePlayout{generation,..}=>*generation,VoiceEvent::Final{transcript} if transcript.session_id==lease.session_id=>lease.generation,_=>continue};
                if generation!=lease.generation{continue;}
                let closed=matches!(event,VoiceEvent::Closed{..});
                // A stalled viewport may miss meter readings, never the call:
                // only what cannot be dropped ends it when the queue is full.
                match events.try_send(event) {
                    Ok(()) => {}
                    Err(mpsc::error::TrySendError::Full(VoiceEvent::Levels { .. })) => {}
                    Err(_) => return Err(VoiceRejection::Overflow),
                }
                if closed{break;}
            }
        }
    }
    Ok(())
}

fn rejection(error: zeron_rpc::RpcError) -> VoiceRejection {
    let message = error.to_string();
    for reason in [
        VoiceRejection::NativeRuntimeUnavailable,
        VoiceRejection::ChatgptRequired,
        VoiceRejection::DeviceUnavailable,
        VoiceRejection::Busy,
        VoiceRejection::RemoteHost,
        VoiceRejection::WrongHarness,
        VoiceRejection::Unsupported,
    ] {
        if message.contains(&format!("{reason:?}")) {
            return reason;
        }
    }
    VoiceRejection::Protocol
}
