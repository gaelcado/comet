//! The live session's controls: one surface in the composer's material, with
//! round glass plates for mute and the hang-up at its ends and the session
//! clock between them.
//!
//! The bar has two placements, both no wider than what they hold. Outside
//! the call screen it floats wherever the user is (`super::float`) and also
//! carries the call's orb in a simpler drawing, which says what the call is
//! doing without an icon; pressing the orb returns to the call screen. On
//! the call screen it is the control bar, under the orb and the feed.

use super::{VoiceController, VoiceState, format_elapsed};
use crate::{glass, icons, motion, theme::Theme};
use gpui::{
    App, Context, Entity, EventEmitter, SharedString, Subscription, Window, div, prelude::*, px,
};
use std::time::{Duration, Instant};
use zeron_proto::voice::VoicePhase;

const LINE_HEIGHT: f32 = 18.0;
/// The same morph as dictation, in and out.
const MORPH: Duration = Duration::from_millis(420);
/// An icon handing over to another inside a control.
const ICON_SWAP: Duration = Duration::from_millis(300);
const CURVE: motion::CubicBezier = motion::CubicBezier::new(0.2, 0.0, 0.0, 1.0);
/// The orb's light moves on this clock while a bar is in view: 30 frames a
/// second reads as continuous and costs half of every display frame. The
/// clock is the display's own: a frame is drawn on the first refresh at
/// least this long after the last one, so frames are evenly spaced whatever
/// the refresh rate, which a free-running timer cannot promise.
const FRAME: Duration = Duration::from_millis(33);
/// A refresh this close to the interval counts as on time.
const FRAME_SLACK: Duration = Duration::from_millis(3);

/// Where the bar stands, which decides its size and what it carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// Outside the call screen, wherever the user is: above the composer in
    /// a conversation, in the top corner of a screen without one
    /// ([`super::float`]).
    Floating,
    /// The call screen's control bar.
    Stage,
}

impl Placement {
    /// Height of the bar's surface. Floating, it is slim enough to stand
    /// in the title bar on that bar's own centre line; on the call screen
    /// it is a step taller.
    pub(crate) fn height(self) -> f32 {
        match self {
            Self::Floating => 36.0,
            Self::Stage => 44.0,
        }
    }

    /// Diameter of the round controls on that surface, the title bar's own
    /// control size when it floats. The surface's round ends are concentric
    /// with them: its inset from them is their gap from its edge.
    fn button(self) -> f32 {
        match self {
            Self::Floating => 28.0,
            Self::Stage => 32.0,
        }
    }

    /// The controls' glyphs.
    fn glyph(self) -> f32 {
        match self {
            Self::Floating => 16.0,
            Self::Stage => 16.0,
        }
    }
}

/// The orb in a bar, simpler than the call screen's: a small disc in its
/// light, in the room a round control has.
const ROW_ORB: super::orb::Shape = super::orb::Shape {
    diameter: 22.0,
    room: 28.0,
};
/// The resolution its fluid is computed at.
const ROW_ORB_RESOLUTION: u32 = 44;
/// Room for a bar's clock up to 9:59, kept before there is one.
const STAGE_CLOCK_WIDTH: f32 = 28.0;
/// The call screen's bar holds three groups: the two neutral controls, the
/// clock, and the hang-up. Controls of one group stand this far apart, and
/// groups more than twice as far, so the grouping reads from the spacing
/// alone.
const STAGE_GAP: f32 = 6.0;
const STAGE_GROUP_GAP: f32 = 14.0;

pub enum VoiceDockEvent {
    /// The session started or ended: the composer's voice button follows it.
    LiveChanged,
    OpenStage,
    /// Open the session's chat, where a pending question is answered.
    OpenTranscript(String),
}

/// Interruptible 0↔1 progress: a retarget starts from the current value, so
/// ending a session mid-entrance reverses without a jump.
#[derive(Clone, Copy)]
struct Tween {
    from: f32,
    to: f32,
    start: Instant,
    duration: Duration,
}

impl Tween {
    fn value(&self, now: Instant) -> f32 {
        let raw = now.saturating_duration_since(self.start).as_secs_f32()
            / (self.duration.as_secs_f32() * motion::speed_scale());
        motion::lerp(self.from, self.to, CURVE.eval(raw))
    }

    fn retarget(&mut self, to: f32, now: Instant, reduced: bool) {
        if self.to == to {
            return;
        }
        self.from = if reduced { to } else { self.value(now) };
        self.to = to;
        self.start = now;
    }
}

/// What the track last showed, kept so it can settle away after the session
/// ends instead of vanishing.
#[derive(Clone, Default)]
struct Shown {
    state: Option<VoiceState>,
    status: &'static str,
    clock: Option<String>,
    connected: bool,
}

