//! Ephemeral voice bridge for the *same* app-server process as text/MCP.
//! Start is fail-closed until provider-enforced included-only usage is verified.
use crate::jsonrpc::{Incoming, RpcClient};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use zeron_proto::voice::*;

pub enum VoiceCommand {
    Start {
        voice: Option<String>,
        generation: u64,
        reply: oneshot::Sender<Result<(), VoiceRejection>>,
    },
    Append {
        frame: VoiceFrame,
        reply: oneshot::Sender<Result<(), VoiceRejection>>,
    },
    Stop {
        reply: oneshot::Sender<Result<(), VoiceRejection>>,
    },
}
pub struct RealtimeControls {
    pub commands: mpsc::Receiver<VoiceCommand>,
    pub events: mpsc::Sender<VoiceEvent>,
}
#[derive(Clone)]
pub struct RealtimeHandle {
    pub commands: mpsc::Sender<VoiceCommand>,
}
pub fn channel() -> (RealtimeHandle, RealtimeControls, mpsc::Receiver<VoiceEvent>) {
    let (commands, command_rx) = mpsc::channel(MEDIA_QUEUE_FRAMES);
    let (events, event_rx) = mpsc::channel(32);
    (
        RealtimeHandle { commands },
        RealtimeControls {
            commands: command_rx,
            events,
        },
        event_rx,
    )
}

/// Drop cancels the bridge, while the session task remains responsible for
/// reaping its subprocess. Voice stop never cancels a delegated text turn.
pub(crate) struct BridgeTask(tokio::task::JoinHandle<()>);
impl Drop for BridgeTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(crate) fn attach(client: RpcClient, thread: String, controls: RealtimeControls) -> BridgeTask {
    let (mut wire, overflow) = client.subscribe_voice();
    BridgeTask(tokio::spawn(async move {
        let RealtimeControls {
            mut commands,
            events,
        } = controls;
        // No runtime-config switch can authorize credit spending.
        let mut active = false;
        let generation = 0;
        let mut sequence = 0;
        let mut timer = tokio::time::interval(Duration::from_millis(100));
        loop {
            tokio::select! {
                _ = events.closed() => break,
                _ = timer.tick() => {
                    if overflow.load(Ordering::Acquire) || client.is_closed() { break; }
                }
                command = commands.recv() => match command {
                    Some(VoiceCommand::Start { reply, .. }) => {
                        let _ = reply.send(Err(VoiceRejection::CreditExclusionUnverified));
                    }
                    Some(VoiceCommand::Append { frame, reply }) => {
                        let result = if active && frame.generation == generation {
                            append(&client, &thread, frame).await
                        } else { Err(VoiceRejection::StaleGeneration) };
                        let _ = reply.send(result);
                    }
                    Some(VoiceCommand::Stop { reply }) => {
                        let result = if active { stop(&client, &thread).await } else { Ok(()) };
                        active = false;
                        let _ = reply.send(result);
                    }
                    None => break,
                },
                notification = wire.recv() => match notification {
                    Some(Incoming::Notification { method, params }) if active => {
                        if let Some(event) = normalize(&thread, generation, &mut sequence, &method, &params) {
                            if events.try_send(event).is_err() { break; }
                        }
                        if method == "thread/realtime/closed" { active = false; }
                    }
                    Some(_) => {},
                    None => break,
                }
            }
        }
        if active {
            let _ = stop(&client, &thread).await;
        }
        let _ = events.try_send(VoiceEvent::Closed {
            generation,
            reason: Some(VoiceRejection::Protocol),
        });
    }))
}

