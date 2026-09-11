//! Settings → Notifications: independently configurable session chimes plus
//! desktop banners on the same status transitions (`shell::on_state_changed`).
//!
//! The ShortcutsPage arrangement: the page holds a working copy, every flip
//! emits [`NotificationsEvent::Changed`], and the shell persists it. Nothing
//! here talks RPC — both flags are device-local UI settings.

use gpui::{Context, EventEmitter, SharedString, Window, div, prelude::*, px};

use crate::icons;
use crate::settings::widgets;
use crate::theme::Theme;

#[derive(Debug, Clone)]
pub enum NotificationsEvent {
    /// A toggle flipped — persist the complete notification preference set.
    Changed {
        sound: bool,
        completion_sound: bool,
        input_sound: bool,
        attention_sound: bool,
        desktop: bool,
        background_only: bool,
    },
}

pub struct NotificationsPage {
    sound: bool,
    completion_sound: bool,
    input_sound: bool,
    attention_sound: bool,
    desktop: bool,
    background_only: bool,
}

impl EventEmitter<NotificationsEvent> for NotificationsPage {}

impl NotificationsPage {
    pub fn new(
        sound: bool,
        completion_sound: bool,
        input_sound: bool,
        attention_sound: bool,
        desktop: bool,
        background_only: bool,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self {
            sound,
            completion_sound,
            input_sound,
            attention_sound,
            desktop,
            background_only,
        }
    }

    fn emit(&self, cx: &mut Context<Self>) {
        cx.emit(NotificationsEvent::Changed {
            sound: self.sound,
            completion_sound: self.completion_sound,
            input_sound: self.input_sound,
            attention_sound: self.attention_sound,
            desktop: self.desktop,
            background_only: self.background_only,
        });
    }
}

