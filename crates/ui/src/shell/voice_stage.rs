//! The call screen: a voice chat's conversation while it is live. Voice runs
//! in a conversation like any other (the open one, or a new one), and while
//! it does that conversation's view is this screen: the orb, what both sides
//! have said, and the controls. Moving to another thread leaves it without
//! ending voice, with the compact row (`crate::voice::VoiceDock`) above that
//! thread's composer as the way back.

use super::*;
use zeron_proto::voice::{VoicePhase, VoiceRole};

/// The orb placeholder's diameter, by how much room the window leaves it.
const STAGE_ORB_MIN: f32 = 104.0;
const STAGE_ORB_MAX: f32 = 176.0;
/// The orb's diameter, as a share of the room between the title bar and the
/// controls.
const STAGE_ORB_SHARE: f32 = 0.24;
/// The orb and what is said under it are one group. With a usual turn under
/// it (`STAGE_GROUP_LINES` lines of words) the group stands this far down
/// that room: below the middle, so the artwork has the upper half of the
/// screen to itself and the group sits nearer the controls it belongs with.
/// The orb holds that place as the turn grows: a longer one takes the room
/// under it.
const STAGE_GROUP_LINES: f32 = 3.0;
const STAGE_GROUP_AT: f32 = 0.6;
/// The orb's slot, in diameters: room for its halo. What stands in for the
/// orb (the loader, a failure) needs at least `STAGE_SLOT_MIN`.
const STAGE_ORB_SLOT: f32 = 1.08;
const STAGE_SLOT_MIN: f32 = 116.0;
const STAGE_FAILURE_SLOT: f32 = 216.0;
/// The call screen's way in, on one linear clock: how long it takes, the
/// share of that clock each of its parts arrives over, and the size the orb
/// starts from, as a share of its own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Entrance {
    ms: f32,
    /// What the screen replaces, leaving.
    beneath: (f32, f32),
    artwork: (f32, f32),
    orb: (f32, f32),
    controls: (f32, f32),
    orb_from: f32,
}

/// Coming back to a call already under way: what the screen replaces leaves
/// first and quickly, the orb arrives, and the controls follow it by about a
/// tenth of a second. With no artwork already in view, the artwork comes in
/// from the start.
const ARRIVING: Entrance = Entrance {
    ms: 420.0,
    beneath: (0.0, 0.4),
    artwork: (0.0, 0.6),
    orb: (0.1, 0.8),
    controls: (0.3, 1.0),
    orb_from: 1.0,
};
/// Placing a call: Codex takes two to three seconds to answer, and the
/// screen spends the first of them arriving. The page leaves at once, the
/// orb grows out of half its size as its glass and its seed of light appear
/// (the seed goes on growing until the call connects, so the arrival and the
/// wait are one movement), and the controls come last.
const PLACING: Entrance = Entrance {
    ms: 1600.0,
    beneath: (0.0, 0.12),
    artwork: (0.0, 0.3),
    orb: (0.06, 0.85),
    controls: (0.62, 1.0),
    orb_from: 0.5,
};
const STAGE_EXIT_MS: f32 = 220.0;
/// How far a part travels as it arrives.
const STAGE_PART_TRAVEL: f32 = 12.0;
/// The orb coming alive once the call connects. Short: by then the user has
/// waited for the call, and nothing should stand between it and them.
const STAGE_ORB_ENTER_MS: f32 = 360.0;
/// The controls: the row's height, its distance from the window's bottom
/// edge, and the clear space above it. The controls are a group of their
/// own: what is said never comes closer to them than the row is from the
/// window's edge and then some.
const STAGE_BAR_HEIGHT: f32 = 44.0;
const STAGE_BAR_BOTTOM: f32 = 24.0;
const STAGE_BAR_CLEARANCE: f32 = 32.0;
/// Where the call runs, beside the controls: the row's height.
const STAGE_CONTEXT_HEIGHT: f32 = 20.0;
/// Its distance from the screen's leading edge, the room it leaves the
/// controls, and the least width worth showing it in.
const STAGE_CONTEXT_INSET: f32 = 20.0;
const STAGE_CONTEXT_CLEARANCE: f32 = 132.0;
const STAGE_CONTEXT_MIN: f32 = 120.0;
/// The feed under the orb: a subtitle's measure. What is said is one block
/// of text, both voices set alike on one grid of lines and told apart by
/// their ink (the user's a step quieter than Codex's). They share one room
/// of `WORDS_LINES` lines: what Codex says takes four of them at most and
/// what the user asked two, so a long answer moves the question out through
/// the top instead of holding it in a room of its own.
const CAPTION_WIDTH: f32 = 560.0;
const WORDS_SIZE: f32 = 18.0;
const WORDS_LINE: f32 = 28.0;
const WORDS_LINES: f32 = 5.0;
const CAPTION_LINES: f32 = 4.0;
const ASKED_LINES: f32 = 2.0;
const CAPTION_CHARS: usize = 420;
/// The newest words stand out while they are being said…
const CAPTION_WORDS: usize = 3;
/// …and rest once the speaker has paused this long.
const CAPTION_WORD_HOLD: std::time::Duration = std::time::Duration::from_millis(1200);
/// The feed stays this long after its turn's last sign of life, then fades
/// away as one over the last stretch.
const FEED_LINGER: std::time::Duration = std::time::Duration::from_secs(10);
const FEED_FADE: std::time::Duration = std::time::Duration::from_millis(700);
/// Space between the orb's slot and the feed, and round what Codex is doing
/// when it stands between the two voices.
const CAPTION_GAP: f32 = 12.0;
const ACTIVITY_GAP: f32 = 6.0;
/// What Codex is doing: how many of the turn's latest calls show, a line's
/// height, and how long the list takes to make room for one more.
const ACTIVITY_SHOWN: usize = 3;
const ACTIVITY_LINE: f32 = crate::transcript::CALL_LINE_HEIGHT;
const ACTIVITY_EASE_MS: f32 = 240.0;
/// How far back in the conversation's record a call looks for them.
const ACTIVITY_SCAN: usize = 24;
/// The resolution the orb's fluid is computed at: it is soft, and loses
/// nothing to being drawn small and stretched.
const STAGE_ORB_RESOLUTION: u32 = 64;

/// The call screen's progress into view, 0–1: `from`, moved towards `open`
/// for `dt_ms`. The clock is linear, so a screen asked to turn back mid-way
/// does it from where it is instead of jumping to an end first. One frame
/// counts for [`STEP_MS`] at most: a frame that took long (the page under
/// the screen being built, the call starting) holds the motion up instead
/// of swallowing it, so what follows the hitch is the rest of the
/// transition and not its last frame.
pub(super) fn stage_progress(
    from: f32,
    open: bool,
    dt_ms: f32,
    reduced: bool,
    enter_ms: f32,
) -> f32 {
    let scale = motion::speed_scale();
    let dt_ms = dt_ms.min(STEP_MS);
    match (open, reduced) {
        (true, true) => 1.0,
        (false, true) => 0.0,
        (true, false) => (from + dt_ms / (enter_ms * scale)).min(1.0),
        (false, false) => (from - dt_ms / (STAGE_EXIT_MS * scale)).max(0.0),
    }
}

/// The longest one frame counts for in the call's transitions (its screen
/// and its floating bar), so neither the time the window sat idle before a
/// change of screen nor a slow frame is motion.
const STEP_MS: f32 = 34.0;
/// A frame of a call transition this long is written to the log, with where
/// the transition was: what holds a transition up is found there.
const SLOW_FRAME_MS: f32 = 80.0;

/// How far one part of the call screen is into view at `progress`: it moves
/// over its own share of the clock, eased.
pub(super) fn stage_part(progress: f32, (start, end): (f32, f32)) -> f32 {
    motion::EASE_OUT_QUINT.eval((progress - start) / (end - start))
}

/// The orb's entrance progress for a call that connected `elapsed_ms` ago
/// (`None`: still connecting).
pub(super) fn orb_reveal(connected_ms: Option<f32>, reduced: bool) -> f32 {
    match connected_ms {
        None => 0.0,
        Some(_) if reduced => 1.0,
        Some(elapsed) => {
            motion::EASE_OUT_EXPO.eval(elapsed / (STAGE_ORB_ENTER_MS * motion::speed_scale()))
        }
    }
}

/// A turn finishing in the conversation a live call runs in is the call
/// carrying on, with Codex about to say what it did: it neither chimes nor
/// posts a banner. A question or a failure there still asks for attention.
pub(super) fn call_silences(sound: crate::sound::Sound, in_call: bool) -> bool {
    in_call && sound == crate::sound::Sound::Done
}

/// What pressing a tool line does.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum ToolAction {
    /// Opens the page it fetched, in the browser.
    Link(String),
    /// Goes to the chat it created or wrote to.
    Chat(String),
    /// Shows the call in the conversation, where its result is read.
    Conversation,
}

/// One tool call, as the call screen lists it.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ToolLine {
    key: String,
    part: String,
    call: zeron_proto::ToolCall,
    running: bool,
    failed: bool,
    action: ToolAction,
}

/// A web address a tool line may open: nothing but `http` and `https`
/// leaves the app on a model's say-so.
fn is_web(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// The chat ids written in `text`.
fn chat_ids(text: &str) -> impl Iterator<Item = &str> {
    text.as_bytes()
        .windows(36)
        .enumerate()
        .filter_map(move |(at, window)| {
            window
                .iter()
                .enumerate()
                .all(|(index, byte)| match index {
                    8 | 13 | 18 | 23 => *byte == b'-',
                    _ => byte.is_ascii_hexdigit(),
                })
                .then(|| &text[at..at + 36])
        })
}

/// The Zeron tool a call is, if it is one: a voice chat coordinates other
/// chats with them.
fn zeron_tool(call: &zeron_proto::ToolCall) -> Option<&str> {
    match call {
        zeron_proto::ToolCall::Mcp { server, tool, .. } if server.eq_ignore_ascii_case("zeron") => {
            Some(tool.as_str())
        }
        _ => None,
    }
}

/// What pressing a tool call's line does. Zeron's own tools lead to the
/// chat they concern: `made`, the chat this call created when the caller
/// could tell, else the one its result names. A fetch leads to its page.
/// Anything else is shown in the conversation.
fn tool_action(
    call: &zeron_proto::ToolCall,
    output: Option<&str>,
    made: Option<&zeron_proto::Chat>,
    chats: &[zeron_proto::Chat],
    own: Option<&str>,
) -> ToolAction {
    if let Some(tool) = zeron_tool(call) {
        let named = || {
            let one = matches!(
                tool,
                "create_chat"
                    | "send_message"
                    | "read_chat"
                    | "get_chat"
                    | "wait_for_turn"
                    | "interrupt_chat"
                    | "respond_to_input"
            );
            output
                .filter(|_| one)
                .into_iter()
                .flat_map(chat_ids)
                .filter(|id| Some(*id) != own)
                .find_map(|id| chats.iter().find(|chat| chat.id == id))
        };
        return match made.or_else(named) {
            Some(chat) => ToolAction::Chat(chat.id.clone()),
            None => ToolAction::Conversation,
        };
    }
    match call {
        zeron_proto::ToolCall::WebFetch { url, .. } if is_web(url) => ToolAction::Link(url.clone()),
        _ => ToolAction::Conversation,
    }
}

/// What Codex has been doing this turn, oldest first, and how many earlier
/// calls of the turn are left out: the tool calls `record` (the
/// conversation's transcript) gained since the user last spoke, its latest
/// few, so they stay in view while Codex reports on them; or, when there are
/// none, a call still running from before.
/// Read from the end of the record, so a long conversation costs a call no
/// more than a short one.
pub(super) fn recent_tools(
    voice: &crate::voice::VoiceController,
    record: &[zeron_doc::SessionMessageEntry],
    chats: &[zeron_proto::Chat],
) -> (Vec<ToolLine>, usize) {
    struct Part<'a> {
        entry: &'a str,
        id: &'a str,
        call: &'a zeron_proto::ToolCall,
        output: Option<&'a str>,
        resolved: bool,
        failed: bool,
    }
    let since = voice
        .utterances
        .iter()
        .rev()
        .find(|utterance| {
            utterance.role == Some(VoiceRole::User) && !utterance.text.trim().is_empty()
        })
        .map_or(voice.started_ms, |utterance| {
            utterance.at_ms.max(voice.started_ms)
        });
    let own = voice.chat_id.as_deref();
    let line = |part: &Part, made: Option<&zeron_proto::Chat>| ToolLine {
        key: format!("{}-{}", part.entry, part.id),
        part: part.id.to_owned(),
        call: part.call.clone(),
        running: !part.resolved,
        failed: part.failed,
        action: tool_action(part.call, part.output, made, chats, own),
    };
    // Newest first while scanning.
    let mut turn = Vec::new();
    let mut running = None;
    'record: for entry in record
        .iter()
        .rev()
        .take(ACTIVITY_SCAN)
        .take_while(|entry| entry.created_at >= voice.started_ms)
    {
        let trailing = entry.created_at >= since;
        for part in entry.parts.iter().rev() {
            let zeron_doc::MessagePart::Tool {
                id,
                call,
                is_error,
                resolved,
                output,
                ..
            } = part
            else {
                continue;
            };
            let part = Part {
                entry: &entry.id,
                id,
                call,
                output: output.as_deref(),
                resolved: *resolved,
                failed: *is_error,
            };
            if trailing {
                turn.push(part);
            } else if !*resolved {
                running = Some(part);
                break 'record;
            }
        }
        if !trailing && !turn.is_empty() {
            break;
        }
    }
    if turn.is_empty() {
        return (running.iter().map(|part| line(part, None)).collect(), 0);
    }
    turn.reverse();
    // A created chat's id is not in the record (a tool's result is kept as
    // a summary), so the chats this turn created are told by what appeared
    // since the turn began, in the order it created them: the voice chat's
    // side chats, and standalone chats (`kind: chat`), which name no parent.
    // A batch creates several at once and tells none apart.
    let creates = |part: &Part| zeron_tool(part.call) == Some("create_chat") && !part.failed;
    let mut made = Vec::new();
    if own.is_some()
        && turn.iter().any(creates)
        && !turn
            .iter()
            .any(|part| zeron_tool(part.call) == Some("create_chats"))
    {
        made.extend(chats.iter().filter(|chat| {
            Some(chat.id.as_str()) != own
                && chat
                    .parent_chat_id
                    .as_deref()
                    .is_none_or(|parent| Some(parent) == own)
                && !zeron_proto::voice::is_orchestrator_chat(&chat.id)
                && chat.created_at.timestamp_millis() >= since
        }));
        made.sort_by_key(|chat| chat.created_at);
    }
    let earlier = turn.len().saturating_sub(ACTIVITY_SHOWN);
    let mut created = 0;
    let lines = turn
        .iter()
        .enumerate()
        .filter_map(|(index, part)| {
            let chat = creates(part)
                .then(|| {
                    created += 1;
                    made.get(created - 1).copied()
                })
                .flatten();
            (index >= earlier).then(|| line(part, chat))
        })
        .collect();
    (lines, earlier)
}