pub struct VoiceDock {
    voice: Entity<VoiceController>,
    placement: Placement,
    live: bool,
    tween: Tween,
    /// 0 with the microphone open, 1 muted: the mute control's two icons
    /// and its plate cross between them.
    muting: Tween,
    shown: Shown,
    painted: Instant,
    /// An orb frame is being waited for, on the display's clock.
    ticking: bool,
    /// The floating bar's orb figure, between frames.
    orb: super::orb::Motion,
    /// The room a composer keeps for the floating bar, and how far the bar
    /// has travelled to it (0 at the top of the screen, 1 in that room).
    slot: super::float::Slot,
    /// Where the floating bar was last put.
    placed: super::float::Slot,
    /// The call screen, which has a bar of its own, is still in the
    /// floating bar's way though it has been set aside: its controls are
    /// leaving, and the two never show together.
    covered: bool,
    travel: f32,
    _voice: Subscription,
}

impl EventEmitter<VoiceDockEvent> for VoiceDock {}

impl VoiceDock {
    pub fn new(
        voice: Entity<VoiceController>,
        placement: Placement,
        cx: &mut Context<Self>,
    ) -> Self {
        let observation = cx.observe(&voice, |dock, voice, cx| {
            let live = voice.read(cx).is_live();
            if live && !dock.live {
                dock.shown = Shown::default();
            }
            if dock.live != live {
                dock.live = live;
                cx.emit(VoiceDockEvent::LiveChanged);
            }
            cx.notify();
        });
        let now = Instant::now();
        let live = voice.read(cx).is_live();
        Self {
            live,
            voice,
            placement,
            tween: Tween {
                from: 0.0,
                to: if live { 1.0 } else { 0.0 },
                start: now,
                duration: MORPH,
            },
            muting: Tween {
                from: 0.0,
                to: 0.0,
                start: now,
                duration: ICON_SWAP,
            },
            shown: Shown::default(),
            painted: now,
            ticking: false,
            orb: Default::default(),
            slot: Default::default(),
            placed: Default::default(),
            covered: false,
            travel: 1.0,
            _voice: observation,
        }
    }

    /// Wait, one display refresh at a time, for the next orb frame to
    /// be due, then redraw. Waiting costs nothing: no view is rebuilt until
    /// then.
    fn await_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.on_next_frame(window, |dock, window, cx| {
            if dock.painted.elapsed() + FRAME_SLACK >= FRAME {
                dock.ticking = false;
                cx.notify();
            } else {
                dock.await_frame(window, cx);
            }
        });
    }

    /// The room a composer keeps for the floating bar.
    pub(crate) fn slot(&self) -> super::float::Slot {
        self.slot.clone()
    }

    /// Whether the call screen, set aside, is still in the floating bar's
    /// way.
    pub(crate) fn covered(&self) -> bool {
        self.covered
    }

    pub(crate) fn set_covered(&mut self, covered: bool) {
        self.covered = covered;
    }

    /// Where the shell records the floating bar's place.
    pub(crate) fn placed(&self) -> super::float::Slot {
        self.placed.clone()
    }

    /// The floating bar docked above a composer and how wide it is there,
    /// once it has been drawn: what belongs above the composer stands
    /// beside it, not on top of it.
    pub(crate) fn docked(&self, cx: &App) -> Option<f32> {
        let bar = self
            .placed
            .get()
            .filter(|_| self.shown(cx) && !self.failed(cx) && self.reserve() > 0.5)?;
        Some(f32::from(bar.size.width))
    }

    /// How far the floating bar has travelled to that room, 0–1.
    pub(crate) fn travel(&self) -> f32 {
        self.travel
    }

    pub(crate) fn set_travel(&mut self, travel: f32) {
        self.travel = travel;
    }

    /// How much of that room is open: the bar is there, or on its way.
    pub(crate) fn reserve(&self) -> f32 {
        self.tween.value(Instant::now()).clamp(0.0, 1.0) * self.travel
    }

    /// A session is underway, so the composer offers no second one.
    pub fn live(&self) -> bool {
        self.live
    }

    /// A failure this row has to explain: one the call screen is not
    /// already showing.
    pub(crate) fn failed(&self, cx: &App) -> bool {
        let voice = self.voice.read(cx);
        voice.phase == VoicePhase::Failed && voice.reason.is_some() && !voice.stage_open
    }

    /// The composer row has something to show: a session whose call screen
    /// is set aside, one still settling away, or a failed start.
    pub fn shown(&self, cx: &App) -> bool {
        (self.live && !self.covered && !self.voice.read(cx).stage_open)
            || self.failed(cx)
            || self.tween.value(Instant::now()) > 0.0
    }

    /// A failed start explains itself where it was started; the next press
    /// of the voice button retries.
    fn render_failure(&self, theme: &Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        let message = SharedString::from(self.voice.read(cx).reason_text());
        let accent = theme.accent;
        let (hover, ink) = (theme.surface_raised_hover, theme.text);
        let dismiss = div()
            .id("voice-dock-dismiss")
            .debug_selector(|| "voice-dock-dismiss".into())
            .role(gpui::Role::Button)
            .aria_label("Dismiss voice message")
            .tab_index(0)
            .flex_none()
            .mt(px(-3.0))
            .h(px(24.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .rounded_full()
            .text_color(theme.text_muted)
            .cursor_pointer()
            .hover(move |s| s.bg(hover).text_color(ink))
            .focus_visible(move |s| s.border_1().border_color(accent))
            .on_click(cx.listener(|dock, _, _, cx| {
                dock.voice.update(cx, |voice, cx| voice.dismiss_reason(cx))
            }))
            .child("Dismiss");
        motion::fade_in(
            "voice-dock-failure-enter",
            div()
                .id("voice-dock-failure")
                .debug_selector(|| "voice-dock-failure".into())
                .flex()
                .items_start()
                .gap(px(8.0))
                .px(px(12.0))
                .text_size(px(12.0))
                .line_height(px(LINE_HEIGHT))
                .child(
                    div()
                        .flex_none()
                        .mt(px(6.0))
                        .size(px(6.0))
                        .rounded_full()
                        .bg(theme.warning),
                )
                .child(
                    div()
                        .id("voice-dock-failure-status")
                        .role(gpui::Role::Status)
                        .aria_label(format!("Voice didn’t start. {message}"))
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_wrap()
                        .gap_x(px(8.0))
                        .child(div().text_color(theme.text).child("Voice didn’t start"))
                        .child(div().min_w_0().text_color(theme.text_muted).child(message)),
                )
                .child(dismiss),
        )
        .into_any_element()
    }
}

/// A round glass control beside the track. `plate` paints its surface.
fn control(
    id: &'static str,
    label: &'static str,
    size: f32,
    theme: &Theme,
    plate: impl FnOnce(gpui::Div) -> gpui::Div,
) -> gpui::Stateful<gpui::Div> {
    let accent = theme.accent;
    plate(div().size(px(size)).flex_none().rounded_full())
        .id(id)
        .debug_selector(move || id.into())
        .role(gpui::Role::Button)
        .aria_label(label)
        .tab_index(0)
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|style| style.opacity(0.85))
        // Pressed reads at once, before anything the press sets off.
        .active(|style| style.opacity(0.7))
        .focus_visible(move |style| style.border_1().border_color(accent))
        .tooltip(crate::settings::widgets::text_tooltip(label))
}