async fn stop(client: &RpcClient, thread: &str) -> Result<(), VoiceRejection> {
    tokio::time::timeout(
        Duration::from_secs(2),
        client.request("thread/realtime/stop", json!({"threadId":thread})),
    )
    .await
    .map_err(|_| VoiceRejection::Protocol)?
    .map_err(|_| VoiceRejection::Protocol)?;
    Ok(())
}
async fn append(client: &RpcClient, thread: &str, frame: VoiceFrame) -> Result<(), VoiceRejection> {
    if frame.data.len() > MAX_AUDIO_BYTES.div_ceil(3) * 4 {
        return Err(VoiceRejection::Overflow);
    }
    let bytes = STANDARD
        .decode(&frame.data)
        .map_err(|_| VoiceRejection::Protocol)?;
    if !frame.format.validate(bytes.len()) {
        return Err(VoiceRejection::Protocol);
    }
    tokio::time::timeout(
        Duration::from_secs(2),
        client.request_media(
            "thread/realtime/appendAudio",
            json!({
                "threadId":thread, "audio":{"data":frame.data,"sampleRate":frame.format.sample_rate,
                "numChannels":frame.format.channels,"samplesPerChannel":bytes.len()/2}
            }),
        ),
    )
    .await
    .map_err(|_| VoiceRejection::Protocol)?
    .map_err(|_| VoiceRejection::Protocol)?;
    Ok(())
}

/// Canonical completed items only. Legacy transcript/done has no stable item
/// identity and is deliberately not committed alongside the canonical stream.
pub(crate) fn normalize(
    thread: &str,
    generation: u64,
    sequence: &mut u64,
    method: &str,
    p: &Value,
) -> Option<VoiceEvent> {
    if p.get("threadId")?.as_str()? != thread {
        return None;
    }
    match method {
        "thread/realtime/item/completed" => {
            let item = p.get("item")?;
            if item.get("type")?.as_str()? != "transcriptSegment" {
                return None;
            }
            let text = item.get("text")?.as_str()?;
            if text.len() > MAX_TRANSCRIPT_BYTES {
                return None;
            }
            let role = match item.get("role")?.as_str()? {
                "user" => VoiceRole::User,
                "assistant" => VoiceRole::Assistant,
                _ => return None,
            };
            Some(VoiceEvent::Final {
                transcript: VoiceTranscript {
                    session_id: item.get("realtimeSessionId")?.as_str()?.to_owned(),
                    item_id: item.get("id")?.as_str()?.to_owned(),
                    role,
                    text: text.to_owned(),
                    promoted_message_id: None,
                },
            })
        }
        "thread/realtime/outputAudio/delta" => {
            let a = p.get("audio")?;
            let data = a.get("data")?.as_str()?;
            if data.len() > MAX_AUDIO_BYTES.div_ceil(3) * 4 {
                return None;
            }
            let format = VoiceFormat {
                encoding: VoiceEncoding::Pcm16Le,
                sample_rate: u32::try_from(a.get("sampleRate")?.as_u64()?).ok()?,
                channels: u16::try_from(a.get("numChannels")?.as_u64()?).ok()?,
            };
            if !format.validate(STANDARD.decode(data).ok()?.len()) {
                return None;
            }
            *sequence += 1;
            Some(VoiceEvent::Audio {
                frame: VoiceFrame {
                    generation,
                    sequence: *sequence,
                    format,
                    data: data.to_owned(),
                    item_id: a.get("itemId").and_then(Value::as_str).map(str::to_owned),
                },
            })
        }
        "thread/realtime/transcript/delta" | "thread/realtime/item/transcript/delta" => {
            let text = p.get("delta")?.as_str()?;
            (text.len() <= MAX_TRANSCRIPT_BYTES).then(|| VoiceEvent::Partial {
                generation,
                text: text.to_owned(),
            })
        }
        "thread/realtime/closed" | "thread/realtime/error" => Some(VoiceEvent::Closed {
            generation,
            reason: (method.ends_with("error")).then_some(VoiceRejection::Protocol),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_items_do_not_duplicate_legacy_or_foreign_threads() {
        let p = json!({"threadId":"parent", "item":{"type":"transcriptSegment", "id":"item", "realtimeSessionId":"voice", "role":"user", "text":"hello"}});
        let mut seq = 0;
        assert!(matches!(
            normalize("parent", 1, &mut seq, "thread/realtime/item/completed", &p),
            Some(VoiceEvent::Final { .. })
        ));
        assert!(normalize("other", 1, &mut seq, "thread/realtime/item/completed", &p).is_none());
        assert!(normalize("parent", 1, &mut seq, "thread/realtime/transcript/done", &p).is_none());
    }
}
