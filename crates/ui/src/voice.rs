//! The window's voice chat. It runs in a Codex conversation on this device
//! (the one it was started in, or a new one) and survives navigation between
//! threads; only an explicit end, an engine change or a provider failure
//! closes it. One at a time. No automatic reconnect or microphone resume.
use crate::orb::OrbState;
use crate::state::EngineHandle;
use gpui::{Context, Task};
use gpui_tokio::Tokio;
use zeron_proto::voice::*;
mod caption;
mod dock;
pub(crate) mod float;
mod hearing;
pub(crate) mod orb;
mod permissions;
mod remote;
mod session;
pub use caption::caption;
pub(crate) use caption::{last_words, tail};
pub use dock::{Placement, VoiceDock, VoiceDockEvent};

pub enum VoiceControl {
    Mute(bool),
}

/// Title of a conversation that began as a voice chat; transcripts never
/// trigger a titler.
pub const VOICE_CHAT_TITLE: &str = ORCHESTRATOR_CHAT_TITLE;

pub struct VoiceController {
    pub phase: VoicePhase,
    pub chat_id: Option<String>,
    pub reason: Option<VoiceRejection>,
    pub snapshot: Option<VoiceSnapshot>,
    /// What both sides have said this session, oldest first: the call
    /// screen's transcript, and the dock's caption from its last entry.
    pub utterances: std::collections::VecDeque<Utterance>,
    /// When this voice chat started, in epoch milliseconds: what the
    /// conversation records from then on belongs to the call.
    pub started_ms: i64,
    pub microphone_level: u16,
    pub speaker_level: u16,
    /// The full-window stage is presented over the shell.
    pub stage_open: bool,
    pub host_name: Option<String>,
    remote: bool,
    speaker_last_loud: Option<std::time::Instant>,
    /// Both voices, as the orb follows them.
    hearing: hearing::Hearing,
    /// When the session first became active; drives the stage's call timer.
    pub active_since: Option<std::time::Instant>,
    engine: Option<EngineHandle>,
    controls: Option<tokio::sync::mpsc::Sender<VoiceControl>>,
    epoch: u64,
    task: Option<Task<()>>,
    cancellation: tokio_util::sync::CancellationToken,
}

/// What a voice chat is doing, as far as its marks are concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceState {
    Connecting,
    Listening,
    Speaking,
    Muted,
    /// Codex is carrying out a request.
    Working,
    /// Codex is waiting on an answer.
    Asking,
    Ending,
}

/// The conversation a voice chat runs in.
pub enum VoiceTarget {
    /// An existing Codex conversation on this device: voice continues it,
    /// with its project, its model and everything said in it so far.
    Chat { chat_id: String, device_id: String },
    /// A new conversation, created for the voice chat in `space_id` (or with
    /// no project) and removed again if the voice chat never starts.
    New {
        device_id: String,
        space_id: Option<String>,
        config: zeron_proto::ChatConfig,
    },
}

/// How many utterances the call screen keeps; older ones are in the chat.
const MAX_UTTERANCES: usize = 48;

/// One thing said during the session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Utterance {
    /// The provider's item; `None` for text it did not attribute to one.
    pub item_id: Option<String>,
    /// Who said it: named when the utterance starts, confirmed by its final.
    pub role: Option<VoiceRole>,
    pub text: String,
    /// When its final segment arrived; `None` while it is being said.
    pub done: Option<std::time::Instant>,
    /// When it began, in epoch milliseconds: its place among the tool calls
    /// the conversation records while the call runs.
    pub at_ms: i64,
    /// When its latest words arrived: the caption brings out the words
    /// being said, and lets them rest when the speaker pauses.
    pub heard: Option<std::time::Instant>,
}