/// Whether the call is looking something up: its latest tool call still
/// running is a search or a fetch, of the web or of the project.
fn looking_up(record: &[zeron_doc::SessionMessageEntry]) -> bool {
    use zeron_proto::ToolCall;
    record
        .iter()
        .rev()
        .take(ACTIVITY_SCAN)
        .flat_map(|entry| entry.parts.iter().rev())
        .find_map(|part| match part {
            zeron_doc::MessagePart::Tool {
                call,
                resolved: false,
                ..
            } => Some(matches!(
                call,
                ToolCall::WebSearch { .. }
                    | ToolCall::WebFetch { .. }
                    | ToolCall::Search { .. }
                    | ToolCall::Glob { .. }
            )),
            _ => None,
        })
        .unwrap_or(false)
}

/// The turn a call is in: what the user last said, and what Codex is saying,
/// or last said, since.
fn turn(
    voice: &crate::voice::VoiceController,
) -> (
    Option<&crate::voice::Utterance>,
    Option<&crate::voice::Utterance>,
) {
    let said = |utterance: &crate::voice::Utterance| !utterance.text.trim().is_empty();
    let asked = voice
        .utterances
        .iter()
        .rposition(|utterance| utterance.role == Some(VoiceRole::User) && said(utterance));
    let answer = voice
        .utterances
        .iter()
        .skip(asked.map_or(0, |at| at + 1))
        .rev()
        .find(|utterance| utterance.role != Some(VoiceRole::User) && said(utterance));
    (asked.map(|at| &voice.utterances[at]), answer)
}

/// One side's words, as the feed draws them.
struct Words {
    key: String,
    text: String,
    /// Where the words in full ink begin.
    fresh: usize,
    /// Still being said.
    live: bool,
    /// When they were last added to.
    last: Option<std::time::Instant>,
}

/// The words being said stand out and a pause lets them rest. Once finished
/// Codex's read in full ink, and the user's stay behind them: they head the
/// turn, they are not what it has to say.
fn words(utterance: &crate::voice::Utterance, user: bool) -> Words {
    let text = crate::voice::tail(&utterance.text, CAPTION_CHARS).to_owned();
    let heard = |within| utterance.heard.is_some_and(|at| at.elapsed() < within);
    let fresh = match utterance.done {
        Some(_) if user => text.len(),
        Some(_) => 0,
        None if heard(CAPTION_WORD_HOLD) => crate::voice::last_words(&text, CAPTION_WORDS),
        None => text.len(),
    };
    Words {
        key: utterance
            .item_id
            .clone()
            .unwrap_or_else(|| "words".to_owned()),
        fresh,
        live: utterance.done.is_none() && heard(FEED_LINGER),
        last: utterance.done.or(utterance.heard),
        text,
    }
}

/// `lines` of `words` at most, the newest kept: a long utterance pushes its
/// oldest line out through the top instead of growing over what follows.
fn spoken(
    id: &'static str,
    words: &Words,
    (said, bright): (gpui::Hsla, gpui::Hsla),
    lines: f32,
) -> gpui::Stateful<gpui::Div> {
    let ink = |color: gpui::Hsla| gpui::HighlightStyle {
        color: Some(color),
        ..Default::default()
    };
    let (fresh, length) = (words.fresh, words.text.len());
    let text = gpui::StyledText::new(words.text.clone()).with_highlights(
        [(0..fresh, ink(said)), (fresh..length, ink(bright))]
            .into_iter()
            .filter(|(range, _)| !range.is_empty()),
    );
    div()
        .id(id)
        .debug_selector(move || id.into())
        .flex_none()
        .w_full()
        .max_h(px(WORDS_LINE * lines))
        .overflow_hidden()
        .flex()
        .flex_col()
        .justify_end()
        .text_size(crate::typography::ui_rems(WORDS_SIZE))
        .line_height(px(WORDS_LINE))
        .child(div().flex_none().w_full().text_center().child(text))
}

/// A quiet text link in the feed: its label, and where it leads as a mark.
/// It owns its hover style, which also lifts a caller's resting opacity: an
/// element takes one hover style, and GPUI asserts on a second.
fn feed_link(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    leads: &'static str,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let label = label.into();
    let (text, accent) = (theme.text, theme.accent);
    div()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label(label.clone())
        .tab_index(0)
        .flex_none()
        .max_w_full()
        .flex()
        .items_center()
        .gap(px(6.0))
        .cursor_pointer()
        .text_color(theme.text_muted)
        // The focus ring's room is always there: showing it moves nothing.
        .border_1()
        .border_color(gpui::transparent_black())
        .rounded(px(6.0))
        .px(px(4.0))
        .hover(move |style| style.opacity(1.0).text_color(text))
        .active(|style| style.opacity(0.7))
        .focus_visible(move |style| style.border_color(accent))
        .child(div().min_w_0().truncate().child(label))
        .child(
            div()
                .flex_none()
                .child(icon(leads).size(px(11.0)).text_color(theme.text_muted)),
        )
}

/// What of a call is shown in its conversation ahead of the record: the
/// conversation it runs in, and each utterance echoed there with the length
/// of text and the finished state it was last shown with.
#[derive(Default)]
pub(super) struct VoiceEchoes {
    chat: Option<String>,
    said: std::collections::HashMap<String, (usize, bool)>,
}

/// What the call screen keeps of its feed between frames.
#[derive(Default)]
pub(super) struct VoiceFeed {
    /// The turn it shows, and that turn's tool lines as last drawn.
    turn: String,
    drawn: Vec<(String, bool)>,
    /// The tool list's height in rows: `rows`, eased from `from` since `at`,
    /// which is also when the list last changed.
    from: f32,
    rows: f32,
    at: Option<std::time::Instant>,
    /// The pointer is on the feed: it stays while it is being read.
    hovered: bool,
}

impl VoiceFeed {
    /// The tool list's height in rows at `now`, and whether it is still
    /// easing there.
    fn height(&self, reduced: bool, now: std::time::Instant) -> (f32, bool) {
        let progress = match self.at {
            Some(at) if !reduced => {
                now.saturating_duration_since(at).as_secs_f32() * 1000.0
                    / (ACTIVITY_EASE_MS * motion::speed_scale())
            }
            _ => 1.0,
        };
        if progress >= 1.0 {
            return (self.rows, false);
        }
        let eased = motion::EASE_OUT_QUINT.eval(progress);
        (self.from + (self.rows - self.from) * eased, true)
    }

    /// Follows the call to `turn` and its `lines`, `rows` tall: a new turn
    /// starts the feed over, and a change in the list eases its height from
    /// where it is. Returns [`Self::height`].
    fn follow(
        &mut self,
        turn: &str,
        lines: &[ToolLine],
        rows: usize,
        reduced: bool,
        now: std::time::Instant,
    ) -> (f32, bool) {
        if self.turn != turn {
            *self = Self {
                turn: turn.to_owned(),
                hovered: self.hovered,
                ..Self::default()
            };
        }
        let same = self.drawn.len() == lines.len()
            && self
                .drawn
                .iter()
                .zip(lines)
                .all(|((key, running), line)| *key == line.key && *running == line.running);
        if !same || self.rows != rows as f32 {
            self.from = self.height(reduced, now).0;
            self.rows = rows as f32;
            self.drawn = lines
                .iter()
                .map(|line| (line.key.clone(), line.running))
                .collect();
            self.at = Some(now);
        }
        self.height(reduced, now)
    }
}

impl Shell {
    /// The conversation a voice chat started now would run in: the open one
    /// when it is a Codex conversation on this device, which voice then
    /// continues; otherwise a new conversation in the selected project (or
    /// with none), on Codex. The new chat's model follows the composer when
    /// it already targets Codex.
    fn voice_target(&self, cx: &App) -> Option<crate::voice::VoiceTarget> {
        let state = self.state.read(cx);
        let local = state.local_device_id.clone()?;
        let remote = std::env::var("ZERON_REMOTE_VOICE").as_deref() == Ok("1");
        if !remote
            && let Some(chat) = state.selected_chat_row()
            && chat.device_id == local
            && chat.config.as_ref().map(|config| config.harness) == Some(HarnessId::Codex)
        {
            return Some(crate::voice::VoiceTarget::Chat {
                chat_id: chat.id.clone(),
                device_id: local,
            });
        }
        let config = self
            .composer
            .read(cx)
            .pickers()
            .read(cx)
            .resolved(cx)
            .chat_config()
            .filter(|config| config.harness == HarnessId::Codex)
            .unwrap_or_else(|| ChatConfig {
                harness: HarnessId::Codex,
                model: None,
                reasoning: None,
                model_options: Default::default(),
                sandbox: zeron_proto::SandboxLevel::WorkspaceWrite,
            });
        if remote {
            return Some(crate::voice::VoiceTarget::New {
                device_id: settings::current(cx).codex_voice_device.unwrap_or(local),
                space_id: None,
                config,
            });
        }
        // Voice runs on this device, so only a project that lives here.
        let space_id = state
            .selected_space_row()
            .filter(|space| space.device_id == local)
            .map(|space| space.id.clone());
        Some(crate::voice::VoiceTarget::New {
            device_id: local,
            space_id,
            config,
        })
    }

    /// The composer's voice button and the shortcut: a voice chat in the
    /// open conversation, or in a new one, opened on its call screen.
    pub(super) fn start_voice(&mut self, cx: &mut Context<Self>) {
        let Some(engine) = self.state.read(cx).engine().cloned() else {
            return;
        };
        let Some(target) = self.voice_target(cx) else {
            return;
        };
        let (device, created) = match &target {
            crate::voice::VoiceTarget::Chat { device_id, .. } => (device_id.clone(), false),
            crate::voice::VoiceTarget::New { device_id, .. } => (device_id.clone(), true),
        };
        let host_name = self
            .state
            .read(cx)
            .device_name(&device)
            .unwrap_or("This device")
            .to_owned();
        let voice = settings::current(cx).codex_voice;
        self.voice_stage_placing = true;
        self.voice.update(cx, |controller, cx| {
            controller.start(engine, target, voice, cx);
            controller.host_name = Some(host_name);
            controller.set_stage_open(true, cx);
        });
        // A new conversation is opened once the workspace lists it.
        self.voice_pending_chat = created
            .then(|| self.voice.read(cx).chat_id.clone())
            .flatten();
        self.follow_voice(cx);
    }

    /// Move into the conversation a voice chat was just created in. Selecting
    /// a chat the workspace does not list yet would be dropped by the next
    /// sync, so this waits for its row; it runs on every voice and workspace
    /// change until then.
    ///
    /// It also waits for the call screen to cover what it replaces: entering
    /// the conversation sets off that route's own travel (the composer docks,
    /// the artwork dissolves), which has no business showing through a
    /// screen that is still arriving.
    pub(super) fn follow_voice(&mut self, cx: &mut Context<Self>) {
        let Some(chat) = self.voice_pending_chat.clone() else {
            return;
        };
        let voice = self.voice.read(cx);
        if !voice.is_live() || voice.chat_id.as_deref() != Some(chat.as_str()) {
            self.voice_pending_chat = None;
        } else if voice.stage_open && self.voice_stage_beneath() > 0.0 {
            // `sync_voice_stage` comes back to it on every frame until then.
        } else if self.state.read(cx).chats.iter().any(|c| c.id == chat) {
            self.voice_pending_chat = None;
            self.open_chat(chat, cx);
        }
    }