/// The composer's own material on a pill: its tint, its hairline, and its
/// shadow where the theme has no blur, so the row reads as a slimmer
/// composer wherever it sits. `frost` blurs what is behind it, as the
/// composer does: worth a render pass of its own over a transcript or the
/// canvas artwork, wasted on the call screen, where nothing is under it.
fn surface(
    theme: &Theme,
    height: f32,
    t: f32,
    frost: bool,
    pill: gpui::Stateful<gpui::Div>,
) -> gpui::AnyElement {
    let lifted = !theme.is_frost();
    let pill = pill
        .h(px(height))
        .rounded_full()
        .border_1()
        .border_color(theme.composer_surface_border().opacity(t))
        .bg(theme.composer_surface_bg().opacity(t))
        .when(lifted && height < 32.0, |el| el.shadow_sm())
        .when(lifted && height >= 32.0, |el| el.shadow_lg());
    if frost {
        crate::frost::frosted(height / 2.0, crate::frost::MENU_BLUR, pill).into_any_element()
    } else {
        pill.into_any_element()
    }
}

impl Render for VoiceDock {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let now = Instant::now();
        let reduced = motion::reduced_motion(cx);
        let placement = self.placement;
        let (height, button, glyph) = (placement.height(), placement.button(), placement.glyph());
        let compact = placement == Placement::Floating;
        let voice = self.voice.read(cx);
        let live = voice.is_live();
        let active = voice.phase == VoicePhase::Active;
        // Everywhere but on the call screen the row steps aside for it: the
        // screen carries its own bar.
        let present =
            live && (placement == Placement::Stage || !(voice.stage_open || self.covered));
        let heard = voice.heard();
        let muted = voice.muted();
        let awaiting = voice.awaiting();
        let chat_id = voice.chat_id.clone();
        if live {
            self.shown = Shown {
                state: voice.state(),
                status: voice.status(),
                clock: voice
                    .active_since
                    .map(|since| format_elapsed(since.elapsed().as_secs())),
                connected: active,
            };
        }
        let label = format!("Voice. {}", self.shown.status);

        self.painted = now;
        self.tween
            .retarget(if present { 1.0 } else { 0.0 }, now, reduced);
        let t = self.tween.value(now).clamp(0.0, 1.0);
        if !reduced && t != self.tween.to {
            window.request_animation_frame();
        }
        // The orb moves on its own clock, and only while someone can see it.
        if present && !reduced && window.is_window_active() && !self.ticking {
            self.ticking = true;
            self.await_frame(window, cx);
        }
        if compact && self.failed(cx) {
            return self.render_failure(&theme, cx);
        }
        if t <= 0.0 && !present {
            return gpui::Empty.into_any_element();
        }