impl Default for VoiceController {
    fn default() -> Self {
        Self {
            phase: VoicePhase::Closed,
            chat_id: None,
            reason: None,
            snapshot: None,
            utterances: Default::default(),
            started_ms: 0,
            microphone_level: 0,
            speaker_level: 0,
            stage_open: false,
            host_name: None,
            remote: false,
            speaker_last_loud: None,
            hearing: Default::default(),
            active_since: None,
            engine: None,
            controls: None,
            epoch: 0,
            task: None,
            cancellation: tokio_util::sync::CancellationToken::new(),
        }
    }
}
impl VoiceController {
    /// A session is underway (including its preparation and native stop).
    pub fn is_live(&self) -> bool {
        matches!(self.phase, VoicePhase::Checking) || self.phase.replaces_composer()
    }

    /// Start a voice chat in `target`: a conversation the user can see,
    /// leave and come back to, like any other. Its harness must be Codex.
    pub fn start(
        &mut self,
        engine: EngineHandle,
        target: VoiceTarget,
        voice: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.cancel(cx);
        self.remote = std::env::var("ZERON_REMOTE_VOICE").as_deref() == Ok("1");
        let (chat_id, host_device_id, create, config) = match target {
            VoiceTarget::Chat { chat_id, device_id } => (chat_id, device_id, None, None),
            VoiceTarget::New {
                device_id,
                space_id,
                config,
            } => {
                let chat_id = uuid::Uuid::new_v4().to_string();
                // Without a project the engine mints a chat whose cwd is `~`.
                let create = serde_json::json!({
                    "op": "createChat",
                    "chatId": chat_id,
                    "spaceId": space_id,
                    "deviceId": device_id,
                    "config": config,
                });
                (chat_id, device_id, Some(create), Some(config))
            }
        };
        // A remote host mints the chat itself, from the configuration alone.
        self.remote &= config.is_some();
        let request = StartVoice {
            chat_id: chat_id.clone(),
            host_device_id: host_device_id.clone(),
            voice,
            worktree: None,
        };
        self.engine = Some(engine.clone());
        self.started_ms = chrono::Utc::now().timestamp_millis();
        self.phase = VoicePhase::Checking;
        self.chat_id = (!self.remote).then_some(chat_id);
        self.reason = None;
        let epoch = self.epoch;
        let cancellation = self.cancellation.clone();
        let (controls, control_rx) = tokio::sync::mpsc::channel(8);
        self.controls = Some(controls);
        let (events, mut event_rx) = tokio::sync::mpsc::channel(32);
        let remote = self.remote;
        let query = Tokio::spawn(cx, async move {
            if let Some(config) = config.filter(|_| remote) {
                remote::run(
                    engine,
                    request.host_device_id,
                    config,
                    request.voice,
                    cancellation,
                    events,
                    control_rx,
                )
                .await
            } else {
                session::run(engine, create, request, cancellation, events, control_rx).await
            }
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
                    // A provider failure already reduced to Failed keeps its reason.
                    Ok(()) if controller.phase == VoicePhase::Failed => {}
                    Ok(()) => {
                        controller.phase = VoicePhase::Closed;
                    }
                    Err(reason) => {
                        controller.phase = VoicePhase::Failed;
                        controller.reason = Some(reason);
                    }
                }
                controller.end_session();
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn reset_session(&mut self) {
        self.snapshot = None;
        self.utterances.clear();
        self.microphone_level = 0;
        self.speaker_level = 0;
        self.controls = None;
        self.task = None;
        self.stage_open = false;
        self.active_since = None;
        self.listen();
    }

    /// The microphone is listened to for the orb while a call this window
    /// runs is connected and not muted, and at no other time.
    fn listen(&mut self) {
        let open = self.phase == VoicePhase::Active && self.controls.is_some();
        self.hearing.follow(open, self.muted());
    }

    /// Both voices right now, for an orb to follow.
    pub(crate) fn heard(&self) -> orb::Voice {
        self.hearing.voice()
    }

    /// The session is over. A failure stays on the call screen it happened
    /// on, to explain itself there; any other ending closes the screen.
    fn end_session(&mut self) {
        let stage_open = self.stage_open && self.phase == VoicePhase::Failed;
        self.reset_session();
        self.stage_open = stage_open;
    }

    /// End the session (any phase). The provider stop runs in the background.
    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        self.epoch = self.epoch.wrapping_add(1);
        self.cancellation.cancel();
        self.cancellation = tokio_util::sync::CancellationToken::new();
        self.reset_session();
        self.phase = VoicePhase::Closed;
        self.chat_id = None;
        self.reason = None;
        self.engine = None;
        cx.notify();
    }

    pub fn set_stage_open(&mut self, open: bool, cx: &mut Context<Self>) {
        // The call screen also shows why a session failed, until dismissed.
        let open = open && (self.is_live() || self.phase == VoicePhase::Failed);
        if self.stage_open != open {
            self.stage_open = open;
            cx.notify();
        }
    }

    /// Forget a failure once it has been seen.
    pub fn dismiss_reason(&mut self, cx: &mut Context<Self>) {
        if self.reason.take().is_some() {
            if self.phase == VoicePhase::Failed {
                self.phase = VoicePhase::Closed;
                self.stage_open = false;
            }
            cx.notify();
        }
    }

    pub fn belongs_to(&self, engine: &EngineHandle) -> bool {
        self.engine
            .as_ref()
            .is_some_and(|e| e.same_connection(engine))
    }
    pub fn muted(&self) -> bool {
        self.snapshot.as_ref().is_some_and(|s| s.muted)
    }
    pub fn toggle_mute(&mut self, cx: &mut Context<Self>) {
        let muted = self.muted();
        if self.remote {
            if let Some(snapshot) = &mut self.snapshot {
                snapshot.muted = !muted;
            }
            self.listen();
            cx.notify();
        }
        if let Some(controls) = &self.controls {
            if controls.try_send(VoiceControl::Mute(!muted)).is_err() {
                self.cancel(cx);
                self.reason = Some(VoiceRejection::Overflow);
            }
        }
    }

    /// The device running Codex, when it is not this one.
    pub fn remote_host(&self) -> Option<&str> {
        self.host_name.as_deref().filter(|_| self.remote)
    }

    /// Where the call stands, for its sound cues. Stopping is still the
    /// state it is stopping from.
    pub(crate) fn call(&self) -> crate::sound::VoiceCall {
        use crate::sound::VoiceCall;
        match self.phase {
            VoicePhase::Active => VoiceCall::Connected,
            VoicePhase::Stopping if self.active_since.is_some() => VoiceCall::Connected,
            VoicePhase::Checking | VoicePhase::Starting | VoicePhase::Stopping => {
                VoiceCall::Connecting
            }
            VoicePhase::Failed => VoiceCall::Failed,
            VoicePhase::Closed => VoiceCall::Idle,
        }
    }

    /// Codex is waiting on an answer in the session transcript.
    pub fn awaiting(&self) -> bool {
        self.snapshot
            .as_ref()
            .is_some_and(|s| s.work == VoiceWork::AwaitingInput)
    }

    /// What the voice chat is doing right now; `None` once it is over.
    pub fn state(&self) -> Option<VoiceState> {
        Some(match self.phase {
            VoicePhase::Checking | VoicePhase::Starting => VoiceState::Connecting,
            VoicePhase::Stopping => VoiceState::Ending,
            VoicePhase::Active => match &self.snapshot {
                Some(s) if s.work == VoiceWork::AwaitingInput => VoiceState::Asking,
                Some(s) if s.muted => VoiceState::Muted,
                Some(s) if s.playing => VoiceState::Speaking,
                Some(s) if s.work == VoiceWork::Working => VoiceState::Working,
                _ => VoiceState::Listening,
            },
            VoicePhase::Closed | VoicePhase::Failed => return None,
        })
    }

    /// The same in words, for assistive technology and tooltips: on screen
    /// the state is always a mark, never a caption.
    pub fn status(&self) -> &'static str {
        match self.phase {
            VoicePhase::Checking => "Getting ready…",
            VoicePhase::Starting => "Connecting…",
            VoicePhase::Stopping => "Ending…",
            VoicePhase::Active => match &self.snapshot {
                Some(s) if s.work == VoiceWork::AwaitingInput => "Codex is asking you something",
                Some(s) if s.muted => "Muted",
                Some(s) if s.playing => "Speaking",
                Some(s) if s.work == VoiceWork::Working => "Working…",
                _ => "Listening",
            },
            VoicePhase::Closed | VoicePhase::Failed => "",
        }
    }