impl Render for NotificationsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let accent = theme.accent;
        let sound = self.sound;
        let completion_sound = self.completion_sound;
        let input_sound = self.input_sound;
        let attention_sound = self.attention_sound;
        let desktop = self.desktop;
        let background_only = self.background_only;
        let toggle = |id: &'static str, label: &'static str, enabled: bool| {
            widgets::toggle_switch(&theme, enabled)
                .id(id)
                .role(gpui::Role::Switch)
                .aria_label(label)
                .aria_toggled(if enabled {
                    gpui::Toggled::True
                } else {
                    gpui::Toggled::False
                })
        };
        let card = widgets::section_card(&theme)
            .child(
                widgets::card_row(&theme, true)
                    .child(widgets::row_tile(&theme, icons::VOLUME_LOUD))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(widgets::row_title(&theme, "Session sounds"))
                            .child(widgets::meta_line(
                                &theme,
                                vec![
                                    div()
                                        .child(SharedString::from(
                                            "Allow sounds for the selected session events below.",
                                        ))
                                        .into_any_element(),
                                ],
                            )),
                    )
                    .child(
                        toggle("notifications-sound-toggle", "Session sounds", sound)
                            .tab_index(0)
                            .focus_visible(move |style| {
                                style.border_2().border_color(accent)
                            })
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.sound = !this.sound;
                                this.emit(cx);
                                cx.notify();
                            })),
                    ),
            )
            .child(
                widgets::card_row(&theme, false)
                    .when(!sound, |el| el.opacity(0.55))
                    .child(widgets::row_tile(&theme, icons::CHECK))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(widgets::row_title(&theme, "Task completed"))
                            .child(widgets::meta_line(
                                &theme,
                                vec![div()
                                    .child(SharedString::from(
                                        "Play a sound when an agent finishes a run.",
                                    ))
                                    .into_any_element()],
                            )),
                    )
                    .child(
                        toggle(
                            "notifications-completion-sound-toggle",
                            "Task completed sound",
                            completion_sound,
                        )
                        .when(sound, |el| {
                            el.tab_index(0)
                                .focus_visible(move |style| {
                                    style.border_2().border_color(accent)
                                })
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.completion_sound = !this.completion_sound;
                                    this.emit(cx);
                                    cx.notify();
                                }))
                        }),
                    ),
            )
            .child(
                widgets::card_row(&theme, false)
                    .when(!sound, |el| el.opacity(0.55))
                    .child(widgets::row_tile(&theme, icons::CHAT_ROUND_LINE))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(widgets::row_title(&theme, "Input required"))
                            .child(widgets::meta_line(
                                &theme,
                                vec![div()
                                    .child(SharedString::from(
                                        "Play a sound when an agent needs your response.",
                                    ))
                                    .into_any_element()],
                            )),
                    )
                    .child(
                        toggle(
                            "notifications-input-sound-toggle",
                            "Input required sound",
                            input_sound,
                        )
                        .when(sound, |el| {
                            el.tab_index(0)
                                .focus_visible(move |style| {
                                    style.border_2().border_color(accent)
                                })
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.input_sound = !this.input_sound;
                                    this.emit(cx);
                                    cx.notify();
                                }))
                        }),
                    ),
            )
            .child(
                widgets::card_row(&theme, false)
                    .when(!sound, |el| el.opacity(0.55))
                    .child(widgets::row_tile(&theme, icons::DANGER_TRIANGLE))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(widgets::row_title(&theme, "Errors and disconnections"))
                            .child(widgets::meta_line(
                                &theme,
                                vec![div()
                                    .child(SharedString::from(
                                        "Play a sound when a run fails or the connection remains unavailable.",
                                    ))
                                    .into_any_element()],
                            )),
                    )
                    .child(
                        toggle(
                            "notifications-attention-sound-toggle",
                            "Errors and disconnections sound",
                            attention_sound,
                        )
                        .when(sound, |el| {
                            el.tab_index(0)
                                .focus_visible(move |style| {
                                    style.border_2().border_color(accent)
                                })
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.attention_sound = !this.attention_sound;
                                    this.emit(cx);
                                    cx.notify();
                                }))
                        }),
                    ),
            )
            .child(
                widgets::card_row(&theme, false)
                    .child(widgets::row_tile(&theme, icons::BELL))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(widgets::row_title(&theme, "Desktop notifications"))
                            .child(widgets::meta_line(
                                &theme,
                                vec![
                                    div()
                                        .child(SharedString::from(
                                            "Show a system banner on the same events, so pings \
                                             reach you while Zeron is in the background.",
                                        ))
                                        .into_any_element(),
                                ],
                            )),
                    )
                    .child(
                        toggle(
                            "notifications-desktop-toggle",
                            "Desktop notifications",
                            desktop,
                        )
                            .tab_index(0)
                            .focus_visible(move |style| {
                                style.border_2().border_color(accent)
                            })
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.desktop = !this.desktop;
                                this.emit(cx);
                                cx.notify();
                            })),
                    ),
            )
            .child(
                // Sub-option of the banner row: dimmed + inert while banners
                // are off (the harnesses not-installed treatment).
                widgets::card_row(&theme, false)
                    .when(!desktop, |el| el.opacity(0.55))
                    .child(widgets::row_tile(&theme, icons::MONITOR))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(widgets::row_title(&theme, "Only when in the background"))
                            .child(widgets::meta_line(
                                &theme,
                                vec![
                                    div()
                                        .child(SharedString::from(
                                            "Skip the banner while a Zeron window is focused.",
                                        ))
                                        .into_any_element(),
                                ],
                            )),
                    )
                    .child(
                        toggle(
                            "notifications-background-toggle",
                            "Only notify when Zeron is in the background",
                            background_only,
                        )
                            .when(desktop, |el| {
                                el.tab_index(0)
                                    .focus_visible(move |style| {
                                        style.border_2().border_color(accent)
                                    })
                                    .cursor_pointer().on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.background_only = !this.background_only;
                                        this.emit(cx);
                                        cx.notify();
                                    },
                                ))
                            }),
                    ),
            );

        div()
            .id("notifications-page")
            .size_full()
            .overflow_y_scroll()
            .child(
                widgets::page_column()
                    .child(widgets::page_header(&theme, "Notifications", None))
                    .child(
                        widgets::page_subtitle(
                            &theme,
                            "Choose which session events can play a sound, and when desktop \
                             notifications appear.",
                        )
                        .max_w(px(512.0))
                        .line_height(px(20.0)),
                    )
                    .child(card),
            )
    }
}
