//! Viewport-owned voice controller. No automatic reconnect or microphone resume.
use crate::state::EngineHandle;
use gpui::{Context, Task};
use gpui_tokio::Tokio;
use zeron_proto::voice::*;
mod session;

pub enum VoiceControl {
    Mute(bool),
}

pub struct VoiceController {
    pub phase: VoicePhase,
    pub chat_id: Option<String>,
    pub reason: Option<VoiceRejection>,
    pub snapshot: Option<VoiceSnapshot>,
    pub partial: String,
    pub microphone_level: u16,
    pub speaker_level: u16,
    partial_item: Option<String>,
    engine: Option<EngineHandle>,
    controls: Option<tokio::sync::mpsc::Sender<VoiceControl>>,
    epoch: u64,
    task: Option<Task<()>>,
    cancellation: tokio_util::sync::CancellationToken,
}
impl Default for VoiceController {
    fn default() -> Self {
        Self {
            phase: VoicePhase::Closed,
            chat_id: None,
            reason: None,
            snapshot: None,
            partial: String::new(),
            microphone_level: 0,
            speaker_level: 0,
            partial_item: None,
            engine: None,
            controls: None,
            epoch: 0,
            task: None,
            cancellation: tokio_util::sync::CancellationToken::new(),
        }
    }
}
impl VoiceController {
    pub fn begin(&mut self, engine: EngineHandle, request: StartVoice, cx: &mut Context<Self>) {
        self.cancel(cx);
        self.engine = Some(engine.clone());
        self.phase = VoicePhase::Checking;
        self.chat_id = Some(request.chat_id.clone());
        self.reason = None;
        let epoch = self.epoch;
        let cancellation = self.cancellation.clone();
        let (controls, control_rx) = tokio::sync::mpsc::channel(8);
        self.controls = Some(controls);
        let (events, mut event_rx) = tokio::sync::mpsc::channel(32);
        let query = Tokio::spawn(cx, async move {
            session::run(engine, request, cancellation, events, control_rx).await
        });
        self.task = Some(cx.spawn(async move |this, cx| {
            let receive = async {
                while let Some(event) = event_rx.recv().await {
                    if this
                        .update(cx, |controller, cx| {
                            if controller.epoch == epoch {
                                controller.reduce(event, cx);
                            }
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            };
            receive.await;
            let result = query.await.unwrap_or(Err(VoiceRejection::Protocol));
            let _ = this.update(cx, |controller, cx| {
                if controller.epoch != epoch {
                    return;
                }
                match result {
                    Ok(()) => {
                        controller.phase = VoicePhase::Closed;
                    }
                    Err(reason) => {
                        controller.phase = VoicePhase::Failed;
                        controller.reason = Some(reason);
                    }
                }
                controller.snapshot = None;
                controller.partial.clear();
                controller.controls = None;
                controller.task = None;
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        self.epoch = self.epoch.wrapping_add(1);
        self.cancellation.cancel();
        self.cancellation = tokio_util::sync::CancellationToken::new();
        self.task = None;
        self.controls = None;
        self.phase = VoicePhase::Closed;
        self.snapshot = None;
        self.partial.clear();
        self.partial_item = None;
        self.microphone_level = 0;
        self.speaker_level = 0;
        self.chat_id = None;
        self.reason = None;
        self.engine = None;
        cx.notify();
    }
    pub fn belongs_to(&self, engine: &EngineHandle) -> bool {
        self.engine
            .as_ref()
            .is_some_and(|e| e.same_connection(engine))
    }
    pub fn toggle_mute(&mut self, cx: &mut Context<Self>) {
        let muted = self.snapshot.as_ref().is_some_and(|s| s.muted);
        if let Some(controls) = &self.controls {
            if controls.try_send(VoiceControl::Mute(!muted)).is_err() {
                self.cancel(cx);
                self.reason = Some(VoiceRejection::Overflow);
            }
        }
    }
    pub fn reduce(&mut self, event: VoiceEvent, cx: &mut Context<Self>) {
        match event {
            VoiceEvent::Snapshot { snapshot } => {
                if self.chat_id.as_deref() != Some(snapshot.chat_id.as_str()) {
                    return;
                }
                if self.snapshot.as_ref().is_some_and(|old| {
                    old.generation != snapshot.generation || old.session_id != snapshot.session_id
                }) {
                    return;
                }
                self.phase = snapshot.phase;
                self.snapshot = Some(snapshot);
            }
            VoiceEvent::Partial {
                generation,
                item_id,
                text,
            } if self
                .snapshot
                .as_ref()
                .is_some_and(|s| s.generation == generation) =>
            {
                if self.partial_item != item_id {
                    self.partial.clear();
                    self.partial_item = item_id;
                }
                if self.partial.len() + text.len() <= MAX_TRANSCRIPT_BYTES {
                    self.partial.push_str(&text);
                } else {
                    self.cancel(cx);
                    self.phase = VoicePhase::Failed;
                    self.reason = Some(VoiceRejection::Overflow);
                }
            }
            VoiceEvent::Levels {
                generation,
                microphone,
                speaker,
            } if self
                .snapshot
                .as_ref()
                .is_some_and(|s| s.generation == generation) =>
            {
                self.microphone_level = microphone;
                self.speaker_level = speaker;
            }
            VoiceEvent::Final { transcript }
                if self
                    .snapshot
                    .as_ref()
                    .is_some_and(|s| s.session_id == transcript.session_id) =>
            {
                self.partial.clear();
                self.partial_item = None;
            }
            VoiceEvent::Closed { generation, reason }
                if self
                    .snapshot
                    .as_ref()
                    .is_some_and(|s| s.generation == generation) =>
            {
                self.phase = VoicePhase::Closed;
                self.reason = reason;
                self.partial.clear();
                self.snapshot = None;
            }
            _ => return,
        }
        cx.notify();
    }
    pub fn reason_text(&self) -> &'static str {
        match self.reason {
            Some(VoiceRejection::CreditExclusionUnverified) => {
                "Codex controls subscription usage and any enabled additional credits."
            }
            Some(VoiceRejection::WrongHarness) => "Voice requires a Codex chat.",
            Some(VoiceRejection::RemoteHost) => "Voice requires a chat hosted on this device.",
            Some(VoiceRejection::IncludedUsageUnavailable) => {
                "Codex usage is currently unavailable."
            }
            Some(VoiceRejection::AudioFormatUnverified) => {
                "This Codex voice format has not been verified."
            }
            Some(VoiceRejection::ChatgptRequired) => "Sign in to Codex with ChatGPT to use voice.",
            Some(VoiceRejection::NativeRuntimeUnavailable) => {
                "Update the standalone Codex installation to include its native voice runtime."
            }
            Some(VoiceRejection::DeviceUnavailable) => {
                "Check microphone permission and your audio devices."
            }
            Some(VoiceRejection::Busy) => "Another window already owns the voice session.",
            Some(_) => "Voice could not connect. You can continue typing.",
            None => "",
        }
    }
}

impl Drop for VoiceController {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::AppContext;
    #[gpui::test]
    fn voice_reducer_ignores_stale_and_foreign_events(cx: &mut gpui::TestAppContext) {
        let voice = cx.new(|_| VoiceController::default());
        voice.update(cx, |voice, cx| {
            voice.chat_id = Some("chat".into());
            let snapshot = VoiceSnapshot {
                session_id: "one".into(),
                chat_id: "chat".into(),
                generation: 2,
                phase: VoicePhase::Active,
                muted: false,
                playing: false,
                work: VoiceWork::Idle,
                reason: None,
                voice: None,
                voices: Vec::new(),
            };
            voice.reduce(
                VoiceEvent::Snapshot {
                    snapshot: snapshot.clone(),
                },
                cx,
            );
            voice.reduce(
                VoiceEvent::Partial {
                    generation: 1,
                    item_id: None,
                    text: "old".into(),
                },
                cx,
            );
            assert!(voice.partial.is_empty());
            voice.reduce(
                VoiceEvent::Partial {
                    generation: 2,
                    item_id: None,
                    text: "current".into(),
                },
                cx,
            );
            let mut foreign = snapshot;
            foreign.chat_id = "other".into();
            foreign.generation = 3;
            voice.reduce(VoiceEvent::Snapshot { snapshot: foreign }, cx);
            assert_eq!(voice.snapshot.as_ref().unwrap().generation, 2);
            voice.reduce(
                VoiceEvent::Closed {
                    generation: 1,
                    reason: None,
                },
                cx,
            );
            assert!(voice.phase.replaces_composer());
            voice.cancel(cx);
            assert!(!voice.phase.replaces_composer());
            assert!(voice.partial.is_empty());
        });
    }
}