    /// The utterance being said, or the last one said.
    pub fn current(&self) -> Option<&Utterance> {
        self.utterances.back().filter(|u| !u.text.trim().is_empty())
    }

    /// The current utterance is still being said, or finished within `hold`.
    pub fn caption_fresh(&self, hold: std::time::Duration) -> bool {
        self.current()
            .is_some_and(|u| u.done.is_none_or(|at| at.elapsed() < hold))
    }

    /// The utterance for `item`, opened at the end of the transcript if this
    /// is the first the session hears of it.
    fn utterance(&mut self, item: &Option<String>) -> &mut Utterance {
        let index = match self.utterances.iter().rposition(|u| &u.item_id == item) {
            // Text without an item only ever continues the newest utterance.
            Some(index) if item.is_some() || index + 1 == self.utterances.len() => index,
            _ => {
                if self.utterances.len() == MAX_UTTERANCES {
                    self.utterances.pop_front();
                }
                self.utterances.push_back(Utterance {
                    item_id: item.clone(),
                    role: None,
                    text: String::new(),
                    done: None,
                    at_ms: chrono::Utc::now().timestamp_millis(),
                    heard: None,
                });
                self.utterances.len() - 1
            }
        };
        &mut self.utterances[index]
    }

    pub fn orb_state(&self) -> OrbState {
        orb_state(self.phase, self.snapshot.as_ref())
    }