    /// Shows what is being said in the call's conversation as it is said.
    /// The engine records an utterance only once it is finished, which
    /// leaves the transcript a sentence behind the call. Until then each
    /// utterance is an echo carrying the id its record will have
    /// ([`zeron_doc::voice::entry_id`]), so the recorded entry takes its
    /// place without a seam. Only the transcript is told, not everything
    /// that observes the workspace: words arrive many times a second.
    pub(super) fn echo_voice(&mut self, cx: &mut Context<Self>) {
        let (chat, session, said) = {
            let voice = self.voice.read(cx);
            let session = voice.snapshot.as_ref().map(|s| s.session_id.clone());
            let said = voice
                .utterances
                .iter()
                .filter_map(|utterance| {
                    let (item, role) = (utterance.item_id.as_ref()?, utterance.role?);
                    (!utterance.text.trim().is_empty()).then(|| {
                        (
                            item.clone(),
                            role,
                            utterance.text.clone(),
                            utterance.at_ms,
                            utterance.done.is_some(),
                        )
                    })
                })
                .collect::<Vec<_>>();
            (
                voice.chat_id.clone().filter(|_| voice.is_live()),
                session,
                said,
            )
        };
        let echoes = &mut self.voice_echoes;
        let (Some(chat), Some(session)) = (chat, session) else {
            // The call is over: what was still being said will never be
            // recorded. What was finished keeps its place until its record
            // arrives.
            let (Some(chat), said) = (echoes.chat.take(), std::mem::take(&mut echoes.said)) else {
                return;
            };
            self.state.update(cx, |state, cx| {
                for (id, _) in said.iter().filter(|(_, (_, done))| !done) {
                    state.remove_echo(&chat, id);
                }
                cx.emit(crate::state::TranscriptTextChanged { doc_id: chat });
            });
            return;
        };
        if echoes.chat.as_deref() != Some(chat.as_str()) {
            echoes.chat = Some(chat.clone());
            echoes.said.clear();
        }
        let mut entries = Vec::new();
        for (item, role, text, at_ms, done) in said {
            let id = zeron_doc::voice::entry_id(&session, &item);
            if echoes.said.insert(id.clone(), (text.len(), done)) == Some((text.len(), done)) {
                continue;
            }
            entries.push(zeron_doc::SessionMessageEntry {
                id,
                role: match role {
                    VoiceRole::User => zeron_doc::MessageRole::User,
                    VoiceRole::Assistant => zeron_doc::MessageRole::Assistant,
                },
                parts: vec![zeron_doc::MessagePart::Text {
                    id: "t0".into(),
                    text,
                }],
                created_at: at_ms,
                device_id: String::new(),
                status: Some(if done {
                    zeron_doc::MessageStatus::Complete
                } else {
                    zeron_doc::MessageStatus::Streaming
                }),
                continuation_of: None,
                duration_ms: None,
            });
        }
        if entries.is_empty() {
            return;
        }
        self.state.update(cx, |state, cx| {
            let device = state
                .chats
                .iter()
                .find(|c| c.id == chat)
                .map(|c| c.device_id.clone())
                .unwrap_or_default();
            let mut changed = false;
            for mut entry in entries {
                entry.device_id = device.clone();
                changed |= state.set_echo(&chat, entry);
            }
            if changed {
                cx.emit(crate::state::TranscriptTextChanged { doc_id: chat });
            }
        });
    }

    /// How much of what the call screen replaces (the conversation, its
    /// panes and its title bar) is still in view, 1–0: it has left by the
    /// time the screen's own parts arrive.
    pub(super) fn voice_stage_beneath(&self) -> f32 {
        if self.voice_stage_cut.is_some() {
            return 0.0;
        }
        1.0 - stage_part(
            self.voice_stage_progress,
            self.voice_stage_entrance().beneath,
        )
    }

    /// How the call screen arrives: the long way for a call being placed,
    /// the short one for a call already under way.
    pub(super) fn voice_stage_entrance(&self) -> Entrance {
        if self.voice_stage_placing {
            PLACING
        } else {
            ARRIVING
        }
    }

    /// Set the call screen aside for the conversation it runs in: the
    /// ordinary transcript and composer, with the compact row as the way
    /// back.
    pub(super) fn show_voice_conversation(&mut self, chat_id: &str, cx: &mut Context<Self>) {
        self.set_voice_stage_open(false, cx);
        if self.state.read(cx).selected_chat.as_deref() != Some(chat_id) {
            self.voice_stage_hold = true;
            self.open_chat(chat_id.to_owned(), cx)
        }
    }

    /// The call's bar outside its call screen, floated where the user is:
    /// in the composer's room in a conversation, in the top corner of a
    /// screen without one ([`crate::voice::float`]).
    pub(super) fn render_voice_bar(&self, cx: &App) -> Option<AnyElement> {
        let dock = self.voice_dock.read(cx);
        // A failed start explains itself in the composer's room instead.
        if !dock.shown(cx) || dock.failed(cx) {
            return None;
        }
        let left = self.sidebar_now();
        Some(
            crate::voice::float::floating(
                self.voice_dock.clone(),
                dock.slot(),
                dock.placed(),
                (left, (self.viewport_width - left).max(0.0)),
                dock.travel(),
            )
            .into_any_element(),
        )
    }

    /// The call screen of the live voice chat: its conversation, on its
    /// dedicated screen.
    pub(super) fn open_voice(&mut self, cx: &mut Context<Self>) {
        let chat = self.voice.read(cx).chat_id.clone();
        if let Some(chat) = chat
            && self.state.read(cx).selected_chat.as_ref() != Some(&chat)
            && self.state.read(cx).chats.iter().any(|c| c.id == chat)
        {
            self.open_chat(chat, cx);
        }
        self.set_voice_stage_open(true, cx);
    }

    /// The composer's voice button: start a voice chat.
    pub(super) fn enter_voice(&mut self, cx: &mut Context<Self>) {
        self.toggle_voice(cx);
    }

    /// The voice shortcut: start a voice chat, or go to the live one.
    pub(super) fn toggle_voice(&mut self, cx: &mut Context<Self>) {
        if self.voice.read(cx).is_live() {
            self.open_voice(cx);
        } else if settings::current(cx).codex_voice_enabled {
            self.start_voice(cx);
        }
    }

    /// The call's cues: one as it connects, one as a connected call ends.
    /// A failed start or a dropped call uses the session attention cue, under
    /// its own setting and the gate it shares with the other failures.
    pub(super) fn sound_voice_call(&mut self, cx: &mut Context<Self>) {
        let call = self.voice.read(cx).call();
        let previous = std::mem::replace(&mut self.voice_call, call);
        match crate::sound::voice_sound_since(call, previous) {
            Some(crate::sound::VoiceCallSound::Cue(cue)) => {
                if settings::current(cx).codex_voice_sound_enabled {
                    crate::sound::play_voice(cue);
                }
            }
            Some(crate::sound::VoiceCallSound::Attention) => {
                if self
                    .settings
                    .session_sound_enabled(crate::sound::Sound::Attention)
                    && self
                        .attention_sound_gate
                        .should_play(std::time::Instant::now())
                {
                    crate::sound::play(crate::sound::Sound::Attention);
                }
            }
            None => {}
        }
    }

