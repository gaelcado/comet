//! The voice orchestrator's chrome: the sidebar footer trigger and the
//! full-window stage. A voice session follows no chat selection, so neither
//! surface lives inside the conversation column — the user moves between
//! threads freely while the orchestrator keeps listening.

use super::*;
use zeron_proto::voice::{VoicePhase, VoiceWork};

/// The stage paints the hero preset magnified to fill the canvas.
pub(super) const VOICE_STAGE_ORB_SCALE: f32 = 2.25;
/// Stage orb footprint, including breathing room around the artwork.
const STAGE_ORB_BOX: f32 = 128.0 * VOICE_STAGE_ORB_SCALE + 112.0;
const STAGE_ENTER_MS: f32 = 460.0;
const STAGE_EXIT_MS: f32 = 220.0;
/// Captions show the tail of a long utterance rather than wrapping off-stage.
const STAGE_CAPTION_CHARS: usize = 160;
const FOOTER_HOVER: &str = "voice-footer-orb";
/// Call-bar geometry: round controls inside a floating pill.
const BAR_CONTROL: f32 = 44.0;
const BAR_PAD: f32 = 8.0;

/// Text tail of `text` holding at most `max` characters, ellipsized in front.
pub(super) fn caption_tail(text: &str, max: usize) -> String {
    let text = text.trim();
    let count = text.chars().count();
    if count <= max {
        return text.to_owned();
    }
    let tail: String = text.chars().skip(count - max).collect();
    format!("…{}", tail.trim_start())
}

/// Stage visibility for a transition that flipped `elapsed_ms` ago.
pub(super) fn stage_reveal(open: bool, elapsed_ms: f32, reduced: bool) -> f32 {
    match (open, reduced) {
        (true, true) => 1.0,
        (false, true) => 0.0,
        (true, false) => motion::EASE_OUT_EXPO.eval(elapsed_ms / STAGE_ENTER_MS),
        (false, false) => 1.0 - motion::EASE.eval(elapsed_ms / STAGE_EXIT_MS),
    }
}

impl Shell {
    /// The footer microphone: a new orchestrator in a projectless Codex chat
    /// on this device. The chat's model follows the composer when it already
    /// targets Codex; otherwise Codex's own default model runs the delegations.
    pub(super) fn start_voice(&mut self, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        let (Some(engine), Some(device)) = (state.engine().cloned(), state.local_device_id.clone())
        else {
            return;
        };
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
        let voice = settings::current(cx).codex_voice;
        self.voice.update(cx, |controller, cx| {
            controller.start(engine, device, config, voice, cx)
        });
    }

    pub(super) fn end_voice(&mut self, cx: &mut Context<Self>) {
        self.voice.update(cx, |voice, cx| voice.cancel(cx));
    }