        let shown = self.shown.clone();
        // The bar's orb: the call's audio and its state in one place, the
        // call screen's own figure with fewer dots, so no icon repeats the
        // controls beside it. The words stay available as its tooltip and
        // to assistive technology.
        let row_orb = compact.then(|| {
            let frame = self.orb.frame(
                super::orb::State::of(shown.state, false),
                heard,
                &theme,
                ROW_ORB_RESOLUTION,
                (ROW_ORB.diameter * window.scale_factor()).round() as u32,
                now,
                reduced,
                window,
            );
            let alive = if shown.connected { t } else { 0.0 };
            super::orb::orb(ROW_ORB, frame, alive, |disc| disc.into_any_element())
                .id("voice-orb")
                .debug_selector(|| "voice-orb".into())
                .tooltip(crate::settings::widgets::text_tooltip(shown.status))
        });
        let clock = shown.clock.clone().map(|clock| {
            div()
                .flex_none()
                .font_family(theme.font_mono.clone())
                .text_size(px(11.0))
                .text_color(theme.text_muted)
                .child(SharedString::from(clock))
        });
        // The surface is the track: what the call reports sits straight on
        // it, between the controls.
        let track = div()
            .id("voice-dock-track")
            .debug_selector(|| "voice-dock-track".into())
            .h(px(button))
            .rounded_full()
            .flex()
            .items_center()
            .text_size(px(13.0))
            .line_height(px(LINE_HEIGHT));
        // Above the composer the whole track is the way back to the call
        // screen; on that screen it only reports.
        let track = if compact {
            let (wash, focus) = (theme.ink(0.06), theme.ink(0.1));
            track
                .role(gpui::Role::Button)
                .aria_label(format!("{label}. Open the call screen"))
                .tab_index(0)
                .cursor_pointer()
                .hover(move |style| style.bg(wash))
                .active(|style| style.opacity(0.7))
                .focus_visible(move |style| style.bg(focus))
                .tooltip(crate::settings::widgets::text_tooltip("Open voice"))
                .on_click(cx.listener(|_, _, _, cx| cx.emit(VoiceDockEvent::OpenStage)))
                .flex_none()
                .pr(px(10.0))
                .gap(px(6.0))
                .children(row_orb)
                .child(
                    div()
                        .min_w(px(STAGE_CLOCK_WIDTH))
                        .flex()
                        .justify_center()
                        .children(clock),
                )
        } else {
            // On the call screen the orb is the screen's own: the bar keeps
            // the controls and the clock, and is no wider than they are.
            // The clock is text on the bar, in no box of its own: what
            // stands on either side of it is the same distance away. Its
            // place is kept before there is one, so the bar is the same
            // width from the first frame.
            track
                .role(gpui::Role::Status)
                .aria_label(label)
                .flex_none()
                .mx(px(STAGE_GROUP_GAP - STAGE_GAP))
                .child(
                    div()
                        .min_w(px(STAGE_CLOCK_WIDTH))
                        .flex()
                        .justify_center()
                        .children(clock),
                )
        };