    /// Normalized 0…1 microphone loudness; zero while muted.
    pub fn microphone_level(&self) -> f32 {
        if self.muted() {
            return 0.0;
        }
        f32::from(self.microphone_level) / f32::from(u16::MAX)
    }

    /// Normalized 0…1 speaker peak; the orb smooths each channel separately.
    pub fn speaker_level(&self) -> f32 {
        f32::from(self.speaker_level) / f32::from(u16::MAX)
    }

    pub fn reduce(&mut self, event: VoiceEvent, cx: &mut Context<Self>) {
        match event {
            VoiceEvent::Snapshot { mut snapshot } => {
                if self.remote {
                    if let Some(previous) = &self.snapshot {
                        snapshot.playing = previous.playing;
                    }
                }
                if self.remote && self.chat_id.is_none() {
                    self.chat_id = Some(snapshot.chat_id.clone());
                }

                if self.chat_id.as_deref() != Some(snapshot.chat_id.as_str()) {
                    return;
                }
                if self.snapshot.as_ref().is_some_and(|old| {
                    old.generation != snapshot.generation || old.session_id != snapshot.session_id
                }) {
                    return;
                }
                self.phase = snapshot.phase;
                if self.phase == VoicePhase::Active && self.active_since.is_none() {
                    self.active_since = Some(std::time::Instant::now());
                    // From the press to a call the user can talk in.
                    tracing::info!(
                        target: "zeron_ui::voice",
                        ms = chrono::Utc::now().timestamp_millis() - self.started_ms,
                        "voice connected"
                    );
                }
                self.snapshot = Some(snapshot);
            }
            VoiceEvent::Partial {
                generation,
                item_id,
                text,
                role,
            } if self
                .snapshot
                .as_ref()
                .is_some_and(|s| s.generation == generation) =>
            {
                let utterance = self.utterance(&item_id);
                if role.is_some() {
                    utterance.role = role;
                }
                if utterance.text.len() + text.len() <= MAX_TRANSCRIPT_BYTES {
                    if !text.is_empty() {
                        utterance.heard = Some(std::time::Instant::now());
                    }
                    utterance.text.push_str(&text);
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
                self.hearing.levels(
                    self.microphone_level(),
                    self.speaker_level(),
                    std::time::Instant::now(),
                );
                if self.remote {
                    if let Some(snapshot) = &mut self.snapshot {
                        let threshold = if snapshot.playing { 328 } else { 655 };
                        if speaker >= threshold {
                            snapshot.playing = true;
                            self.speaker_last_loud = Some(std::time::Instant::now());
                        } else if self
                            .speaker_last_loud
                            .is_none_or(|t| t.elapsed() >= std::time::Duration::from_millis(250))
                        {
                            snapshot.playing = false;
                        }
                    }
                }
            }
            // The final segment settles its utterance in place; the canonical
            // copy is already in the chat's transcript.
            VoiceEvent::Final { transcript }
                if self
                    .snapshot
                    .as_ref()
                    .is_some_and(|s| s.session_id == transcript.session_id) =>
            {
                let utterance = self.utterance(&Some(transcript.item_id));
                utterance.text = transcript.text;
                utterance.role = Some(transcript.role);
                utterance.done = Some(std::time::Instant::now());
            }
            VoiceEvent::Closed { generation, reason }
                if self
                    .snapshot
                    .as_ref()
                    .is_some_and(|s| s.generation == generation) =>
            {
                self.phase = if reason.is_some() {
                    VoicePhase::Failed
                } else {
                    VoicePhase::Closed
                };
                self.reason = reason;
                self.end_session();
            }
            _ => return,
        }
        self.listen();
        cx.notify();
    }
    pub fn reason_text(&self) -> &'static str {
        match self.reason {
            Some(VoiceRejection::CreditExclusionUnverified) => {
                "Codex controls subscription usage and any enabled additional credits."
            }
            Some(VoiceRejection::WrongHarness) => "Voice requires Codex.",
            Some(VoiceRejection::RemoteHost) => "Voice runs on this device only.",
            Some(VoiceRejection::IncludedUsageUnavailable) => {
                "Codex usage is currently unavailable."
            }
            Some(VoiceRejection::AudioFormatUnverified) => {
                "This Codex voice format has not been verified."
            }
            Some(VoiceRejection::ChatgptRequired) => "Sign in to Codex with ChatGPT to use voice.",
            // Remote calls run Zeron's bundled audio runtime, not the Codex CLI's.
            Some(VoiceRejection::NativeRuntimeUnavailable) if self.remote => {
                "This Zeron build's voice runtime couldn't start. Reinstall Zeron."
            }
            Some(VoiceRejection::NativeRuntimeUnavailable) => {
                "Update the standalone Codex installation to include its native voice runtime."
            }
            Some(VoiceRejection::DeviceUnavailable) => {
                "Check microphone permission and your audio devices."
            }
            Some(VoiceRejection::MicrophonePermissionDenied) => {
                "Allow Zeron to use the microphone in System Settings → Privacy & Security → Microphone."
            }
            Some(VoiceRejection::MicrophoneMetadataMissing) => {
                "Microphone setup is missing. Rebuild or reinstall Zeron, then restart it."
            }
            Some(VoiceRejection::Busy) => "Another window already owns the voice session.",
            Some(_) => "Voice could not connect. Try again in a moment.",
            None => "",
        }
    }
}