    pub(super) fn set_voice_stage_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.voice
            .update(cx, |voice, cx| voice.set_stage_open(open, cx));
    }

    /// Track stage open/close flips for the transition clock and hand focus
    /// over: the stage owns the keyboard while it is up. Navigating to
    /// another thread or to Settings steps the stage aside; voice continues.
    pub(super) fn sync_voice_stage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.state.read(cx).selected_chat.clone();
        if self.voice.read(cx).stage_open
            && self.voice_stage_was_open
            && (matches!(self.route, Route::Settings(_)) || selected != self.voice_stage_selection)
        {
            self.set_voice_stage_open(false, cx);
        }
        let open = self.voice.read(cx).stage_open;
        if open == self.voice_stage_was_open {
            return;
        }
        self.voice_stage_was_open = open;
        self.voice_stage_selection = selected;
        self.voice_stage_changed_at = Some(std::time::Instant::now());
        let focus = if open {
            self.voice_stage_focus.clone()
        } else {
            self.composer.focus_handle(cx)
        };
        if window.is_window_active() {
            window.focus(&focus, cx);
        }
    }

    pub(super) fn render_voice_trigger(
        &mut self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let voice = self.voice.read(cx);
        let live = voice.is_live();
        let stage_open = voice.stage_open;
        let failure = (voice.phase == VoicePhase::Failed)
            .then(|| voice.reason_text())
            .filter(|text| !text.is_empty());
        let (orb_state, level) = (voice.orb_state(), voice.level());
        let reduced = self.reduced_motion;
        self.voice_footer_orb.update(cx, |orb, cx| {
            orb.set_visible(live, cx);
            orb.set_state(orb_state, cx);
            orb.set_speed(1.0 + level * 1.5, cx);
            orb.set_reduced_motion(reduced, cx);
        });

        let button = div()
            .id("voice-trigger")
            .debug_selector(|| "voice-trigger".into())
            .role(gpui::Role::Button)
            .tab_index(0)
            .relative()
            .size(px(SIDEBAR_FOOTER_BUTTON_SIZE))
            .flex_none()
            .rounded(px(8.0))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .bg(motion::hover_blend(
                FOOTER_HOVER,
                theme.glass_hover().opacity(0.0),
                theme.glass_hover(),
            ))
            .on_hover(motion::hover_listener(FOOTER_HOVER))
            .focus_visible(|s| s.border_2().border_color(theme.accent));

        if live {
            return button
                .aria_label(if stage_open {
                    "Hide voice"
                } else {
                    "Open voice"
                })
                .tooltip(stage_tooltip(if stage_open {
                    "Hide voice · Esc"
                } else {
                    "Voice is live · open"
                }))
                .on_click(
                    cx.listener(move |this, _, _, cx| this.set_voice_stage_open(!stage_open, cx)),
                )
                .child(self.voice_footer_orb.clone())
                .into_any_element();
        }

        let tooltip: SharedString = failure
            .map(SharedString::from)
            .unwrap_or_else(|| "Start voice · Codex orchestrator".into());
        let button = button
            .aria_label("Start voice")
            .tooltip(move |_, cx| {
                let text = tooltip.clone();
                cx.new(|_| SurfaceTabTooltip { text }).into()
            })
            .on_click(cx.listener(|this, _, _, cx| this.start_voice(cx)))
            .child(
                icon(icons::MICROPHONE)
                    .size(px(15.0))
                    .text_color(motion::hover_blend(
                        FOOTER_HOVER,
                        theme.text_muted,
                        theme.text,
                    )),
            );
        let Some(failure) = failure else {
            return button.into_any_element();
        };
        // A failed start explains itself once, anchored to the microphone;
        // the next press retries.
        let popup_theme = theme.for_popup();
        let card = popover::popover_card(&popup_theme)
            .w(px(232.0))
            .flex()
            .flex_row()
            .items_start()
            .gap(px(8.0))
            .child(
                icon(icons::DANGER_TRIANGLE)
                    .mt(px(1.0))
                    .size(px(14.0))
                    .text_color(popup_theme.warning),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(crate::typography::ui_rems(12.0))
                    .text_color(popup_theme.text_muted)
                    .child(SharedString::from(failure)),
            )
            .child(
                div()
                    .id("voice-failure-dismiss")
                    .flex_none()
                    .cursor_pointer()
                    .rounded(px(4.0))
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.voice.update(cx, |voice, cx| voice.dismiss_reason(cx));
                    }))
                    .child(
                        icon(icons::CLOSE)
                            .size(px(14.0))
                            .text_color(popup_theme.text_faint),
                    ),
            )
            .into_any_element();
        button
            .child(
                div()
                    .absolute()
                    .top(px(4.0))
                    .right(px(4.0))
                    .size(px(5.0))
                    .rounded_full()
                    .bg(theme.warning),
            )
            .child(popover::anchored_menu_above_end(
                "voice-failure",
                card,
                None,
            ))
            .into_any_element()
    }

    /// Full-window stage: the session's orb over the new-thread hero artwork,
    /// live caption, and the session controls. Escape returns to the chats
    /// without ending voice.
    pub(super) fn render_voice_stage(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let open = self.voice.read(cx).stage_open;
        let elapsed = self
            .voice_stage_changed_at
            .map_or(f32::INFINITY, |at| at.elapsed().as_secs_f32() * 1000.0);
        let reveal = stage_reveal(open, elapsed, self.reduced_motion);
        if reveal > 0.0 && reveal < 1.0 {
            self.motion_active.set(true);
        }
        let voice = self.voice.read(cx);
        let (orb_state, level) = (voice.orb_state(), voice.level());
        let status = voice.status_text();
        let caption = caption_tail(&voice.partial, STAGE_CAPTION_CHARS);
        let snapshot = voice.snapshot.clone();
        let chat_id = voice.chat_id.clone();
        let awaiting = snapshot
            .as_ref()
            .is_some_and(|s| s.work == VoiceWork::AwaitingInput);
        let reduced = self.reduced_motion;
        self.voice_stage_orb.update(cx, |orb, cx| {
            orb.set_visible(open, cx);
            orb.set_state(orb_state, cx);
            orb.set_speed(1.0 + level * 1.5, cx);
            orb.set_reduced_motion(reduced, cx);
            // The orb blooms out of the footer as the stage arrives.
            orb.set_scale(VOICE_STAGE_ORB_SCALE * (0.82 + 0.18 * reveal), cx);
        });
        if reveal <= 0.001 {
            return None;
        }

        let theme = Theme::of(cx).clone();
        let viewport = window.viewport_size();
        // The sidebar stays usable beside the stage; collapsed, the stage
        // spans the whole window.
        let left = self.sidebar_now();
        let width = (f32::from(viewport.width) - left).max(0.0);
        let ui_settings = settings::current(cx);
        let artwork = ui_settings
            .new_thread_composer_background
            .as_ref()
            .and_then(|background| {
                crate::new_thread_background_effects::prepare(
                    ui_settings.new_thread_background_effect,
                    &theme,
                    std::path::Path::new(&background.path),
                    cx,
                )
            });
        let adjustment = ui_settings
            .new_thread_composer_background
            .as_ref()
            .map(|background| background.adjustment)
            .unwrap_or_default();
        let hero = new_thread_background(
            artwork,
            adjustment,
            f32::from(viewport.height),
            width,
            self.voice_stage_bounds.clone(),
            0.0,
            new_thread_background_opacity(theme.is_frost()),
        );

        let orb_block = div()
            .relative()
            .size(px(STAGE_ORB_BOX))
            .flex()
            .items_center()
            .justify_center()
            .child(self.voice_stage_orb.clone());

        let stage_bounds = self.voice_stage_bounds.clone();
        let center = div()
            .relative()
            .top(px(18.0 * (1.0 - reveal)))
            .flex()
            .flex_col()
            .items_center()
            .child(
                // The hero's cutout follows this block like it follows the
                // new-thread composer, keeping the orb and caption legible.
                gpui::canvas(
                    move |bounds, _, _| stage_bounds.set(Some(bounds)),
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .child(orb_block)
            .child(
                div()
                    .mt(px(6.0))
                    .text_size(crate::typography::ui_rems(18.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(if awaiting { theme.warning } else { theme.text })
                    .child(SharedString::from(status)),
            )
            .child(
                div()
                    .mt(px(10.0))
                    .w(px(560.0))
                    .max_w(px((width - 48.0).max(200.0)))
                    .h(px(48.0))
                    .overflow_hidden()
                    .text_center()
                    .text_size(crate::typography::ui_rems(14.0))
                    .line_height(px(22.0))
                    .text_color(theme.text_muted)
                    .child(caption),
            );

        let controls = self.render_voice_call_bar(&theme, awaiting, snapshot, chat_id, cx);

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
                .opacity(reveal)
                .bg(theme.bg)
                .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                    let key = event.keystroke.key.as_str();
                    let plain = !event.keystroke.modifiers.modified();
                    if key == "escape" {
                        this.set_voice_stage_open(false, cx);
                        cx.stop_propagation();
                    } else if key == "m" && plain {
                        this.voice.update(cx, |voice, cx| voice.toggle_mute(cx));
                        cx.stop_propagation();
                    }
                }))
                .child(div().absolute().inset_0().child(hero))
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .pb(px(64.0))
                        .child(center),
                )
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom(px(32.0 - 16.0 * (1.0 - reveal)))
                        .flex()
                        .justify_center()
                        .child(controls),
                )
                .into_any_element(),
        )
    }

    /// Video-call style bar: session time, the call controls grouped around
    /// a red hang-up, and the way back to the chats.
    fn render_voice_call_bar(
        &mut self,
        theme: &Theme,
        awaiting: bool,
        snapshot: Option<zeron_proto::voice::VoiceSnapshot>,
        chat_id: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let popup = theme.for_popup();
        let voice = self.voice.read(cx);
        let muted = voice.muted();
        let mic_level = voice.microphone_level();
        let elapsed = voice
            .active_since
            .map(|since| crate::voice::format_elapsed(since.elapsed().as_secs()));
        let live_tone = if awaiting {
            popup.warning
        } else if muted {
            popup.text_faint
        } else {
            popup.success
        };

        // Session clock, like a meeting's running time.
        let clock = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .pl(px(10.0))
            .pr(px(6.0))
            .child(div().size(px(7.0)).rounded_full().bg(live_tone))
            .child(
                div()
                    .min_w(px(40.0))
                    .font_family(popup.font_mono.clone())
                    .text_size(crate::typography::ui_rems(13.0))
                    .text_color(popup.text_muted)
                    .child(SharedString::from(elapsed.unwrap_or_else(|| "–:––".into()))),
            );

        // Microphone: inverted plate while muted; while live, a ring that
        // swells with your voice.
        let mic = round_control("voice-bar-mic", &popup, muted)
            .aria_label(if muted {
                "Unmute microphone"
            } else {
                "Mute microphone"
            })
            .tooltip(stage_tooltip(if muted {
                "Unmute · M"
            } else {
                "Mute · M"
            }))
            .on_click(cx.listener(|this, _, _, cx| {
                this.voice.update(cx, |voice, cx| voice.toggle_mute(cx))
            }))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded_full()
                    .border_2()
                    .border_color(popup.success.opacity((mic_level * 6.0).min(0.9))),
            )
            .child(
                icon(if muted {
                    icons::MICROPHONE_OFF
                } else {
                    icons::MICROPHONE
                })
                .size(px(19.0))
                .text_color(if muted { popup.on_solid } else { popup.text }),
            );

        // Voices apply to the next session; the current one keeps its own.
        let voices = snapshot
            .as_ref()
            .map(|s| s.voices.clone())
            .unwrap_or_default();
        let selected = settings::current(cx).codex_voice;
        let voice_choice = (!voices.is_empty()).then(|| {
            let label = selected.clone().unwrap_or_else(|| "Default".into());
            round_control("voice-bar-voice", &popup, false)
                .w_auto()
                .px(px(14.0))
                .gap(px(7.0))
                .aria_label("Choose the voice for your next session")
                .tooltip(stage_tooltip("Voice for your next session"))
                .on_click(move |_, _, cx| {
                    let next = match selected
                        .as_ref()
                        .and_then(|v| voices.iter().position(|id| id == v))
                    {
                        Some(index) if index + 1 < voices.len() => Some(voices[index + 1].clone()),
                        Some(_) => None,
                        None => voices.first().cloned(),
                    };
                    settings::update(settings::SavePolicy::Immediate, cx, |s| {
                        s.codex_voice = next
                    });
                    cx.refresh_windows();
                })
                .child(
                    icon(icons::VOLUME_LOUD)
                        .size(px(17.0))
                        .text_color(popup.text_muted),
                )
                .child(
                    div()
                        .text_size(crate::typography::ui_rems(13.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(SharedString::from(label)),
                )
        });

        let transcript = chat_id.map(|chat_id| {
            round_control("voice-bar-transcript", &popup, false)
                .aria_label(if awaiting {
                    "Answer Codex in the transcript"
                } else {
                    "Open the voice transcript"
                })
                .tooltip(stage_tooltip(if awaiting {
                    "Codex is asking you something"
                } else {
                    "Transcript"
                }))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.set_voice_stage_open(false, cx);
                    this.open_chat(chat_id.clone(), cx);
                }))
                .child(
                    icon(icons::CHAT_ROUND_LINE)
                        .size(px(19.0))
                        .text_color(if awaiting { popup.warning } else { popup.text }),
                )
                .when(awaiting, |el| {
                    el.child(
                        div()
                            .absolute()
                            .top(px(9.0))
                            .right(px(9.0))
                            .size(px(8.0))
                            .rounded_full()
                            .border_2()
                            .border_color(popover::surface_bg(&popup))
                            .bg(popup.warning),
                    )
                })
        });

        // The hang-up: the one saturated control, wide enough to never be
        // mistaken for its neighbours.
        let end = div()
            .id("voice-bar-end")
            .role(gpui::Role::Button)
            .aria_label("End voice")
            .tab_index(0)
            .h(px(BAR_CONTROL))
            .px(px(20.0))
            .rounded_full()
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .bg(motion::hover_blend(
                "voice-bar-end",
                popup.danger,
                popup.danger.blend(gpui::black().opacity(0.14)),
            ))
            .on_hover(motion::hover_listener("voice-bar-end"))
            .focus_visible(|s| s.border_2().border_color(popup.accent))
            .tooltip(stage_tooltip("End voice"))
            .on_click(cx.listener(|this, _, _, cx| this.end_voice(cx)))
            .child(
                icon(icons::PHONE_HANG_UP)
                    .size(px(19.0))
                    .text_color(gpui::white()),
            )
            .child(
                div()
                    .text_size(crate::typography::ui_rems(13.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(gpui::white())
                    .child(SharedString::from("End")),
            );

        let back = round_control("voice-bar-back", &popup, false)
            .aria_label("Back to chats")
            .tooltip(stage_tooltip("Back to chats · Esc"))
            .on_click(cx.listener(|this, _, _, cx| this.set_voice_stage_open(false, cx)))
            .child(
                icon(icons::ALT_ARROW_DOWN)
                    .size(px(19.0))
                    .text_color(popup.text_muted),
            );

        let divider = || div().w(px(1.0)).h(px(24.0)).mx(px(4.0)).bg(popup.border);
        let radius = BAR_CONTROL / 2.0 + BAR_PAD;
        let bar = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .p(px(BAR_PAD))
            .rounded(px(radius))
            .border_1()
            .border_color(popup.border.opacity(0.7))
            .bg(popover::surface_bg(&popup))
            .when(!popup.is_frost(), |el| el.shadow_lg())
            .text_color(popup.text)
            .child(clock)
            .child(divider())
            .child(mic)
            .children(voice_choice)
            .children(transcript)
            .child(end)
            .child(divider())
            .child(back);
        crate::frost::frosted(radius, 18.0, bar).into_any_element()
    }
}