        // Mute keeps its place from the first frame: until the microphone is
        // open it is there but inert, so nothing shifts when the call
        // connects. Its two icons hand over with a scale and a fade, and the
        // plate inverts under them.
        self.muting
            .retarget(if muted { 1.0 } else { 0.0 }, now, reduced);
        let m = self.muting.value(now).clamp(0.0, 1.0);
        if !reduced && m != self.muting.to {
            window.request_animation_frame();
        }
        let usable = shown.connected;
        let ink = motion::mix(theme.text, theme.on_solid, m);
        let swap = |path: &'static str, amount: f32| {
            let scale = 0.25 + 0.75 * amount;
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    icons::icon(path)
                        .size(px(glyph))
                        .text_color(ink.opacity(amount))
                        .with_transformation(gpui::Transformation::scale(gpui::size(scale, scale))),
                )
        };
        let mute = control(
            "voice-dock-mute",
            if muted {
                "Unmute microphone"
            } else {
                "Mute microphone"
            },
            button,
            &theme,
            |el| crate::dictation::glass::light(el, &theme, t),
        )
        .when(m > 0.0, |el| {
            el.child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded_full()
                    .bg(theme.solid.opacity(m * t)),
            )
        })
        .when(m < 1.0, |el| el.child(swap(icons::MICROPHONE, 1.0 - m)))
        .when(m > 0.0, |el| el.child(swap(icons::MICROPHONE_OFF, m)))
        .when(usable, |el| {
            el.on_click(cx.listener(|dock, _, _, cx| {
                dock.voice.update(cx, |voice, cx| voice.toggle_mute(cx))
            }))
        })
        .when(!usable, |el| el.opacity(0.4).cursor_default());

        // The way to the conversation itself: on the call screen it is
        // always there (the call sets its screen aside and the ordinary
        // transcript shows, with the compact row to come back by); the
        // compact row only makes room for it while Codex waits on an answer.
        let chat = chat_id.filter(|_| !compact || awaiting).map(|chat_id| {
            control(
                "voice-dock-transcript",
                if awaiting {
                    "Answer Codex in the conversation"
                } else {
                    "Show the conversation"
                },
                button,
                &theme,
                |el| crate::dictation::glass::light(el, &theme, t),
            )
            .on_click(cx.listener(move |_, _, _, cx| {
                cx.emit(VoiceDockEvent::OpenTranscript(chat_id.clone()))
            }))
            .child(
                icons::icon(icons::CHAT_ROUND_LINE)
                    .size(px(glyph))
                    .text_color(if awaiting { theme.warning } else { theme.text }),
            )
        });

        // The filled glass plate on the theme's destructive fill. It holds still: the
        // orb is what follows the voice.
        let end = control("voice-dock-end", "End voice", button, &theme, |el| {
            glass::danger_plate(&theme, t).apply(el)
        })
        .on_click(cx.listener(|dock, _, _, cx| dock.voice.update(cx, |voice, cx| voice.cancel(cx))))
        .child(
            icons::icon(icons::PHONE_HANG_UP)
                .size(px(glyph))
                .text_color(gpui::white().opacity(t)),
        );

        // One surface in the composer's material, with the controls on it in
        // dictation's order: the secondary control on the leading side, what
        // the call reports between, the action that ends it on the trailing
        // side. The hairline counts in the inset, which keeps the surface's
        // round ends concentric with the controls. On the call screen the
        // two neutral controls stand together, then the clock, then the
        // hang-up; above the composer the track is the way back and comes
        // first, the conversation control only joining it for a question.
        let bar = div()
            .id("voice-dock")
            .debug_selector(|| "voice-dock".into())
            .px(px((height - button) / 2.0 - 1.0))
            .flex()
            .items_center()
            .gap(px(if compact { 8.0 } else { STAGE_GAP }))
            .child(mute);
        let bar = if compact {
            bar.child(track).children(chat)
        } else {
            bar.children(chat).child(track)
        }
        .child(end);
        // No wider than what it holds. On the call screen it stands under
        // the feed; elsewhere the shell floats it, over a transcript or the
        // new-session artwork, which it blurs as the composer does. It
        // takes the presses meant for it, never the title bar's drag.
        div()
            .flex()
            .opacity(t)
            .child(surface(&theme, height, t, compact, bar.occlude()))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeron_proto::voice::{
        VoiceEvent, VoiceRejection, VoiceRole, VoiceSnapshot, VoiceTranscript, VoiceWork,
    };

    struct Host(Entity<VoiceDock>);
    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().w(px(640.0)).child(self.0.clone())
        }
    }

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

    fn partial(item: &str, text: &str, role: Option<VoiceRole>) -> VoiceEvent {
        VoiceEvent::Partial {
            generation: 2,
            item_id: Some(item.into()),
            text: text.into(),
            role,
        }
    }

    /// Reduced motion snaps the morph, so bounds are final on the first draw.
    fn dock_window(
        placement: Placement,
        cx: &mut gpui::TestAppContext,
    ) -> (
        tempfile::TempDir,
        Entity<VoiceController>,
        Entity<VoiceDock>,
        &mut gpui::VisualTestContext,
    ) {
        let dir = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            gpui_base::init(cx);
            cx.set_global(Theme::default());
            crate::settings::init(crate::settings::UiSettings::default(), dir.path(), cx);
            motion::set_reduced_motion(cx, true);
        });
        let voice = cx.new(|_| VoiceController::default());
        let (host, cx) = cx.add_window_view(|_, cx| {
            let voice = voice.clone();
            Host(cx.new(|cx| VoiceDock::new(voice, placement, cx)))
        });
        let dock = host.read_with(cx, |host, _| host.0.clone());
        (dir, voice, dock, cx)
    }

    fn events(dock: &Entity<VoiceDock>, cx: &mut gpui::VisualTestContext) -> EventLog {
        let log = EventLog::default();
        let sink = log.0.clone();
        let subscription = cx.update(|_, cx| {
            cx.subscribe(dock, move |_, event: &VoiceDockEvent, _| {
                sink.borrow_mut().push(match event {
                    VoiceDockEvent::LiveChanged => "live".to_owned(),
                    VoiceDockEvent::OpenStage => "stage".to_owned(),
                    VoiceDockEvent::OpenTranscript(chat) => format!("chat:{chat}"),
                });
            })
        });
        subscription.detach();
        log
    }

    #[derive(Default)]
    struct EventLog(std::rc::Rc<std::cell::RefCell<Vec<String>>>);
    impl EventLog {
        fn seen(&self) -> Vec<String> {
            self.0.borrow().clone()
        }
    }

    fn connect(voice: &Entity<VoiceController>, cx: &mut gpui::VisualTestContext) {
        voice.update(cx, |voice, cx| {
            voice.chat_id = Some("chat".into());
            voice.reduce(
                VoiceEvent::Snapshot {
                    snapshot: snapshot(),
                },
                cx,
            );
        });
    }

    fn draw(cx: &mut gpui::VisualTestContext) {
        cx.update(|window, cx| window.draw(cx).clear());
    }

    fn click(selector: &'static str, cx: &mut gpui::VisualTestContext) {
        let bounds = cx.debug_bounds(selector).unwrap();
        cx.simulate_click(bounds.center(), gpui::Modifiers::default());
    }

    #[gpui::test]
    fn compact_row_follows_the_session_and_its_controls_drive_it(cx: &mut gpui::TestAppContext) {
        let (_dir, voice, dock, cx) = dock_window(Placement::Floating, cx);
        let log = events(&dock, cx);

        draw(cx);
        assert!(cx.debug_bounds("voice-dock").is_none());
        dock.read_with(cx, |dock, cx| assert!(!dock.live() && !dock.shown(cx)));

        connect(&voice, cx);
        draw(cx);
        dock.read_with(cx, |dock, cx| assert!(dock.live() && dock.shown(cx)));
        assert_eq!(log.seen(), ["live"]);

        // One slim surface, no wider than what it holds, with the controls
        // on it in dictation's order: mute on the leading edge, the orb and
        // the clock between, the hang-up on the trailing edge, all inset
        // from its edge by the same amount.
        let (height, button) = (Placement::Floating.height(), Placement::Floating.button());
        let inset = px((height - button) / 2.0);
        let resting = cx.debug_bounds("voice-dock").unwrap();
        let (mute, track, end) = (
            cx.debug_bounds("voice-dock-mute").unwrap(),
            cx.debug_bounds("voice-dock-track").unwrap(),
            cx.debug_bounds("voice-dock-end").unwrap(),
        );
        assert_eq!(resting.size.height, px(height));
        assert!(resting.size.width < px(200.0), "{:?}", resting.size.width);
        assert_eq!(mute.left(), resting.left() + inset);
        assert_eq!(end.right(), resting.right() - inset);
        for control in [mute, end] {
            assert_eq!(control.size, gpui::size(px(button), px(button)));
            assert_eq!(control.top(), resting.top() + inset);
        }
        assert!(mute.right() < track.left() && track.right() < end.left());
        // No way to the chat or back to chats here: the track is the way
        // to the call screen, and nothing is pending.
        assert!(cx.debug_bounds("voice-dock-transcript").is_none());

        // What is said never changes it: the words are heard, and read in
        // the conversation or on the call screen.
        voice.update(cx, |voice, cx| {
            voice.reduce(partial("a1", &"word ".repeat(120), None), cx)
        });
        draw(cx);
        assert_eq!(cx.debug_bounds("voice-dock").unwrap(), resting);

        click("voice-dock", cx);
        assert_eq!(log.seen(), ["live", "stage"]);

        // A question from Codex brings the chat control in beside mute.
        voice.update(cx, |voice, cx| {
            let mut asking = snapshot();
            asking.work = VoiceWork::AwaitingInput;
            voice.reduce(VoiceEvent::Snapshot { snapshot: asking }, cx);
        });
        draw(cx);
        click("voice-dock-transcript", cx);
        assert_eq!(log.seen(), ["live", "stage", "chat:chat"]);

        // The call screen takes over: the row steps aside, still live.
        voice.update(cx, |voice, cx| voice.set_stage_open(true, cx));
        draw(cx);
        dock.read_with(cx, |dock, cx| assert!(dock.live() && !dock.shown(cx)));
        assert!(cx.debug_bounds("voice-dock").is_none());
        voice.update(cx, |voice, cx| voice.set_stage_open(false, cx));
        draw(cx);

        click("voice-dock-mute", cx);
        // The hang-up ends the session and the row leaves with it.
        click("voice-dock-end", cx);
        draw(cx);
        voice.read_with(cx, |voice, _| assert!(!voice.is_live()));
        dock.read_with(cx, |dock, cx| assert!(!dock.live() && !dock.shown(cx)));
        assert!(cx.debug_bounds("voice-dock").is_none());
        assert_eq!(log.seen().last().map(String::as_str), Some("live"));
    }

    #[gpui::test]
    fn call_screen_bar_leads_to_the_conversation_and_ends_the_call(cx: &mut gpui::TestAppContext) {
        let (_dir, voice, dock, cx) = dock_window(Placement::Stage, cx);
        let log = events(&dock, cx);
        connect(&voice, cx);
        voice.update(cx, |voice, cx| {
            voice.set_stage_open(true, cx);
            // Words are the transcript's business here, never the bar's.
            voice.reduce(partial("a1", &"word ".repeat(120), None), cx);
        });
        draw(cx);
        let (height, button) = (Placement::Stage.height(), Placement::Stage.button());
        let bar = cx.debug_bounds("voice-dock").unwrap();
        // No wider than its controls: the orb above it carries the call.
        assert_eq!(bar.size.height, px(height));
        assert!(bar.size.width < px(240.0), "{:?}", bar.size.width);
        assert!(cx.debug_bounds("voice-dock-mute").unwrap().left() > bar.left());
        // The conversation's own view: there is nothing to minimize to.
        assert_eq!(
            cx.debug_bounds("voice-dock-end").unwrap().right(),
            bar.right() - px((height - button) / 2.0)
        );
        for selector in ["voice-dock-mute", "voice-dock-end"] {
            let control = cx.debug_bounds(selector).unwrap();
            assert_eq!(control.size, gpui::size(px(button), px(button)));
        }
        // The ordinary transcript is one press away at any time, beside
        // mute: the two neutral controls, then the clock, then the hang-up.
        let across = [
            "voice-dock-mute",
            "voice-dock-transcript",
            "voice-dock-track",
            "voice-dock-end",
        ]
        .map(|part| cx.debug_bounds(part).unwrap());
        assert!(
            across
                .windows(2)
                .all(|pair| pair[0].right() <= pair[1].left())
        );
        // The spacing says the same: the two controls stand together, and
        // the clock is as far from them as from the hang-up, more than
        // twice as far as they are from one another.
        let [mute, transcript, clock, end] = across;
        let together = transcript.left() - mute.right();
        let apart = clock.left() - transcript.right();
        assert_eq!(apart, end.left() - clock.right());
        assert!(apart > together * 2.0, "{together:?} {apart:?}");
        // The state is at the orb's centre on this screen, not in the track.
        assert!(cx.debug_bounds("voice-state").is_none());
        // The track reports; it is not a button on the call screen.
        click("voice-dock", cx);
        click("voice-dock-transcript", cx);
        assert_eq!(log.seen(), ["live", "chat:chat"]);
        click("voice-dock-end", cx);
        draw(cx);
        assert!(cx.debug_bounds("voice-dock").is_none());
    }

    #[gpui::test]
    fn the_state_is_a_mark_in_a_constant_slot_never_a_caption(cx: &mut gpui::TestAppContext) {
        // The rows that have no orb beside them carry the state themselves.
        let (_dir, voice, _dock, cx) = dock_window(Placement::Floating, cx);
        voice.update(cx, |voice, cx| {
            voice.chat_id = Some("chat".into());
            voice.phase = VoicePhase::Checking;
            cx.notify();
        });
        draw(cx);
        voice.read_with(cx, |voice, _| {
            assert_eq!(voice.state(), Some(VoiceState::Connecting))
        });
        // The orb carries the state, in the room of a round control.
        let slot = cx.debug_bounds("voice-orb").unwrap();
        let edge = px(ROW_ORB.room);
        assert_eq!(slot.size, gpui::size(edge, edge));
        let bar = cx.debug_bounds("voice-dock").unwrap();

        connect(&voice, cx);
        let mut seen = vec![VoiceState::Connecting];
        let changes: [fn(&mut VoiceSnapshot); 5] = [
            |_| {},
            |s| s.playing = true,
            |s| s.muted = true,
            |s| s.work = VoiceWork::Working,
            |s| s.work = VoiceWork::AwaitingInput,
        ];
        for (index, change) in changes.into_iter().enumerate() {
            voice.update(cx, |voice, cx| {
                let mut next = snapshot();
                change(&mut next);
                voice.reduce(VoiceEvent::Snapshot { snapshot: next }, cx);
                seen.push(voice.state().unwrap());
            });
            draw(cx);
            // Whatever the state, the orb says it itself, in the same room:
            // no icon repeats the controls beside it, and the state is never
            // spelled out.
            let orb = cx.debug_bounds("voice-orb").unwrap();
            assert_eq!(orb.size, slot.size);
            assert!(cx.debug_bounds("voice-state").is_none());
            // Nothing moves between states. Only a question changes the
            // bar: it gains the way to answer it.
            if index < 4 {
                assert_eq!(orb, slot);
                assert_eq!(cx.debug_bounds("voice-dock").unwrap(), bar);
            } else {
                assert!(cx.debug_bounds("voice-dock-transcript").is_some());
            }
        }
        assert_eq!(
            seen,
            [
                VoiceState::Connecting,
                VoiceState::Listening,
                VoiceState::Speaking,
                VoiceState::Muted,
                VoiceState::Working,
                VoiceState::Asking,
            ]
        );
        voice.update(cx, |voice, cx| voice.cancel(cx));
        voice.read_with(cx, |voice, _| assert_eq!(voice.state(), None));
    }

    #[gpui::test]
    fn row_morphs_in_and_settles_away_after_the_session_ends(cx: &mut gpui::TestAppContext) {
        let (_dir, voice, dock, cx) = dock_window(Placement::Floating, cx);
        cx.update(|_, cx| motion::set_reduced_motion(cx, false));
        connect(&voice, cx);
        draw(cx);
        dock.update(cx, |dock, _| {
            let start = dock.tween.start;
            assert_eq!((dock.tween.from, dock.tween.to), (0.0, 1.0));
            let half = dock.tween.value(start + MORPH / 2);
            assert!(half > 0.0 && half < 1.0, "{half}");
            assert_eq!(dock.tween.value(start + MORPH * 2), 1.0);
            // Let the entrance finish, then end the session.
            dock.tween.start = start - MORPH * 2;
        });
        voice.update(cx, |voice, cx| voice.cancel(cx));
        draw(cx);
        dock.update(cx, |dock, cx| {
            // Still on screen, settling away from where it was.
            assert!(!dock.live() && dock.shown(cx));
            assert_eq!((dock.tween.from, dock.tween.to), (1.0, 0.0));
            dock.tween.start -= MORPH * 2;
            assert!(!dock.shown(cx));
        });
    }

    #[gpui::test]
    fn the_orb_follows_whoever_is_talking(cx: &mut gpui::TestAppContext) {
        let (_dir, voice, dock, cx) = dock_window(Placement::Floating, cx);
        voice.update(cx, |voice, cx| {
            voice.chat_id = Some("chat".into());
            voice.phase = VoicePhase::Checking;
            cx.notify();
        });
        draw(cx);
        // Connecting: no capture yet, so no light and no clock.
        dock.read_with(cx, |dock, _| {
            assert!(!dock.shown.connected && dock.shown.clock.is_none());
            assert_eq!(dock.shown.state, Some(VoiceState::Connecting));
        });
        connect(&voice, cx);
        voice.update(cx, |voice, cx| {
            voice.reduce(
                VoiceEvent::Levels {
                    generation: 2,
                    microphone: u16::MAX / 4,
                    speaker: 0,
                },
                cx,
            );
        });
        draw(cx);
        dock.read_with(cx, |dock, _| {
            assert!(dock.shown.connected && dock.shown.clock.is_some());
        });
        voice.update(cx, |voice, cx| {
            let mut speaking = snapshot();
            speaking.playing = true;
            voice.reduce(VoiceEvent::Snapshot { snapshot: speaking }, cx);
        });
        draw(cx);
        dock.read_with(cx, |dock, _| {
            assert_eq!(dock.shown.state, Some(VoiceState::Speaking));
        });
    }

    #[gpui::test]
    fn failed_start_explains_itself_above_the_composer_until_dismissed(
        cx: &mut gpui::TestAppContext,
    ) {
        let (_dir, voice, dock, cx) = dock_window(Placement::Floating, cx);
        voice.update(cx, |voice, cx| {
            voice.phase = VoicePhase::Failed;
            voice.reason = Some(VoiceRejection::MicrophonePermissionDenied);
            cx.notify();
        });
        draw(cx);
        dock.read_with(cx, |dock, cx| assert!(!dock.live() && dock.shown(cx)));
        assert!(cx.debug_bounds("voice-dock-failure").is_some());
        click("voice-dock-dismiss", cx);
        draw(cx);
        dock.read_with(cx, |dock, cx| assert!(!dock.shown(cx)));
        assert!(cx.debug_bounds("voice-dock-failure").is_none());
    }

    #[gpui::test]
    fn transcript_keeps_both_sides_in_order_and_settles_finals_in_place(
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
            let texts = |voice: &VoiceController| -> Vec<(Option<VoiceRole>, String, bool)> {
                voice
                    .utterances
                    .iter()
                    .map(|u| (u.role, u.text.clone(), u.done.is_some()))
                    .collect()
            };
            // The provider names the speaker as the utterance starts.
            voice.reduce(partial("u1", "", Some(VoiceRole::User)), cx);
            assert!(voice.current().is_none(), "nothing said yet");
            voice.reduce(partial("u1", "Open the ", None), cx);
            voice.reduce(partial("u1", "landing page", None), cx);
            assert_eq!(
                texts(voice),
                [(Some(VoiceRole::User), "Open the landing page".into(), false)]
            );
            assert!(voice.caption_fresh(Duration::ZERO));

            // The reply starts while the user's final is still on its way:
            // each lands in its own utterance, in the order they began.
            voice.reduce(partial("a1", "", Some(VoiceRole::Assistant)), cx);
            voice.reduce(partial("a1", "On it.", None), cx);
            voice.reduce(
                VoiceEvent::Final {
                    transcript: VoiceTranscript {
                        session_id: "one".into(),
                        item_id: "u1".into(),
                        role: VoiceRole::User,
                        text: "Open the landing page.".into(),
                        promoted_message_id: None,
                    },
                },
                cx,
            );
            assert_eq!(
                texts(voice),
                [
                    (Some(VoiceRole::User), "Open the landing page.".into(), true),
                    (Some(VoiceRole::Assistant), "On it.".into(), false),
                ]
            );
            // The caption is the newest utterance, still being said.
            assert_eq!(
                crate::voice::caption(voice, 80),
                Some(("On it.".into(), Some(VoiceRole::Assistant)))
            );
            assert!(voice.caption_fresh(Duration::ZERO));
            voice.reduce(
                VoiceEvent::Final {
                    transcript: VoiceTranscript {
                        session_id: "one".into(),
                        item_id: "a1".into(),
                        role: VoiceRole::Assistant,
                        text: "On it. Opening it now.".into(),
                        promoted_message_id: None,
                    },
                },
                cx,
            );
            // Finished: it stays only as long as the surface holds it.
            assert!(voice.caption_fresh(Duration::from_secs(60)));
            assert!(!voice.caption_fresh(Duration::ZERO));
            assert_eq!(voice.utterances.len(), 2);

            // A final the session never saw start still joins the transcript.
            voice.reduce(
                VoiceEvent::Final {
                    transcript: VoiceTranscript {
                        session_id: "one".into(),
                        item_id: "u2".into(),
                        role: VoiceRole::User,
                        text: "Thanks".into(),
                        promoted_message_id: None,
                    },
                },
                cx,
            );
            assert_eq!(voice.utterances.len(), 3);
            assert_eq!(voice.status(), "Listening");
            voice.cancel(cx);
            assert!(voice.utterances.is_empty());
        });
    }
}