/// Call-timer text: `m:ss`, or `h:mm:ss` past the hour.
pub fn format_elapsed(seconds: u64) -> String {
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// Shared with the mobile apps so every orb reads the call the same way.
pub use zeron_voice_session::orb_state;

impl Drop for VoiceController {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::AppContext;

    fn snapshot() -> VoiceSnapshot {
        VoiceSnapshot {
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
        }
    }

    #[gpui::test]
    fn voice_reducer_ignores_stale_and_foreign_events(cx: &mut gpui::TestAppContext) {
        let voice = cx.new(|_| VoiceController::default());
        voice.update(cx, |voice, cx| {
            voice.chat_id = Some("chat".into());
            let snapshot = snapshot();
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
                    role: None,
                },
                cx,
            );
            assert!(voice.utterances.is_empty());
            voice.reduce(
                VoiceEvent::Partial {
                    generation: 2,
                    item_id: None,
                    text: "current".into(),
                    role: None,
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
            assert!(voice.is_live());
            voice.set_stage_open(true, cx);
            assert!(voice.stage_open);
            voice.cancel(cx);
            assert!(!voice.is_live());
            assert!(!voice.stage_open);
            assert!(voice.utterances.is_empty());
            // A closed session never reopens the stage.
            voice.set_stage_open(true, cx);
            assert!(!voice.stage_open);
        });
    }

    #[gpui::test]
    fn provider_failure_explains_itself_on_the_call_screen_until_dismissed(
        cx: &mut gpui::TestAppContext,
    ) {
        let voice = cx.new(|_| VoiceController::default());
        voice.update(cx, |voice, cx| {
            voice.chat_id = Some("chat".into());
            voice.reduce(
                VoiceEvent::Snapshot {
                    snapshot: snapshot(),
                },
                cx,
            );
            voice.set_stage_open(true, cx);
            voice.reduce(
                VoiceEvent::Closed {
                    generation: 2,
                    reason: Some(VoiceRejection::DeviceUnavailable),
                },
                cx,
            );
            assert_eq!(voice.phase, VoicePhase::Failed);
            assert!(!voice.is_live() && voice.stage_open);
            assert!(!voice.reason_text().is_empty());
            voice.dismiss_reason(cx);
            assert_eq!(voice.phase, VoicePhase::Closed);
            assert!(!voice.stage_open);

            // A session that fails with its screen set aside leaves it there.
            voice.chat_id = Some("chat".into());
            voice.reduce(
                VoiceEvent::Snapshot {
                    snapshot: snapshot(),
                },
                cx,
            );
            voice.reduce(
                VoiceEvent::Closed {
                    generation: 2,
                    reason: Some(VoiceRejection::DeviceUnavailable),
                },
                cx,
            );
            assert_eq!(voice.phase, VoicePhase::Failed);
            assert!(!voice.stage_open);
            // An ordinary ending always closes the screen.
            voice.dismiss_reason(cx);
            voice.reduce(
                VoiceEvent::Snapshot {
                    snapshot: snapshot(),
                },
                cx,
            );
            voice.set_stage_open(true, cx);
            voice.reduce(
                VoiceEvent::Closed {
                    generation: 2,
                    reason: None,
                },
                cx,
            );
            assert!(!voice.stage_open);
        });
    }

    #[test]
    fn call_timer_reads_like_a_call() {
        assert_eq!(format_elapsed(0), "0:00");
        assert_eq!(format_elapsed(75), "1:15");
        assert_eq!(format_elapsed(3600 + 62), "1:01:02");
    }

    #[test]
    fn visual_state_keeps_muting_and_playback_orthogonal() {
        let mut s = snapshot();
        s.muted = true;
        s.playing = true;
        assert_eq!(orb_state(s.phase, Some(&s)), OrbState::Composing);
        s.playing = false;
        assert_eq!(orb_state(s.phase, Some(&s)), OrbState::Breathing);
        s.muted = false;
        assert_eq!(orb_state(s.phase, Some(&s)), OrbState::Listening);
        s.work = VoiceWork::Working;
        assert_eq!(orb_state(s.phase, Some(&s)), OrbState::Working);
        s.work = VoiceWork::AwaitingInput;
        assert_eq!(orb_state(s.phase, Some(&s)), OrbState::Solving);
        assert_eq!(
            orb_state(VoicePhase::Starting, Some(&s)),
            OrbState::Connecting
        );
    }
}