fn stage_tooltip(text: &'static str) -> impl Fn(&mut Window, &mut App) -> gpui::AnyView + 'static {
    move |_, cx| cx.new(|_| SurfaceTabTooltip { text: text.into() }).into()
}

/// A round call-bar control; `active` inverts it onto the solid plate.
fn round_control(id: &'static str, theme: &Theme, active: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .role(gpui::Role::Button)
        .tab_index(0)
        .relative()
        .size(px(BAR_CONTROL))
        .flex_none()
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .text_color(if active { theme.on_solid } else { theme.text })
        .bg(if active {
            theme.solid
        } else {
            motion::hover_blend(id, theme.glass_hover().opacity(0.5), theme.glass_hover())
        })
        .on_hover(motion::hover_listener(id))
        .focus_visible(|s| s.border_2().border_color(theme.accent))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caption_keeps_the_latest_words_of_a_long_utterance() {
        assert_eq!(caption_tail("  short  ", 10), "short");
        let tail = caption_tail("one two three four five", 9);
        assert_eq!(tail, "…four five");
        // Multibyte text is cut on character boundaries.
        assert_eq!(caption_tail("ñandú camión", 6), "…camión");
    }

    #[test]
    fn stage_reveal_eases_in_and_out_and_snaps_for_reduced_motion() {
        assert_eq!(stage_reveal(true, 0.0, false), 0.0);
        assert_eq!(stage_reveal(true, f32::INFINITY, false), 1.0);
        assert!(stage_reveal(true, STAGE_ENTER_MS / 4.0, false) > 0.5);
        assert_eq!(stage_reveal(false, 0.0, false), 1.0);
        assert_eq!(stage_reveal(false, f32::INFINITY, false), 0.0);
        assert_eq!(stage_reveal(true, 0.0, true), 1.0);
        assert_eq!(stage_reveal(false, 0.0, true), 0.0);
    }
}