    /// Leaving the screen of a failed session also puts its failure away:
    /// it has been read.
    pub(super) fn set_voice_stage_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.voice.update(cx, |voice, cx| {
            if !open && voice.phase == VoicePhase::Failed {
                voice.dismiss_reason(cx);
            }
            voice.set_stage_open(open, cx)
        });
    }

    /// A session that could not start, or dropped: why, and the way to try
    /// again, on the screen it happened on.
    fn render_voice_failure(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        use settings::widgets::{ActionTone, text_action};
        let reason = SharedString::from(self.voice.read(cx).reason_text());
        let accent = theme.accent;
        let action = |id: &'static str, tone: ActionTone, label: &'static str| {
            text_action(theme, tone, label)
                .id(id)
                .debug_selector(move || id.into())
                .role(gpui::Role::Button)
                .aria_label(label)
                .tab_index(0)
                // The focus ring's room is always there.
                .border_1()
                .border_color(gpui::transparent_black())
                .focus_visible(move |style| style.border_color(accent))
        };
        div()
            .id("voice-stage-failure")
            .debug_selector(|| "voice-stage-failure".into())
            .role(gpui::Role::Status)
            .aria_label(format!("Voice stopped. {reason}"))
            .max_w(px(420.0))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(6.0))
            .child(
                icon(icons::DANGER_TRIANGLE)
                    .size(px(22.0))
                    .text_color(theme.warning),
            )
            .child(
                div()
                    .mt(px(6.0))
                    .text_size(crate::typography::ui_rems(15.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child("Voice stopped"),
            )
            .child(
                div()
                    .text_center()
                    .text_size(crate::typography::ui_rems(13.0))
                    .line_height(px(20.0))
                    .text_color(theme.text_muted)
                    .child(reason),
            )
            .child(
                div()
                    .mt(px(14.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        action("voice-stage-close", ActionTone::Quiet, "Close").on_click(
                            cx.listener(|this, _, _, cx| this.set_voice_stage_open(false, cx)),
                        ),
                    )
                    .child(
                        action("voice-stage-retry", ActionTone::Solid, "Try again")
                            .on_click(cx.listener(|this, _, _, cx| this.start_voice(cx))),
                    ),
            )
    }

    /// Track stage open/close flips for the transition clock and hand focus
    /// over: the stage owns the keyboard while it is up. The call screen is
    /// the voice chat's conversation: opening that conversation opens it,
    /// and moving to another thread or to Settings leaves it; voice
    /// continues either way.
    pub(super) fn sync_voice_stage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.state.read(cx).selected_chat.clone();
        let navigating_to_settings =
            self.route != self.voice_stage_route && matches!(self.route, Route::Settings(_));
        // Record every render, even with no visibility change: leaving and
        // reentering the same settings page must count as navigation too.
        self.voice_stage_route = self.route;
        let previous = std::mem::replace(&mut self.voice_stage_selection, selected.clone());
        let moved = selected != previous;
        let (open, live, failed, home) = {
            let voice = self.voice.read(cx);
            (
                voice.stage_open,
                voice.is_live(),
                voice.phase == VoicePhase::Failed,
                selected.is_some() && voice.chat_id == selected,
            )
        };
        if moved && home && live {
            // Unless the conversation was opened for its composer, to answer
            // a question from Codex.
            if !std::mem::take(&mut self.voice_stage_hold) {
                // The page under the screen is already the conversation,
                // which nobody has seen yet: it is not shown leaving.
                self.voice_stage_cut = Some(previous.is_none());
                // On its way from this very frame, so that no frame shows
                // neither the page nor the screen.
                self.voice_stage_progress = self.voice_stage_progress.max(f32::EPSILON);
                self.set_voice_stage_open(true, cx);
            }
        } else if open && self.voice_stage_was_open {
            // A failed voice chat's own conversation may have been removed
            // from under it: that is no reason to put the failure away.
            let left = moved && !home && !(failed && selected.is_none());
            if navigating_to_settings || left {
                self.voice_pending_chat = None;
                self.set_voice_stage_open(false, cx);
            }
        }
        let open = self.voice.read(cx).stage_open;
        if !open {
            self.voice_stage_cut = None;
        }
        // The screen's clock. A change of direction starts counting now:
        // the time the window sat idle before the press is not motion.
        let now = std::time::Instant::now();
        let elapsed = self
            .voice_stage_ticked
            .replace(now)
            .map(|at| now.saturating_duration_since(at).as_secs_f32() * 1000.0);
        let target = if open { 1.0 } else { 0.0 };
        let moving = self.voice_stage_progress != target || self.voice_bar_travel.active();
        if let Some(ms) = elapsed.filter(|ms| moving && *ms > SLOW_FRAME_MS) {
            tracing::info!(
                target: "zeron_ui::voice",
                ms = ms as u64,
                screen = self.voice_stage_progress,
                bar = self.voice_bar_travel.value,
                open,
                "slow frame in a call transition"
            );
        }
        self.voice_stage_progress = match elapsed {
            // The first frame has nothing to come from.
            None => target,
            Some(_) if open != self.voice_stage_was_open => self.voice_stage_progress,
            Some(elapsed) => stage_progress(
                self.voice_stage_progress,
                open,
                elapsed,
                self.reduced_motion,
                self.voice_stage_entrance().ms,
            ),
        };
        // The long way in is for placing a call: once the screen has left,
        // the next time it comes is the short way.
        if !open && self.voice_stage_progress <= 0.0 {
            self.voice_stage_placing = false;
        }
        if self.voice_stage_progress != target {
            self.motion_active.set(true);
        }
        self.follow_voice(cx);
        // The floating bar belongs above a conversation's composer and at
        // the top of any screen without one. It passes from one to the
        // other with the route without crossing the screen
        // (`float::passage`), on the composer's own spring and in its time:
        // it arrives above the composer as the composer settles. A bar that
        // is not in view takes its place at once.
        let docked = matches!(self.route, Route::Chat) && selected.is_some();
        let place = if docked { 1.0 } else { 0.0 };
        // The floating bar and the call screen's own never show together:
        // the floating one leaves as the screen is asked for, and comes
        // back only once the screen's controls have left.
        let covered = open || self.voice_stage_progress > self.voice_stage_entrance().controls.0;
        if self.voice_dock.read(cx).covered() != covered {
            self.voice_dock
                .update(cx, |dock, _| dock.set_covered(covered));
        }
        if self.reduced_motion || !self.voice_dock.read(cx).shown(cx) {
            self.voice_bar_travel = crate::composer_dock::Glide::new(place);
        } else if covered {
            // Leaving for the call screen: it goes where it stands.
        } else {
            // Its room above the composer goes with the conversation, at
            // once: so does the bar, which then arrives in the corner.
            if !docked && self.voice_bar_travel.value > 0.5 {
                self.voice_bar_travel = crate::composer_dock::Glide::new(0.5);
            }
            self.voice_bar_travel.advance(
                place,
                elapsed.unwrap_or(0.0).min(STEP_MS) / 1000.0,
                crate::composer_dock::duration(docked),
            );
        }
        if self.voice_bar_travel.active() {
            self.motion_active.set(true);
        }
        let travel = self.voice_bar_travel.value.clamp(0.0, 1.0);
        if self.voice_dock.read(cx).travel() != travel {
            self.voice_dock
                .update(cx, |dock, _| dock.set_travel(travel));
        }
        if open == self.voice_stage_was_open {
            return;
        }
        self.voice_stage_was_open = open;
        let focus = if open {
            self.voice_stage_focus.clone()
        } else {
            self.composer.focus_handle(cx)
        };
        if window.is_window_active() {
            window.focus(&focus, cx);
        }
    }

    /// What pressing a tool line of the feed does: a fetch opens its page, a
    /// Zeron call goes to its chat (a side chat opens beside the voice
    /// chat's conversation, the call carrying on in the floating bar), and
    /// anything else is shown in the conversation, where it opens.
    fn press_voice_tool(&mut self, action: &ToolAction, cx: &mut Context<Self>) {
        let own = self.voice.read(cx).chat_id.clone();
        match action {
            ToolAction::Link(url) => cx.open_url(url),
            ToolAction::Chat(id) => {
                let side = self
                    .state
                    .read(cx)
                    .chats
                    .iter()
                    .find(|chat| &chat.id == id)
                    .is_some_and(|chat| chat.parent_chat_id.is_some());
                match own {
                    Some(own) if side => {
                        self.show_voice_conversation(&own, cx);
                        self.open_child_chat_tab(id, cx);
                    }
                    _ => self.open_chat(id.clone(), cx),
                }
            }
            ToolAction::Conversation => {
                if let Some(own) = own {
                    self.show_voice_conversation(&own, cx);
                }
            }
        }
    }

    /// One tool call of the feed, written as the transcript writes it, so
    /// the call screen and the conversation show the same thing. Pressing
    /// it leads where its mark says. `age` counts back from the latest,
    /// which is in full ink while the ones before recede.
    fn render_voice_tool(
        &self,
        theme: &Theme,
        line: ToolLine,
        age: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let ToolLine {
            key,
            part,
            call,
            running,
            failed,
            action,
        } = line;
        let (leads, label) = match &action {
            ToolAction::Link(_) => (crate::icons::ARROW_UP_RIGHT, "Open the page in the browser"),
            ToolAction::Chat(_) => (crate::icons::ARROW_RIGHT, "Go to the chat"),
            ToolAction::Conversation => (crate::icons::ARROW_RIGHT, "Show in the conversation"),
        };
        let (muted, accent) = (theme.text_muted, theme.accent);
        let group = SharedString::from(format!("voice-tool-{key}"));
        // Spinners pulse through the call's bar, a view that redraws on its
        // own anyway, rather than through the shell and all that observes it.
        let written = crate::transcript::call_line(
            &part,
            &call,
            failed,
            !running,
            theme,
            self.voice_stage_bar.entity_id(),
            cx,
        );
        let row = div()
            .id(gpui::ElementId::Name(group.clone()))
            .debug_selector({
                let key = key.clone();
                move || format!("voice-stage-tool-{key}")
            })
            .group(group.clone())
            .role(gpui::Role::Button)
            .aria_label(label)
            .tab_index(0)
            .flex_none()
            .max_w_full()
            .h(px(ACTIVITY_LINE))
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .opacity(match age {
                0 => 1.0,
                1 => 0.76,
                2 => 0.6,
                _ => 0.48,
            })
            .border_1()
            .border_color(gpui::transparent_black())
            .rounded(px(6.0))
            .px(px(4.0))
            .hover(|style| style.opacity(1.0))
            .active(|style| style.opacity(0.7))
            .focus_visible(move |style| style.border_color(accent))
            .on_click(cx.listener(move |this, _, _, cx| this.press_voice_tool(&action, cx)))
            .child(written)
            .child(
                div()
                    .flex_none()
                    .opacity(0.0)
                    .group_hover(group, |style| style.opacity(1.0))
                    .child(icon(leads).size(px(12.0)).text_color(muted)),
            );
        // A short fade: an entrance that runs at display rate redraws the
        // whole window for as long as it lasts.
        motion::fade_quick(
            gpui::ElementId::Name(format!("voice-activity-{key}").into()),
            div().flex_none().max_w_full().flex().flex_col().child(row),
        )
    }

    /// The call's feed, under the orb: the turn the call is in, top to
    /// bottom in the order it happens. What the user asked, quiet; what
    /// Codex does about it, one line per tool call; what Codex says, as the
    /// caption, with the words being said brought out. Nothing in it has a
    /// surface of its own and it only ever grows downwards, so nothing in it
    /// moves but to make room for a new tool line. A new turn takes its
    /// place, and it leaves as one a while after the turn goes quiet, unless
    /// it is being read: the conversation keeps everything. `room` is the
    /// height it may take (a short window loses the turn's start, never what
    /// is being said) and `reveal` the orb's own arrival.
    fn render_voice_feed(
        &mut self,
        theme: &Theme,
        width: f32,
        room: f32,
        reveal: f32,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let now = std::time::Instant::now();
        let (turn, asked, answer, lines, earlier, active, chat_id, status) = {
            let voice = self.voice.read(cx);
            let state = self.state.read(cx);
            let (asked, answer) = turn(voice);
            // The conversation's record only belongs to this call while it
            // is the one in view.
            let (lines, earlier) =
                if voice.chat_id.is_some() && voice.chat_id == state.selected_chat {
                    recent_tools(voice, &state.transcript, &state.chats)
                } else {
                    (Vec::new(), 0)
                };
            (
                asked
                    .and_then(|asked| asked.item_id.clone())
                    .unwrap_or_else(|| "open".to_owned()),
                asked.map(|asked| words(asked, true)),
                answer.map(|answer| words(answer, false)),
                lines,
                earlier,
                matches!(
                    voice.state(),
                    Some(
                        crate::voice::VoiceState::Working
                            | crate::voice::VoiceState::Speaking
                            | crate::voice::VoiceState::Asking
                    )
                ),
                voice.chat_id.clone(),
                voice.status(),
            )
        };
        let rows = lines.len() + usize::from(earlier > 0);
        let (height, easing) =
            self.voice_feed
                .follow(&turn, &lines, rows, self.reduced_motion, now);
        if easing {
            self.motion_active.set(true);
        }
        // One life for the whole feed: it is there while its turn is, and
        // goes as one once nothing in it has changed for a while.
        let spoken_words = || asked.iter().chain(answer.iter());
        let busy = active
            || self.voice_feed.hovered
            || lines.iter().any(|line| line.running)
            || spoken_words().any(|words| words.live);
        let last = spoken_words()
            .filter_map(|words| words.last)
            .chain(self.voice_feed.at)
            .max();
        let presence = if busy {
            1.0
        } else {
            last.map_or(0.0, |at| {
                let left = FEED_LINGER.saturating_sub(at.elapsed()).as_secs_f32();
                (left / FEED_FADE.as_secs_f32()).clamp(0.0, 1.0)
            })
        };
        let slot = div()
            .id("voice-stage-feed")
            .debug_selector(|| "voice-stage-feed".into())
            .role(gpui::Role::Status)
            .flex_none()
            .w(px(width))
            // What is said shares one room, whatever Codex is doing has its
            // own on top of it, and the newest stays in view: the oldest
            // line leaves through the top.
            .max_h(px(room.min(
                WORDS_LINES * WORDS_LINE
                    + if rows > 0 {
                        height * ACTIVITY_LINE + 2.0 * ACTIVITY_GAP
                    } else {
                        0.0
                    },
            )))
            .overflow_hidden()
            .flex()
            .flex_col()
            .justify_end()
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                this.voice_feed.hovered = *hovered;
                cx.notify();
            }));
        if presence <= 0.0 || (asked.is_none() && answer.is_none() && rows == 0) {
            return slot.aria_label(status);
        }
        let label = spoken_words()
            .last()
            .map_or_else(|| status.to_owned(), |words| words.text.clone());
        // One step of ink apart: the user's words rest faint and are muted
        // while being said; Codex's rest muted while being said and end in
        // full ink.
        let (faint, said, bright) = (theme.text_faint, theme.text_muted, theme.text);

        let activity = (rows > 0).then(|| {
            let count = lines.len();
            div()
                .id("voice-stage-activity")
                .debug_selector(|| "voice-stage-activity".into())
                .flex_none()
                .w_full()
                .flex()
                .flex_col()
                .items_center()
                .py(px(ACTIVITY_GAP))
                .text_size(crate::typography::ui_rems(13.5))
                .line_height(px(20.0))
                // A new line opens its own room instead of jolting what is
                // said under it.
                .when(easing, |el| {
                    el.h(px(height * ACTIVITY_LINE + 2.0 * ACTIVITY_GAP))
                        .overflow_hidden()
                })
                .when(earlier > 0, |el| {
                    el.child(
                        feed_link(
                            "voice-stage-earlier",
                            format!("{earlier} earlier"),
                            crate::icons::ARROW_RIGHT,
                            theme,
                        )
                        .debug_selector(|| "voice-stage-earlier".into())
                        .h(px(ACTIVITY_LINE))
                        .opacity(0.6)
                        .when_some(chat_id, |el, chat_id| {
                            el.on_click(cx.listener(move |this, _, _, cx| {
                                this.show_voice_conversation(&chat_id, cx)
                            }))
                        }),
                    )
                })
                .children(lines.into_iter().enumerate().map(|(index, line)| {
                    self.render_voice_tool(theme, line, count - 1 - index, cx)
                        .into_any_element()
                }))
        });
        let caption = answer.as_ref().map(|answer| {
            motion::fade_quick(
                gpui::ElementId::Name(format!("voice-caption-{}", answer.key).into()),
                div().flex_none().w_full().child(spoken(
                    "voice-stage-caption",
                    answer,
                    (said, bright),
                    CAPTION_LINES,
                )),
            )
        });
        slot.aria_label(label)
            .opacity(presence * reveal)
            .child(motion::fade_quick(
                gpui::ElementId::Name(format!("voice-feed-{turn}").into()),
                div()
                    .flex_none()
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .children(asked.as_ref().map(|asked| {
                        spoken("voice-stage-asked", asked, (faint, said), ASKED_LINES)
                    }))
                    .children(activity)
                    .children(caption),
            ))
    }

    /// The call screen: the session's orb where the new-thread hero artwork
    /// dissolves, the turn the call is in under it, and the controls.
    pub(super) fn render_voice_stage(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let progress = self.voice_stage_progress;
        let entrance = self.voice_stage_entrance();
        let (orb_part, controls_part) = (
            stage_part(progress, entrance.orb),
            stage_part(progress, entrance.controls),
        );
        let viewport = window.viewport_size();
        // The sidebar stays usable beside the stage; collapsed, the stage
        // spans the whole window.
        let left = self.sidebar_now();
        let width = (f32::from(viewport.width) - left).max(0.0);
        // The orb sizes itself to the window, so a short one still leaves
        // the conversation room; the conversation starts right under it.
        let controls = STAGE_BAR_BOTTOM + STAGE_BAR_HEIGHT + STAGE_BAR_CLEARANCE;
        let body = (f32::from(viewport.height) - Theme::TITLEBAR_HEIGHT - controls).max(0.0);
        let orb_edge = (body * STAGE_ORB_SHARE).clamp(STAGE_ORB_MIN, STAGE_ORB_MAX);

        let voice = self.voice.read(cx);
        let failed = voice.phase == VoicePhase::Failed;
        let orb_region = if failed {
            STAGE_FAILURE_SLOT
        } else {
            (orb_edge * STAGE_ORB_SLOT).max(STAGE_SLOT_MIN)
        };
        let caption_width = CAPTION_WIDTH.min((width - 48.0).max(200.0));
        let reduced = self.reduced_motion;
        let state = voice.state();
        let status = voice.status();
        // Whether the work Codex is doing is a lookup, for the orb's figure.
        let searching = {
            let workspace = self.state.read(cx);
            voice.chat_id.is_some()
                && voice.chat_id == workspace.selected_chat
                && looking_up(&workspace.transcript)
        };
        let now = std::time::Instant::now();
        // The orb is in its place from the moment the call is placed, held
        // back with the loader at its centre, and comes alive when the call
        // connects: nothing moves, so the wait reads as the call ringing.
        let connected = voice
            .active_since
            .map(|at| at.elapsed().as_secs_f32() * 1000.0);
        let orb_in = orb_reveal(connected, reduced);
        if connected.is_some() && orb_in < 1.0 {
            self.motion_active.set(true);
        }
        // A failure stands where the orb it replaces stood.
        let group = orb_region + CAPTION_GAP + STAGE_GROUP_LINES * WORDS_LINE;
        let top_pad = ((body - group) * STAGE_GROUP_AT).max(8.0);
        if progress <= 0.0 {
            self.voice_feed = VoiceFeed::default();
            self.voice_orb.release(window);
            return None;
        }

        let theme = Theme::of(cx).clone();
        let (background, effect) = settings::read(cx, |settings| {
            (
                settings.new_thread_composer_background.clone(),
                settings.new_thread_background_effect,
            )
        });
        let artwork = background.as_ref().and_then(|background| {
            crate::new_thread_background_effects::prepare(
                effect,
                &theme,
                std::path::Path::new(&background.path),
                cx,
            )
        });
        let adjustment = background
            .as_ref()
            .map(|background| background.adjustment)
            .unwrap_or_default();
        // The new-thread hero's artwork, in the very crop that screen shows,
        // dissolving from the top of the window down to where the orb sits:
        // one fade across the whole background, with nothing cut out of it.
        // The orb stands on the horizon that leaves, and everything the call
        // says is below it, on the window's own material.
        let horizon = Theme::TITLEBAR_HEIGHT + top_pad + orb_region / 2.0;
        // Over the new-session screen the same artwork is already in view:
        // this one stands in for it at once, so the picture holds still
        // while that screen leaves from under it. Fading one in as the other
        // fades out would dim the picture half-way through. Over a
        // conversation there is none, and it comes in with the screen.
        let over_canvas = self.voice_stage_cut == Some(true)
            || (matches!(self.route, Route::Chat) && self.state.read(cx).selected_chat.is_none());
        let artwork_part = if over_canvas {
            1.0
        } else {
            stage_part(progress, entrance.artwork)
        };
        let hero = artwork.map(|artwork| {
            div()
                .absolute()
                .top_0()
                .left_0()
                .w(px(width))
                .h(px(new_thread_background_height(f32::from(viewport.height))))
                .opacity(artwork_part * new_thread_background_opacity(theme.is_frost()))
                .child(
                    gpui::canvas(
                        |_, _, _| {},
                        move |bounds, _, window, _| {
                            crate::new_thread_background_mask::paint_above(
                                artwork.clone(),
                                bounds,
                                bounds.top() + px(horizon),
                                adjustment,
                                window,
                            );
                        },
                    )
                    .absolute()
                    .inset_0(),
                )
        });

        // One slot for the orb, or for the failure if the call could not
        // start.
        let orb = div()
            .relative()
            .flex_none()
            .w_full()
            .h(px(orb_region))
            .flex()
            .items_center()
            .justify_center()
            .when(failed, |el| el.child(self.render_voice_failure(&theme, cx)))
            .when(!failed, |el| {
                // The orb is the call: a sphere of glass with a fluid in it
                // that the voice stirs and that moves as the call's state
                // has it, from the moment the call is placed. Until the call
                // connects it is held back.
                let heard = self.voice.read(cx).heard();
                let frame = self.voice_orb.frame(
                    crate::voice::orb::State::of(state, searching),
                    heard,
                    &theme,
                    STAGE_ORB_RESOLUTION,
                    (orb_edge * window.scale_factor()).round() as u32,
                    now,
                    reduced,
                    window,
                );
                el.child(crate::voice::orb::orb(
                    crate::voice::orb::Shape {
                        // A call being placed grows its orb into place.
                        diameter: orb_edge
                            * (entrance.orb_from + (1.0 - entrance.orb_from) * orb_part),
                        room: orb_region,
                    },
                    frame,
                    orb_in,
                    |disc| {
                        disc.id("voice-stage-orb")
                            .debug_selector(|| "voice-stage-orb".into())
                            .role(gpui::Role::Status)
                            .aria_label(status)
                            .into_any_element()
                    },
                ))
            });
        // The feed takes the stage with the orb, right under it, down to
        // the controls at most.
        let feed = if !failed && orb_in > 0.0 {
            let room = (body - top_pad - orb_region - CAPTION_GAP).max(0.0);
            Some(
                self.render_voice_feed(&theme, caption_width, room, orb_in, cx)
                    .mt(px(CAPTION_GAP)),
            )
        } else {
            self.voice_feed = VoiceFeed::default();
            None
        };
        // Where the call runs, where a conversation says it too: at the
        // bottom's leading edge, as the status strip under a composer, on
        // the controls' own line and out of the orb's way. The device and
        // the project, in the new-session screen's quiet chips; a
        // conversation that had a name before the call keeps it in front.
        let context = {
            let state = self.state.read(cx);
            let chat_id = self.voice.read(cx).chat_id.clone();
            chat_id
                .and_then(|id| state.chats.iter().find(|chat| chat.id == id))
                .map(|chat| {
                    let title =
                        chat.title
                            .as_deref()
                            .map(transcript::single_line)
                            .filter(|title| {
                                !title.is_empty() && title != crate::voice::VOICE_CHAT_TITLE
                            });
                    let device = state
                        .device_name(&chat.device_id)
                        .unwrap_or("This device")
                        .to_owned();
                    let project = chat
                        .space_id
                        .as_deref()
                        .and_then(|id| state.space_row(id))
                        .map(|space| space.display_name().to_string())
                        .unwrap_or_else(|| "No project".to_owned());
                    title
                        .map(|title| (crate::icons::CHAT_ROUND_LINE, title))
                        .into_iter()
                        .chain([
                            (crate::icons::MONITOR, device),
                            (crate::icons::FOLDER, project),
                        ])
                        .collect::<Vec<_>>()
                })
        };
        let quiet = theme.text_muted.opacity(0.7);
        // What the leading side has beside the centred controls.
        let context_room = width / 2.0 - STAGE_CONTEXT_CLEARANCE - STAGE_CONTEXT_INSET;
        let context = context
            .filter(|_| context_room >= STAGE_CONTEXT_MIN)
            .map(|chips| {
                div()
                    .id("voice-stage-context")
                    .debug_selector(|| "voice-stage-context".into())
                    .absolute()
                    .left(px(STAGE_CONTEXT_INSET))
                    .w(px(context_room))
                    .bottom(px(STAGE_BAR_BOTTOM
                        + (STAGE_BAR_HEIGHT - STAGE_CONTEXT_HEIGHT) / 2.0
                        - STAGE_PART_TRAVEL * (1.0 - controls_part)))
                    .h(px(STAGE_CONTEXT_HEIGHT))
                    .opacity(controls_part)
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .text_size(crate::typography::ui_rems(12.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(quiet)
                    .children(chips.into_iter().map(|(glyph, label)| {
                        div()
                            .min_w_0()
                            .max_w(px(280.0))
                            .px(px(8.0))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(icon(glyph).size(px(12.0)).flex_none().text_color(quiet))
                            .child(div().min_w_0().truncate().child(SharedString::from(label)))
                    }))
            });
        // The stage occludes the conversation titlebar. Restore its native
        // drag and double-click behavior above the stage artwork.
        let titlebar = self.titlebar_drag_region(
            "voice-stage-titlebar",
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .h(px(Theme::TITLEBAR_HEIGHT)),
            cx,
        );

        Some(
            div()
                .id("voice-stage")
                .debug_selector(|| "voice-stage".into())
                .track_focus(&self.voice_stage_focus)
                .absolute()
                .top_0()
                .bottom_0()
                .right_0()
                .left(px(left))
                .occlude()
                // No surface of its own: the conversation it replaces leaves
                // from beneath it, so the window's own material shows through
                // exactly as it does behind any conversation. Its parts
                // arrive in turn rather than as one sheet.
                // Escape is the shell's to decide (`Shell::on_key_down`):
                // whatever is open over this screen takes it first.
                .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                    if event.keystroke.key == "m" && !event.keystroke.modifiers.modified() {
                        this.voice.update(cx, |voice, cx| voice.toggle_mute(cx));
                        cx.stop_propagation();
                    }
                }))
                .child(div().absolute().inset_0().children(hero))
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .top(px(
                            Theme::TITLEBAR_HEIGHT + STAGE_PART_TRAVEL * (1.0 - orb_part)
                        ))
                        .opacity(orb_part)
                        .bottom(px(controls))
                        .pt(px(top_pad))
                        .flex()
                        .flex_col()
                        .items_center()
                        .overflow_hidden()
                        .child(orb)
                        .children(feed),
                )
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom(px(
                            STAGE_BAR_BOTTOM - STAGE_PART_TRAVEL * (1.0 - controls_part)
                        ))
                        .opacity(controls_part)
                        .px(px(24.0))
                        .flex()
                        .justify_center()
                        .child(self.voice_stage_bar.clone()),
                )
                .children(context)
                .child(titlebar)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn voice_stage_topbar_accepts_window_drag_presses(cx: &mut gpui::TestAppContext) {
        struct StageHost(Entity<Shell>);
        impl Render for StageHost {
            fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                self.0.update(cx, |shell, cx| {
                    shell.sync_voice_stage(window, cx);
                    div()
                        .size_full()
                        .relative()
                        .children(shell.render_voice_stage(window, cx))
                })
            }
        }

        let dir = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            gpui_base::init(cx);
            cx.set_global(Theme::default());
            crate::app_menus::init(cx);
            settings::init(settings::UiSettings::default(), dir.path(), cx);
        });
        let (host, cx) = cx.add_window_view(|_, cx| {
            StageHost(cx.new(|cx| {
                let state = cx.new(|_| AppState::new());
                let mut shell = Shell::new(
                    state,
                    EngineBootConfig {
                        data_dir: dir.path().into(),
                        ipc_port: 0,
                        edge_url: "http://127.0.0.1:1".into(),
                        edge_token: None,
                        org_id: None,
                        workos_client_id: None,
                        default_harness: HarnessId::Mock,
                    },
                    cx,
                );
                shell.reduced_motion = true;
                shell.voice.update(cx, |voice, _| {
                    voice.phase = VoicePhase::Active;
                    voice.stage_open = true;
                });
                shell
            }))
        });
        let shell = host.read_with(cx, |host, _| host.0.clone());
        cx.update(|window, cx| window.draw(cx).clear());
        let bounds = cx.debug_bounds("voice-stage").unwrap();
        let topbar = gpui::point(
            bounds.center().x,
            bounds.top() + px(Theme::TITLEBAR_HEIGHT / 2.0),
        );
        cx.simulate_mouse_down(topbar, MouseButton::Left, gpui::Modifiers::default());
        shell.read_with(cx, |shell, _| assert!(shell.titlebar_should_move));
        cx.simulate_mouse_up(topbar, MouseButton::Left, gpui::Modifiers::default());
        shell.read_with(cx, |shell, _| assert!(!shell.titlebar_should_move));

        let body = gpui::point(
            bounds.center().x,
            bounds.top() + px(Theme::TITLEBAR_HEIGHT + 32.0),
        );
        cx.simulate_mouse_down(body, MouseButton::Left, gpui::Modifiers::default());
        shell.read_with(cx, |shell, _| assert!(!shell.titlebar_should_move));
        cx.simulate_mouse_up(body, MouseButton::Left, gpui::Modifiers::default());
    }

    #[gpui::test]
    fn voice_stage_can_start_and_reopen_in_settings_until_navigation(
        cx: &mut gpui::TestAppContext,
    ) {
        struct StageHost(Entity<Shell>);
        impl Render for StageHost {
            fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                self.0.update(cx, |shell, cx| {
                    shell.sync_voice_stage(window, cx);
                    div()
                        .size_full()
                        .children(shell.render_voice_stage(window, cx))
                })
            }
        }

        let dir = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            gpui_base::init(cx);
            cx.set_global(Theme::default());
            crate::app_menus::init(cx);
            settings::init(settings::UiSettings::default(), dir.path(), cx);
        });
        let (host, cx) = cx.add_window_view(|_, cx| {
            StageHost(cx.new(|cx| {
                let state = cx.new(|_| AppState::new());
                let mut shell = Shell::new(
                    state,
                    EngineBootConfig {
                        data_dir: dir.path().into(),
                        ipc_port: 0,
                        edge_url: "http://127.0.0.1:1".into(),
                        edge_token: None,
                        org_id: None,
                        workos_client_id: None,
                        default_harness: HarnessId::Mock,
                    },
                    cx,
                );
                shell.reduced_motion = true;
                shell.route = Route::Settings(SettingsSection::Voice);
                // The initial stage opens while native voice is still preparing.
                shell.voice.update(cx, |voice, cx| {
                    voice.phase = VoicePhase::Checking;
                    voice.set_stage_open(true, cx);
                });
                shell
            }))
        });
        let shell = host.read_with(cx, |host, _| host.0.clone());
        let stays_open = |cx: &mut gpui::VisualTestContext| {
            for _ in 0..3 {
                cx.update(|window, cx| window.draw(cx).clear());
                shell.read_with(cx, |shell, cx| assert!(shell.voice.read(cx).stage_open));
                assert!(cx.debug_bounds("voice-stage").is_some());
            }
        };
        stays_open(cx);

        shell.update(cx, |shell, cx| {
            shell
                .voice
                .update(cx, |voice, _| voice.phase = VoicePhase::Active);
            shell.set_voice_stage_open(false, cx);
        });
        cx.update(|window, cx| window.draw(cx).clear());
        shell.update(cx, |shell, cx| shell.set_voice_stage_open(true, cx));
        stays_open(cx);

        // Choosing another settings page hides the stage without ending voice.
        shell.update(cx, |shell, cx| {
            shell.open_settings(SettingsSection::General, cx)
        });
        cx.update(|window, cx| window.draw(cx).clear());
        shell.read_with(cx, |shell, cx| {
            assert!(!shell.voice.read(cx).stage_open);
            assert!(shell.voice.read(cx).is_live());
        });
        shell.update(cx, |shell, cx| shell.set_voice_stage_open(true, cx));
        stays_open(cx);

        // Leaving and reentering the same settings page is navigation too.
        shell.update(cx, |shell, _| shell.route = Route::Chat);
        cx.update(|window, cx| window.draw(cx).clear());
        shell.update(cx, |shell, cx| {
            shell.open_settings(SettingsSection::General, cx)
        });
        cx.update(|window, cx| window.draw(cx).clear());
        shell.read_with(cx, |shell, cx| {
            assert!(!shell.voice.read(cx).stage_open);
            assert!(shell.voice.read(cx).is_live());
        });

        shell.update(cx, |shell, cx| shell.set_voice_stage_open(true, cx));
        stays_open(cx);
        shell.update(cx, |shell, cx| {
            shell.state.update(cx, |state, _| {
                state.selected_chat = Some("another-chat".into())
            });
        });
        cx.update(|window, cx| window.draw(cx).clear());
        shell.read_with(cx, |shell, cx| {
            assert!(!shell.voice.read(cx).stage_open);
            assert!(shell.voice.read(cx).is_live());
        });
    }

    fn shell_window(
        cx: &mut gpui::TestAppContext,
    ) -> (
        tempfile::TempDir,
        Entity<Shell>,
        &mut gpui::VisualTestContext,
    ) {
        struct StageHost(Entity<Shell>);
        impl Render for StageHost {
            fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                self.0.update(cx, |shell, cx| {
                    shell.sync_voice_stage(window, cx);
                    div()
                        .size_full()
                        .relative()
                        .children(shell.render_voice_stage(window, cx))
                })
            }
        }
        let dir = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            gpui_base::init(cx);
            cx.set_global(Theme::default());
            crate::app_menus::init(cx);
            settings::init(settings::UiSettings::default(), dir.path(), cx);
        });
        let data_dir = dir.path().to_path_buf();
        let (host, cx) = cx.add_window_view(|_, cx| {
            StageHost(cx.new(|cx| {
                let state = cx.new(|_| AppState::new());
                let mut shell = Shell::new(
                    state,
                    EngineBootConfig {
                        data_dir,
                        ipc_port: 0,
                        edge_url: "http://127.0.0.1:1".into(),
                        edge_token: None,
                        org_id: None,
                        workos_client_id: None,
                        default_harness: HarnessId::Mock,
                    },
                    cx,
                );
                shell.reduced_motion = true;
                shell
            }))
        });
        let shell = host.read_with(cx, |host, _| host.0.clone());
        (dir, shell, cx)
    }

    fn live(shell: &Entity<Shell>, stage: bool, cx: &mut gpui::VisualTestContext) {
        shell.update(cx, |shell, cx| {
            shell.voice.update(cx, |voice, cx| {
                voice.phase = VoicePhase::Active;
                voice.active_since = Some(std::time::Instant::now());
                voice.chat_id = Some("voice-chat".into());
                voice.snapshot = Some(zeron_proto::voice::VoiceSnapshot {
                    session_id: "one".into(),
                    chat_id: "voice-chat".into(),
                    generation: 2,
                    phase: VoicePhase::Active,
                    muted: false,
                    playing: false,
                    work: zeron_proto::voice::VoiceWork::Idle,
                    reason: None,
                    voice: None,
                    voices: Vec::new(),
                });
                voice.set_stage_open(stage, cx);
                cx.notify();
            });
        });
        cx.update(|window, cx| window.draw(cx).clear());
    }

    /// Select a chat the way the workspace does, without the notification
    /// that makes this engine-less window end the voice chat.
    fn select(shell: &Entity<Shell>, chat: Option<&str>, cx: &mut gpui::VisualTestContext) {
        let chat = chat.map(str::to_owned);
        shell.update(cx, |shell, cx| {
            shell
                .state
                .update(cx, |state, _| state.selected_chat = chat);
        });
        cx.update(|window, cx| window.draw(cx).clear());
    }

    #[gpui::test]
    fn the_shortcut_is_inert_while_voice_is_off_and_leads_to_a_live_call_screen(
        cx: &mut gpui::TestAppContext,
    ) {
        let (_dir, shell, cx) = shell_window(cx);
        shell.update(cx, |shell, cx| {
            settings::update(settings::SavePolicy::Immediate, cx, |s| {
                s.codex_voice_enabled = false
            });
            shell.toggle_voice(cx);
            assert!(!shell.voice.read(cx).is_live());
        });
        // A voice chat that is already live is reachable either way, and the
        // shortcut never sets its screen aside.
        live(&shell, false, cx);
        shell.update(cx, |shell, cx| {
            shell.toggle_voice(cx);
            assert!(shell.voice.read(cx).stage_open);
            shell.toggle_voice(cx);
            assert!(shell.voice.read(cx).stage_open);
        });
    }

    #[gpui::test]
    fn the_call_screen_is_the_voice_chats_conversation(cx: &mut gpui::TestAppContext) {
        let (_dir, shell, cx) = shell_window(cx);
        let open = |cx: &mut gpui::VisualTestContext| {
            shell.read_with(cx, |shell, cx| shell.voice.read(cx).stage_open)
        };
        // Started from the new-session canvas: the screen is up before the
        // new conversation is selected, and stays up when it is.
        live(&shell, true, cx);
        select(&shell, Some("voice-chat"), cx);
        assert!(open(cx));

        // Another thread leaves the screen; voice continues.
        select(&shell, Some("another-chat"), cx);
        assert!(!open(cx));
        shell.read_with(cx, |shell, cx| assert!(shell.voice.read(cx).is_live()));
        // Coming back to the conversation is coming back to the call.
        select(&shell, Some("voice-chat"), cx);
        assert!(open(cx));

        // Unless it is opened for its composer, to answer Codex.
        select(&shell, Some("another-chat"), cx);
        shell.update(cx, |shell, _| shell.voice_stage_hold = true);
        select(&shell, Some("voice-chat"), cx);
        assert!(!open(cx));
        shell.read_with(cx, |shell, _| assert!(!shell.voice_stage_hold));

        // A failed start whose new conversation was removed from under it
        // keeps explaining itself; a real move to another thread puts it away.
        shell.update(cx, |shell, cx| {
            shell.voice.update(cx, |voice, cx| {
                voice.phase = VoicePhase::Failed;
                voice.reason = Some(zeron_proto::voice::VoiceRejection::Protocol);
                voice.set_stage_open(true, cx);
                cx.notify();
            });
        });
        cx.update(|window, cx| window.draw(cx).clear());
        select(&shell, None, cx);
        assert!(open(cx));
        select(&shell, Some("another-chat"), cx);
        assert!(!open(cx));
        shell.read_with(cx, |shell, cx| {
            assert!(shell.voice.read(cx).reason.is_none())
        });
    }

    #[gpui::test]
    fn call_screen_captions_the_call_and_steps_aside_for_a_question(cx: &mut gpui::TestAppContext) {
        use zeron_proto::voice::{VoiceEvent, VoiceTranscript};
        let (_dir, shell, cx) = shell_window(cx);
        live(&shell, true, cx);
        // Nothing said yet: the orb above the middle, an empty feed right
        // under it, and the controls at the bottom.
        let stage = cx.debug_bounds("voice-stage").unwrap();
        let orb = cx.debug_bounds("voice-stage-orb").unwrap();
        let empty = cx.debug_bounds("voice-stage-feed").unwrap();
        let bar = cx.debug_bounds("voice-dock").unwrap();
        assert!(stage.top() < orb.top() && orb.bottom() < empty.top());
        assert!(orb.center().y < stage.center().y);
        assert!(
            empty.top() - orb.bottom() < px(64.0),
            "the feed belongs to the orb"
        );
        assert_eq!(empty.size.height, px(0.0));
        for part in [
            "voice-stage-asked",
            "voice-stage-activity",
            "voice-stage-caption",
        ] {
            assert!(cx.debug_bounds(part).is_none(), "{part}");
        }

        let say = |shell: &Entity<Shell>, long: bool, cx: &mut gpui::VisualTestContext| {
            shell.update(cx, |shell, cx| {
                shell.voice.update(cx, |voice, cx| {
                    for (item, role, text) in [
                        ("u1", VoiceRole::User, "Open the landing page."),
                        ("a1", VoiceRole::Assistant, "On it. Opening it now."),
                    ] {
                        voice.reduce(
                            VoiceEvent::Final {
                                transcript: VoiceTranscript {
                                    session_id: "one".into(),
                                    item_id: item.into(),
                                    role,
                                    text: text.into(),
                                    promoted_message_id: None,
                                },
                            },
                            cx,
                        );
                    }
                    if long {
                        voice.reduce(
                            VoiceEvent::Partial {
                                generation: 2,
                                item_id: Some("a2".into()),
                                text: "word ".repeat(400),
                                role: Some(VoiceRole::Assistant),
                            },
                            cx,
                        );
                    }
                });
            });
            cx.update(|window, cx| window.draw(cx).clear());
            cx.debug_bounds("voice-stage-caption").unwrap()
        };
        // A call shows the turn it is in, not the conversation: what the
        // user asked heads it, quiet, and what Codex says is the caption,
        // one line here. Neither has a surface of its own.
        let short = say(&shell, false, cx);
        let asked = cx.debug_bounds("voice-stage-asked").unwrap();
        assert_eq!(asked.top(), empty.top());
        // The two are one block of text: set alike, one straight under the
        // other.
        assert_eq!(asked.size.height, px(WORDS_LINE));
        assert_eq!(short.top(), asked.bottom());
        assert_eq!(short.size.height, px(WORDS_LINE));
        // A long one stops at four lines: its oldest words leave through
        // the top, it grows downwards, and nothing else on the screen moves.
        let long = say(&shell, true, cx);
        assert_eq!(long.origin, short.origin);
        assert_eq!(long.size.height, px(WORDS_LINE * CAPTION_LINES));
        assert!(long.bottom() < bar.top());
        assert_eq!(cx.debug_bounds("voice-stage-asked").unwrap(), asked);
        assert_eq!(cx.debug_bounds("voice-dock").unwrap(), bar);
        assert_eq!(cx.debug_bounds("voice-stage-orb").unwrap(), orb);
        // The two share one room: under a long answer, a long question
        // gives up its first line through the top.
        let ask = |text: String, cx: &mut gpui::VisualTestContext| {
            shell.update(cx, |shell, cx| {
                shell.voice.update(cx, |voice, cx| {
                    voice.reduce(
                        VoiceEvent::Final {
                            transcript: VoiceTranscript {
                                session_id: "one".into(),
                                item_id: "u1".into(),
                                role: VoiceRole::User,
                                text,
                                promoted_message_id: None,
                            },
                        },
                        cx,
                    )
                });
            });
            cx.update(|window, cx| window.draw(cx).clear());
        };
        ask("word ".repeat(60), cx);
        let feed = cx.debug_bounds("voice-stage-feed").unwrap();
        assert_eq!(feed.top(), empty.top());
        assert_eq!(feed.size.height, px(WORDS_LINE * WORDS_LINES));
        let question = cx.debug_bounds("voice-stage-asked").unwrap();
        assert_eq!(question.size.height, px(WORDS_LINE * ASKED_LINES));
        assert_eq!(question.top(), feed.top() - px(WORDS_LINE));
        assert_eq!(
            cx.debug_bounds("voice-stage-caption").unwrap().bottom(),
            feed.bottom()
        );
        ask("Open the landing page.".into(), cx);
        assert_eq!(cx.debug_bounds("voice-stage-asked").unwrap(), asked);

        // What Codex is doing comes from the record of the conversation the
        // call runs in…
        shell.update(cx, |shell, cx| {
            shell.state.update(cx, |state, _| {
                state.selected_chat = Some("voice-chat".into());
                state.transcript = vec![
                    serde_json::from_value(serde_json::json!({
                        "id": "work",
                        "role": "assistant",
                        "createdAt": chrono::Utc::now().timestamp_millis() + 60_000,
                        "deviceId": "device",
                        "parts": [{
                            "kind": "tool", "id": "t1", "resolved": true, "isError": false,
                            "call": {"kind": "exec", "command": "cargo test"},
                            "output": "ok. 12 passed; see https://example.com/run/7.",
                        }],
                    }))
                    .unwrap(),
                ];
            });
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear());
        // …and takes its place in the turn: after what was asked, before
        // what Codex says about it, one line tall, clear of the controls.
        let activity = cx.debug_bounds("voice-stage-activity").unwrap();
        assert_eq!(cx.debug_bounds("voice-stage-asked").unwrap(), asked);
        assert_eq!(activity.top(), asked.bottom());
        assert_eq!(activity.size.height, px(ACTIVITY_LINE + 2.0 * ACTIVITY_GAP));
        let caption = cx.debug_bounds("voice-stage-caption").unwrap();
        assert_eq!(caption.top(), activity.bottom());
        assert!(caption.bottom() < bar.top());
        // A line is written as the conversation writes it, and pressing it
        // shows it there, where its result is read: the conversation takes
        // the call screen's place.
        let tool = cx.debug_bounds("voice-stage-tool-work-t1").unwrap();
        assert!(activity.contains(&tool.center()));
        cx.simulate_click(tool.center(), gpui::Modifiers::default());
        shell.read_with(cx, |shell, cx| {
            assert!(!shell.voice.read(cx).stage_open);
            assert!(shell.voice.read(cx).is_live());
        });
        shell.update(cx, |shell, cx| shell.open_voice(cx));
        cx.update(|window, cx| window.draw(cx).clear());
        assert_eq!(cx.debug_bounds("voice-stage-asked").unwrap(), asked);

        // The ordinary transcript is one press away: the bar's conversation
        // control sets the call screen aside in the same conversation, and
        // the compact row's track brings it back.
        select(&shell, Some("voice-chat"), cx);
        let chat = cx.debug_bounds("voice-dock-transcript").unwrap();
        cx.simulate_click(chat.center(), gpui::Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear());
        shell.read_with(cx, |shell, cx| {
            assert!(!shell.voice.read(cx).stage_open);
            assert!(shell.voice.read(cx).is_live());
            assert!(!shell.voice_stage_hold);
            assert_eq!(
                shell.state.read(cx).selected_chat.as_deref(),
                Some("voice-chat")
            );
        });
        shell.update(cx, |shell, cx| {
            shell
                .voice_dock
                .update(cx, |_, cx| cx.emit(crate::voice::VoiceDockEvent::OpenStage));
        });
        shell.read_with(cx, |shell, cx| assert!(shell.voice.read(cx).stage_open));
    }

    #[gpui::test]
    fn a_turn_with_more_calls_than_the_feed_holds_counts_the_earlier_ones(
        cx: &mut gpui::TestAppContext,
    ) {
        let (_dir, shell, cx) = shell_window(cx);
        live(&shell, true, cx);
        select(&shell, Some("voice-chat"), cx);
        let tools = (0..ACTIVITY_SHOWN + 2)
            .map(|index| {
                serde_json::json!({
                    "kind": "tool", "id": format!("t{index}"), "resolved": true, "isError": false,
                    "call": {"kind": "exec", "command": format!("step {index}")},
                })
            })
            .collect::<Vec<_>>();
        shell.update(cx, |shell, cx| {
            shell.state.update(cx, |state, _| {
                state.transcript = vec![
                    serde_json::from_value(serde_json::json!({
                        "id": "work",
                        "role": "assistant",
                        "createdAt": chrono::Utc::now().timestamp_millis() + 60_000,
                        "deviceId": "device",
                        "parts": tools,
                    }))
                    .unwrap(),
                ];
            });
            cx.notify();
        });
        // Drawing it at all is the point: the line once set a second hover
        // style on its link, which a debug build aborts on.
        cx.update(|window, cx| window.draw(cx).clear());
        let activity = cx.debug_bounds("voice-stage-activity").unwrap();
        let earlier = cx.debug_bounds("voice-stage-earlier").unwrap();
        assert_eq!(earlier.top(), activity.top() + px(ACTIVITY_GAP));
        assert_eq!(earlier.size.height, px(ACTIVITY_LINE));
        // It heads the turn's latest calls, the oldest of which are gone.
        assert!(cx.debug_bounds("voice-stage-tool-work-t1").is_none());
        let first = cx.debug_bounds("voice-stage-tool-work-t2").unwrap();
        assert_eq!(first.top(), earlier.bottom());
        // And leads to the conversation, where all of them are.
        cx.simulate_click(earlier.center(), gpui::Modifiers::default());
        shell.read_with(cx, |shell, cx| {
            assert!(!shell.voice.read(cx).stage_open);
            assert!(shell.voice.read(cx).is_live());
        });
    }

    #[gpui::test]
    fn the_shell_follows_the_call_for_its_cues(cx: &mut gpui::TestAppContext) {
        use crate::sound::VoiceCall;
        let (_dir, shell, cx) = shell_window(cx);
        shell.read_with(cx, |shell, _| assert_eq!(shell.voice_call, VoiceCall::Idle));
        live(&shell, false, cx);
        shell.read_with(cx, |shell, _| {
            assert_eq!(shell.voice_call, VoiceCall::Connected)
        });
        // A native stop is still the call it is stopping.
        shell.update(cx, |shell, cx| {
            shell.voice.update(cx, |voice, cx| {
                voice.active_since = Some(std::time::Instant::now());
                voice.phase = VoicePhase::Stopping;
                cx.notify();
            });
        });
        shell.read_with(cx, |shell, _| {
            assert_eq!(shell.voice_call, VoiceCall::Connected)
        });
        shell.update(cx, |shell, cx| {
            shell.voice.update(cx, |voice, cx| voice.cancel(cx));
        });
        shell.read_with(cx, |shell, _| assert_eq!(shell.voice_call, VoiceCall::Idle));
    }

    #[gpui::test]
    fn call_screen_holds_a_loader_until_the_call_connects(cx: &mut gpui::TestAppContext) {
        let (_dir, shell, cx) = shell_window(cx);
        shell.update(cx, |shell, cx| {
            shell.voice.update(cx, |voice, cx| {
                voice.phase = VoicePhase::Checking;
                voice.chat_id = Some("voice-chat".into());
                voice.set_stage_open(true, cx);
            });
        });
        cx.update(|window, cx| window.draw(cx).clear());
        // Starting: the orb is already in its place, about the middle of the
        // room (the turn it heads hangs below it, so the artwork keeps the
        // upper half), held back, with its own figure for a call that is
        // loading.
        let stage = cx.debug_bounds("voice-stage").unwrap();
        let waiting = cx.debug_bounds("voice-stage-orb").unwrap();
        let bar = cx.debug_bounds("voice-dock").unwrap();
        let top = stage.top() + px(Theme::TITLEBAR_HEIGHT);
        let bottom = bar.top() - px(STAGE_BAR_CLEARANCE);
        let middle = top + (bottom - top) / 2.0;
        let off = (waiting.center().y - middle).abs();
        assert!(off < (bottom - top) / 10.0, "{waiting:?} vs {middle:?}");
        assert!(waiting.bottom() > middle, "{waiting:?} vs {middle:?}");
        assert!(cx.debug_bounds("voice-stage-feed").is_none());
        shell.read_with(cx, |shell, cx| {
            assert_eq!(shell.voice.read(cx).status(), "Getting ready…");
        });

        // Connected: the orb comes alive where it was (nothing on the screen
        // moves) and the feed is under it.
        live(&shell, true, cx);
        let orb = cx.debug_bounds("voice-stage-orb").unwrap();
        // (To the pixel grid: the two sizes round differently.)
        assert!((orb.center().x - waiting.center().x).abs() < px(1.0));
        assert!((orb.center().y - waiting.center().y).abs() < px(1.0));
        assert!(orb.size.width > waiting.size.width);
        assert!(cx.debug_bounds("voice-stage-feed").is_some());
        assert_eq!(cx.debug_bounds("voice-dock").unwrap(), bar);
    }

    #[gpui::test]
    fn a_failed_start_stays_on_the_call_screen_with_a_way_out(cx: &mut gpui::TestAppContext) {
        let (_dir, shell, cx) = shell_window(cx);
        let fail = |cx: &mut gpui::VisualTestContext| {
            shell.update(cx, |shell, cx| {
                shell.voice.update(cx, |voice, cx| {
                    voice.phase = VoicePhase::Failed;
                    voice.reason =
                        Some(zeron_proto::voice::VoiceRejection::MicrophonePermissionDenied);
                    voice.set_stage_open(true, cx);
                    cx.notify();
                });
            });
            cx.update(|window, cx| window.draw(cx).clear());
        };
        fail(cx);
        assert!(cx.debug_bounds("voice-stage-failure").is_some());
        assert!(cx.debug_bounds("voice-stage-orb").is_none());
        assert!(cx.debug_bounds("voice-stage-retry").is_some());
        // No session, so no session controls.
        assert!(cx.debug_bounds("voice-dock").is_none());

        // Close puts the failure away with the screen.
        let close = cx.debug_bounds("voice-stage-close").unwrap();
        cx.simulate_click(close.center(), gpui::Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear());
        shell.read_with(cx, |shell, cx| {
            let voice = shell.voice.read(cx);
            assert!(!voice.stage_open && voice.reason.is_none());
            assert_eq!(voice.phase, VoicePhase::Closed);
        });
        assert!(cx.debug_bounds("voice-stage").is_none());

        // Escape does the same.
        fail(cx);
        shell.update(cx, |shell, cx| shell.set_voice_stage_open(false, cx));
        shell.read_with(cx, |shell, cx| {
            assert!(shell.voice.read(cx).reason.is_none());
        });
    }

    #[gpui::test]
    fn the_call_lists_what_codex_did_this_turn_and_where_each_call_leads(
        cx: &mut gpui::TestAppContext,
    ) {
        use crate::voice::Utterance;
        let chat = |id: &str, parent: Option<&str>, at_ms: i64| -> zeron_proto::Chat {
            serde_json::from_value(serde_json::json!({
                "id": id, "title": id, "deviceId": "local", "archived": false,
                "parentChatId": parent,
                "createdAt": chrono::DateTime::from_timestamp_millis(at_ms).unwrap(),
            }))
            .unwrap()
        };
        let (first, second, named) = (
            "aaaaaaaa-0000-4000-8000-000000000001",
            "bbbbbbbb-0000-4000-8000-000000000002",
            "cccccccc-0000-4000-8000-000000000003",
        );
        let chats = [
            // Listed newest first, as the sidebar sorts them. A standalone
            // chat has no parent; a side chat names the call's.
            chat(second, None, 1_700),
            chat(first, Some("voice-chat"), 1_600),
            // Before the turn, or another chat's worker: not the turn's.
            chat(
                "dddddddd-0000-4000-8000-000000000004",
                Some("voice-chat"),
                900,
            ),
            chat("eeeeeeee-0000-4000-8000-000000000005", Some(named), 1_650),
            chat(named, None, 100),
        ];
        let tool = |id: &str, call: serde_json::Value, resolved: bool, output: Option<&str>| {
            serde_json::json!({
                "kind": "tool", "id": id, "call": call, "resolved": resolved,
                "isError": resolved && output == Some("failed"), "output": output,
            })
        };
        let zeron =
            |name: &str| serde_json::json!({"kind": "mcp", "server": "zeron", "tool": name});
        let entry =
            |id: &str, at: i64, parts: serde_json::Value| -> zeron_doc::SessionMessageEntry {
                serde_json::from_value(serde_json::json!({
                    "id": id, "role": "assistant", "createdAt": at,
                    "deviceId": "device", "parts": parts,
                }))
                .unwrap()
            };
        let record = vec![
            // Before the call: part of the conversation, not of the call.
            entry(
                "old",
                500,
                serde_json::json!([tool(
                    "t0",
                    serde_json::json!({"kind": "exec", "command": "ls"}),
                    false,
                    None
                )]),
            ),
            entry(
                "work",
                1_500,
                serde_json::json!([
                    {"kind": "text", "id": "p0", "text": "typed reply"},
                    tool("t1", serde_json::json!({"kind": "webFetch", "url": "https://example.com/docs"}), true, None),
                    tool("t2", serde_json::json!({"kind": "webFetch", "url": "file:///etc/passwd"}), true, None),
                    tool("t3", zeron("create_chat"), true, Some("failed")),
                    tool("t4", zeron("create_chat"), true, Some("{…")),
                    tool("t5", zeron("create_chat"), true, None),
                    tool("t6", zeron("send_message"), true, Some(&format!("sent to {named}"))),
                    tool("t7", serde_json::json!({"kind": "exec", "command": "cargo test"}), false, None),
                ]),
            ),
        ];
        let voice = cx.new(|_| crate::voice::VoiceController::default());
        let said = |at_ms: i64, item: &str, role: VoiceRole, text: &str| Utterance {
            item_id: Some(item.into()),
            role: Some(role),
            text: text.into(),
            done: None,
            at_ms,
            heard: None,
        };
        voice.update(cx, |voice, _| {
            voice.started_ms = 1_000;
            voice.chat_id = Some("voice-chat".into());
            voice.utterances.extend([
                said(1_100, "u1", VoiceRole::User, "Run the tests in new chats"),
                // Opened by its start, with nothing said yet: not a turn.
                said(1_800, "u2", VoiceRole::User, "  "),
            ]);
        });
        let listed = |voice: &Entity<crate::voice::VoiceController>,
                      cx: &mut gpui::TestAppContext| {
            voice.read_with(cx, |voice, _| {
                let (lines, earlier) = recent_tools(voice, &record, &chats);
                (
                    lines
                        .into_iter()
                        .map(|line| (line.key, line.running, line.action))
                        .collect::<Vec<_>>(),
                    earlier,
                )
            })
        };
        // Since the user last spoke: the turn's latest calls, oldest first,
        // the rest counted. What came before the call never shows. A chat
        // the turn created is told by the order they appeared in (one that
        // failed created none), another by the id its result names; only a
        // web address leaves the app; the rest is shown in the conversation.
        let (lines, earlier) = listed(&voice, cx);
        assert_eq!(earlier, 4);
        assert_eq!(
            lines,
            [
                ("work-t5".to_owned(), false, ToolAction::Chat(second.into())),
                ("work-t6".to_owned(), false, ToolAction::Chat(named.into())),
                ("work-t7".to_owned(), true, ToolAction::Conversation),
            ]
        );
        let action = |call: serde_json::Value, made: Option<&zeron_proto::Chat>| {
            tool_action(
                &serde_json::from_value(call).unwrap(),
                None,
                made,
                &chats,
                Some("voice-chat"),
            )
        };
        let page = serde_json::json!({"kind": "webFetch", "url": "https://example.com/docs"});
        assert_eq!(
            action(page, None),
            ToolAction::Link("https://example.com/docs".into())
        );
        let file = serde_json::json!({"kind": "webFetch", "url": "file:///etc/passwd"});
        assert_eq!(action(file, None), ToolAction::Conversation);
        assert_eq!(
            action(zeron("create_chat"), Some(&chats[1])),
            ToolAction::Chat(first.into())
        );
        assert_eq!(action(zeron("list_chats"), None), ToolAction::Conversation);
        // The user speaks again: a new turn, and only a call still running
        // carries over.
        voice.update(cx, |voice, _| {
            voice
                .utterances
                .push_back(said(2_500, "u3", VoiceRole::User, "And the linter?"))
        });
        assert_eq!(
            listed(&voice, cx).0,
            [("work-t7".to_owned(), true, ToolAction::Conversation)]
        );
    }

    #[gpui::test]
    fn the_calls_bar_stays_in_reach_on_every_screen(cx: &mut gpui::TestAppContext) {
        // The shell's own layering of the two: the composer in the page,
        // the bar floated over it.
        struct Page(Entity<Shell>);
        impl Render for Page {
            fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                self.0.update(cx, |shell, cx| {
                    shell.viewport_width = f32::from(window.viewport_size().width);
                    shell.sync_voice_stage(window, cx);
                    div()
                        .size_full()
                        .relative()
                        .flex()
                        .flex_col()
                        .justify_end()
                        .child(shell.composer.clone())
                        .children(shell.render_voice_bar(cx))
                })
            }
        }
        let (_dir, shell, cx) = shell_window(cx);
        let (_, cx) = cx.add_window_view(|_, _| Page(shell.clone()));
        cx.update(|_, cx| motion::set_reduced_motion(cx, true));
        let draw = |cx: &mut gpui::VisualTestContext| {
            cx.update(|window, cx| window.draw(cx).clear());
        };
        let enter = |chat: Option<&str>, cx: &mut gpui::VisualTestContext| {
            let chat = chat.map(str::to_owned);
            shell.update(cx, |shell, cx| {
                shell
                    .state
                    .update(cx, |state, _| state.selected_chat = chat);
                cx.notify();
            });
            draw(cx);
        };
        draw(cx);
        assert!(cx.debug_bounds("voice-dock").is_none(), "no call, no bar");
        // Between its two places the bar never crosses the screen: it is at
        // one or the other, within a few points, and absent in the middle.
        {
            use crate::voice::float::{passage, place};
            let bar = gpui::size(px(200.0), px(36.0));
            let slot = gpui::Bounds::new(
                gpui::point(px(300.0), px(700.0)),
                gpui::size(px(700.0), px(36.0)),
            );
            let at = |amount| place(bar, (240.0, 1000.0), Some(slot), amount);
            let (corner, docked) = (at(0.0), at(1.0));
            assert!(docked.y - corner.y > px(600.0));
            for (amount, home) in [(0.2, corner), (0.45, corner), (0.55, docked), (0.8, docked)] {
                assert_eq!(at(amount).x, home.x);
                assert!((at(amount).y - home.y).abs() <= px(8.0), "{amount}");
            }
            assert_eq!(passage(0.5).1, 0.0);
            assert_eq!((passage(0.0), passage(1.0)), ((false, 1.0), (true, 1.0)));
        }
        // What floats above a composer stands beside the docked bar, on its
        // trailing side, when the composer has the room.
        assert_eq!(
            crate::voice::float::beside(200.0, 720.0, 164.0),
            Some(468.0)
        );
        assert_eq!(crate::voice::float::beside(200.0, 500.0, 164.0), None);
        // Live with its screen set aside, on the new-session screen: the bar
        // stands in the top trailing corner, as far from one edge as from
        // the other, with the call's controls on it, and the composer keeps
        // no room for it.
        shell.update(cx, |shell, cx| {
            shell.voice.update(cx, |voice, cx| {
                voice.phase = VoicePhase::Active;
                voice.chat_id = Some("voice-chat".into());
                cx.notify();
            });
        });
        draw(cx);
        let right = shell.read_with(cx, |shell, _| shell.viewport_width);
        let inset = px(crate::voice::float::CORNER_INSET);
        let top = cx.debug_bounds("voice-dock").unwrap();
        assert!(
            (top.right() - (px(right) - inset)).abs() < px(1.0),
            "{top:?}"
        );
        assert_eq!(top.top(), inset);
        for control in ["voice-dock-mute", "voice-orb", "voice-dock-end"] {
            assert!(top.contains(&cx.debug_bounds(control).unwrap().center()));
        }
        assert!(cx.debug_bounds("voice-dock-slot").is_none());
        // In a conversation it is the same bar, moved into the room the
        // composer keeps above itself.
        enter(Some("another-chat"), cx);
        let slot = cx.debug_bounds("voice-dock-slot").unwrap();
        let docked = cx.debug_bounds("voice-dock").unwrap();
        assert_eq!(docked.size, top.size);
        assert_eq!(docked.bottom(), slot.bottom());
        assert!((docked.center().x - slot.center().x).abs() < px(1.0));
        assert!(docked.top() > px(Theme::TITLEBAR_HEIGHT));
        // Back on a screen with no composer it returns to the top.
        enter(None, cx);
        assert_eq!(cx.debug_bounds("voice-dock").unwrap(), top);
        // The call screen has its own bar: this one steps aside.
        shell.update(cx, |shell, cx| shell.set_voice_stage_open(true, cx));
        draw(cx);
        assert!(cx.debug_bounds("voice-dock").is_none());
    }

    #[gpui::test]
    fn the_conversation_shows_what_is_said_as_it_is_said(cx: &mut gpui::TestAppContext) {
        use zeron_proto::voice::{VoiceEvent, VoiceTranscript};
        let (_dir, shell, cx) = shell_window(cx);
        live(&shell, false, cx);
        select(&shell, Some("voice-chat"), cx);
        let hear = |event: VoiceEvent, cx: &mut gpui::VisualTestContext| {
            shell.update(cx, |shell, cx| {
                shell.voice.update(cx, |voice, cx| voice.reduce(event, cx))
            });
        };
        let partial = |item: &str, text: &str, role: Option<VoiceRole>| VoiceEvent::Partial {
            generation: 2,
            item_id: Some(item.into()),
            text: text.into(),
            role,
        };
        let shown = |cx: &mut gpui::VisualTestContext| {
            shell.read_with(cx, |shell, cx| {
                shell
                    .state
                    .read(cx)
                    .pending_echoes()
                    .iter()
                    .map(|entry| {
                        let zeron_doc::MessagePart::Text { text, .. } = &entry.parts[0] else {
                            panic!("spoken words are text");
                        };
                        (entry.id.clone(), text.clone(), entry.status)
                    })
                    .collect::<Vec<_>>()
            })
        };
        let id = zeron_doc::voice::entry_id("one", "u1");
        let (streaming, complete) = (
            Some(zeron_doc::MessageStatus::Streaming),
            Some(zeron_doc::MessageStatus::Complete),
        );
        // An utterance that has a speaker but no words yet shows nothing.
        hear(partial("u1", "", Some(VoiceRole::User)), cx);
        assert!(shown(cx).is_empty());
        // Its words are in the conversation as they arrive, under the id
        // its record will have…
        hear(partial("u1", "Tell me", None), cx);
        assert_eq!(shown(cx), [(id.clone(), "Tell me".into(), streaming)]);
        hear(partial("u1", " a story", None), cx);
        assert_eq!(
            shown(cx),
            [(id.clone(), "Tell me a story".into(), streaming)]
        );
        // …and settle when it is finished, still ahead of the record.
        hear(
            VoiceEvent::Final {
                transcript: VoiceTranscript {
                    session_id: "one".into(),
                    item_id: "u1".into(),
                    role: VoiceRole::User,
                    text: "Tell me a story.".into(),
                    promoted_message_id: None,
                },
            },
            cx,
        );
        assert_eq!(
            shown(cx),
            [(id.clone(), "Tell me a story.".into(), complete)]
        );
        // The recorded entry takes the echo's place, and is not echoed again.
        shell.update(cx, |shell, cx| {
            shell.state.update(cx, |state, _| {
                state.apply_transcript(vec![
                    serde_json::from_value(serde_json::json!({
                        "id": id, "role": "user", "createdAt": 1, "deviceId": "device",
                        "parts": [{"kind": "text", "id": "t0", "text": "Tell me a story."}],
                    }))
                    .unwrap(),
                ])
            })
        });
        hear(partial("a1", "Once", Some(VoiceRole::Assistant)), cx);
        let answer = zeron_doc::voice::entry_id("one", "a1");
        assert_eq!(shown(cx), [(answer, "Once".into(), streaming)]);
        // What was still being said when the call ends is never recorded:
        // it leaves with the call.
        shell.update(cx, |shell, cx| {
            shell.voice.update(cx, |voice, cx| voice.cancel(cx))
        });
        assert!(shown(cx).is_empty());
    }

    #[gpui::test]
    fn the_call_says_where_it_runs_beside_its_controls(cx: &mut gpui::TestAppContext) {
        let (_dir, shell, cx) = shell_window(cx);
        live(&shell, true, cx);
        // A conversation the app does not list yet has nothing to say.
        assert!(cx.debug_bounds("voice-stage-context").is_none());
        let list = |title: &str, cx: &mut gpui::VisualTestContext| {
            let chat: zeron_proto::Chat = serde_json::from_value(serde_json::json!({
                "id": "voice-chat", "title": title, "deviceId": "device", "archived": false,
                "createdAt": chrono::Utc::now(),
            }))
            .unwrap();
            shell.update(cx, |shell, cx| {
                shell.state.update(cx, |state, _| state.chats = vec![chat]);
                cx.notify();
            });
            cx.update(|window, cx| window.draw(cx).clear());
            cx.debug_bounds("voice-stage-context").unwrap()
        };
        // Device and project sit at the bottom's leading edge, on the
        // controls' own line and clear of them, where a conversation keeps
        // its status too. Nothing stands in the title bar's corner or
        // between the feed and the controls.
        let context = list(crate::voice::VOICE_CHAT_TITLE, cx);
        let (stage, bar) = (
            cx.debug_bounds("voice-stage").unwrap(),
            cx.debug_bounds("voice-dock").unwrap(),
        );
        assert_eq!(context.left(), stage.left() + px(STAGE_CONTEXT_INSET));
        assert!((context.center().y - bar.center().y).abs() < px(1.0));
        assert!(context.right() < bar.left());
        assert!(cx.debug_bounds("voice-stage-heading").is_none());
    }

    #[gpui::test]
    fn a_new_call_enters_its_conversation_once_its_screen_covers_the_canvas(
        cx: &mut gpui::TestAppContext,
    ) {
        let (_dir, shell, cx) = shell_window(cx);
        live(&shell, true, cx);
        let chat: zeron_proto::Chat = serde_json::from_value(serde_json::json!({
            "id": "voice-chat", "title": "Voice session", "deviceId": "device",
            "archived": false, "createdAt": chrono::Utc::now(),
        }))
        .unwrap();
        shell.update(cx, |shell, cx| {
            shell.state.update(cx, |state, _| {
                state.chats = vec![chat];
                state.selected_chat = None;
            });
            shell.voice_pending_chat = Some("voice-chat".into());
            let selected = |shell: &Shell, cx: &App| shell.state.read(cx).selected_chat.clone();
            // The screen is still arriving over the new-session screen:
            // entering the conversation now would send the composer docking
            // across it. The canvas stays as it is.
            shell.voice_stage_progress = ARRIVING.beneath.1 / 2.0;
            assert!(shell.voice_stage_beneath() > 0.0);
            shell.follow_voice(cx);
            assert_eq!(selected(shell, cx), None);
            assert!(shell.voice_pending_chat.is_some());
            // Covered: the route changes where nobody sees it.
            shell.voice_stage_progress = ARRIVING.beneath.1;
            assert_eq!(shell.voice_stage_beneath(), 0.0);
            shell.follow_voice(cx);
            assert_eq!(selected(shell, cx).as_deref(), Some("voice-chat"));
            assert!(shell.voice_pending_chat.is_none());
        });
    }

    #[gpui::test]
    fn escape_on_the_call_screen_goes_to_what_is_open_over_it(cx: &mut gpui::TestAppContext) {
        // The whole shell, so the key takes its real path.
        struct Whole(Entity<Shell>);
        impl Render for Whole {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div().size_full().child(self.0.clone())
            }
        }
        let (_dir, shell, cx) = shell_window(cx);
        let (_, cx) = cx.add_window_view(|_, _| Whole(shell.clone()));
        shell.update(cx, |shell, cx| {
            // No engine in tests: render the workspace, not the boot gate.
            shell.debug_gate = Some(GatePhase::Ready);
            shell._state_observation = cx.observe(&shell.state, |_, _, _| {});
        });
        live(&shell, true, cx);
        cx.update(|window, cx| {
            window.draw(cx).clear();
            window.focus(&shell.read(cx).voice_stage_focus.clone(), cx);
        });
        let on_screen = |cx: &mut gpui::VisualTestContext| {
            shell.read_with(cx, |shell, cx| {
                let voice = shell.voice.read(cx);
                (
                    voice.stage_open,
                    voice.is_live(),
                    shell.command_palette.is_some(),
                )
            })
        };
        // The call screen is the conversation's view, not a sheet over it:
        // the session shortcuts work from it as from any conversation.
        shell.read_with(cx, |shell, cx| assert!(!shell.overlay_owns_keyboard(cx)));
        // The command palette opens over it, and Escape puts the palette
        // away: the screen does not take the key first.
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| shell.toggle_command_palette(window, cx));
            window.draw(cx).clear();
        });
        assert_eq!(on_screen(cx), (true, true, true));
        cx.simulate_keystrokes("escape");
        assert_eq!(on_screen(cx), (true, true, false));
        // With nothing over it, Escape leaves the call and its screen alone.
        cx.simulate_keystrokes("escape");
        assert_eq!(on_screen(cx), (true, true, false));
        // A failure shown there is closed by it.
        shell.update(cx, |shell, cx| {
            shell.voice.update(cx, |voice, cx| {
                voice.phase = VoicePhase::Failed;
                voice.reason = Some(zeron_proto::voice::VoiceRejection::Protocol);
                cx.notify();
            });
        });
        cx.update(|window, cx| window.draw(cx).clear());
        cx.simulate_keystrokes("escape");
        assert!(!on_screen(cx).0);
    }

    #[test]
    fn a_call_keeps_its_own_turns_quiet() {
        use crate::sound::Sound;
        assert!(call_silences(Sound::Done, true));
        // Every other conversation still rings, and so does anything in the
        // call's own that needs the user.
        assert!(!call_silences(Sound::Done, false));
        assert!(!call_silences(Sound::Request, true));
        assert!(!call_silences(Sound::Attention, true));
    }

    #[test]
    fn the_orb_arrives_with_the_call() {
        assert_eq!(orb_reveal(None, false), 0.0);
        assert_eq!(orb_reveal(None, true), 0.0);
        assert_eq!(orb_reveal(Some(0.0), false), 0.0);
        let midway = orb_reveal(Some(STAGE_ORB_ENTER_MS / 4.0), false);
        assert!(midway > 0.5 && midway < 1.0, "{midway}");
        assert_eq!(orb_reveal(Some(STAGE_ORB_ENTER_MS * 2.0), false), 1.0);
        assert_eq!(orb_reveal(Some(0.0), true), 1.0);
    }

    #[test]
    fn the_call_screen_turns_back_from_where_it_is_and_arrives_in_parts() {
        let near = |a: f32, b: f32| (a - b).abs() < 1e-4;
        // Frames at 60 a second, for `ms` in all.
        let run = |from: f32, open: bool, ms: f32| {
            (0..(ms / 16.0) as usize).fold(from, |at, _| {
                stage_progress(at, open, 16.0, false, ARRIVING.ms)
            })
        };
        assert_eq!(stage_progress(0.0, true, 0.0, false, ARRIVING.ms), 0.0);
        assert_eq!(run(0.0, true, ARRIVING.ms * 2.0), 1.0);
        let half = run(0.0, true, ARRIVING.ms / 2.0);
        assert!((half - 0.5).abs() < 0.03, "{half}");
        // A frame that took long holds the screen up: it does not swallow
        // the transition.
        let held = stage_progress(0.0, true, ARRIVING.ms, false, ARRIVING.ms);
        assert!(near(held, STEP_MS / ARRIVING.ms), "{held}");
        // Closed mid-way, it leaves from there (and faster than it came).
        let back = run(half, false, STAGE_EXIT_MS / 4.0);
        assert!(back > 0.2 && back < half - 0.2, "{back}");
        assert_eq!(run(back, false, STAGE_EXIT_MS), 0.0);
        assert_eq!(stage_progress(0.0, true, 0.0, true, ARRIVING.ms), 1.0);
        assert_eq!(stage_progress(1.0, false, 0.0, true, ARRIVING.ms), 0.0);
        // What it replaces has left before the controls are half-way in,
        // and the orb leads them.
        for way in [ARRIVING, PLACING] {
            let gone = way.beneath.1;
            assert_eq!(stage_part(gone, way.beneath), 1.0);
            assert!(stage_part(0.7, way.orb) > stage_part(0.7, way.controls));
            assert_eq!(stage_part(way.controls.0, way.controls), 0.0);
            assert_eq!(stage_part(1.0, way.orb), 1.0);
            assert_eq!(stage_part(1.0, way.controls), 1.0);
        }
        // Placing a call takes the long way in, with the orb growing into
        // place: Codex takes seconds to answer, and the screen spends the
        // first of them arriving.
        assert!(PLACING.ms > ARRIVING.ms * 3.0 && PLACING.orb_from < ARRIVING.orb_from);
        let placing = (0..(ARRIVING.ms / 16.0) as usize).fold(0.0, |at, _| {
            stage_progress(at, true, 16.0, false, PLACING.ms)
        });
        assert!(placing < 0.3, "{placing}");
    }
}
