use crate::{
    popover,
    settings::{self, widgets},
    theme::Theme,
};
use gpui::{App, Context, Entity, Global, Task, Window, div, prelude::*, px};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
struct VoiceGlobal {
    card: Entity<VoiceCard>,
    directory: PathBuf,
}
impl Global for VoiceGlobal {}
pub(crate) fn init(root: PathBuf, cx: &mut App) {
    let directory = root.join("models/parakeet-tdt-0.6b-v3-int8");
    let card = cx.new(|_| VoiceCard {
        scroll: widgets::PageScroll::default(),
        ready: zeron_voice::installed(&directory),
        directory: directory.clone(),
        cancel: None,
        task: None,
        progress: 0,
        error: None,
    });
    cx.set_global(VoiceGlobal { card, directory });
}
pub(crate) fn directory(cx: &App) -> PathBuf {
    cx.global::<VoiceGlobal>().directory.clone()
}
pub(crate) fn enabled(cx: &App) -> bool {
    settings::current(cx).dictation_enabled
        && cx
            .try_global::<VoiceGlobal>()
            .is_some_and(|g| g.card.read(cx).ready)
}
pub(crate) fn card(cx: &mut App) -> Entity<VoiceCard> {
    if !cx.has_global::<VoiceGlobal>() {
        init(std::env::temp_dir().join("zeron-voice-test"), cx);
    }
    cx.global::<VoiceGlobal>().card.clone()
}
pub(crate) struct VoiceCard {
    scroll: widgets::PageScroll,
    directory: PathBuf,
    ready: bool,
    cancel: Option<Arc<AtomicBool>>,
    task: Option<Task<()>>,
    progress: u64,
    error: Option<String>,
}
impl VoiceCard {
    fn set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        settings::update(settings::SavePolicy::Immediate, cx, |s| {
            s.dictation_enabled = enabled
        });
        cx.refresh_windows();
        cx.notify();
    }
    fn primary(&mut self, cx: &mut Context<Self>) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Release);
        } else if self.ready {
            self.set_enabled(!settings::current(cx).dictation_enabled, cx);
        } else {
            self.download(cx);
        }
        cx.notify();
    }
    fn download(&mut self, cx: &mut Context<Self>) {
        if self.cancel.is_some() {
            return;
        }
        self.error = None;
        self.progress = 0;
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(cancel.clone());
        let progress = Arc::new(AtomicU64::new(0));
        let counter = progress.clone();
        let dir = self.directory.clone();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let r = zeron_voice::download(&dir, &cancel, |n| counter.store(n, Ordering::Relaxed))
                .map_err(|_| {
                if cancel.load(Ordering::Acquire) { "Download cancelled.".to_owned() }
                else { "Couldn’t download or verify the model. Check your connection and free storage, then retry.".to_owned() }
            });
            let _ = tx.send(r);
        });
        self.task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let result = rx.try_recv();
                let done = !matches!(result, Err(std::sync::mpsc::TryRecvError::Empty));
                if this
                    .update(cx, |this, cx| {
                        this.progress = progress.load(Ordering::Relaxed);
                        match result {
                            Ok(Ok(())) => {
                                this.ready = true;
                                let cancelled = this
                                    .cancel
                                    .as_ref()
                                    .is_some_and(|c| c.load(Ordering::Acquire));
                                this.cancel = None;
                                this.set_enabled(!cancelled, cx);
                            }
                            Ok(Err(e)) => {
                                this.cancel = None;
                                this.error = Some(e);
                            }
                            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                this.cancel = None;
                                this.error = Some("Download interrupted. Try again.".into());
                            }
                            _ => {}
                        }
                        cx.notify();
                    })
                    .is_err()
                    || done
                {
                    break;
                }
            }
        }));
        cx.notify();
    }
    fn remove(&mut self, cx: &mut Context<Self>) {
        if zeron_voice::busy() {
            self.error = Some("Stop dictation before removing the model.".into());
            cx.notify();
            return;
        }
        self.set_enabled(false, cx);
        zeron_voice::unload();
        match std::fs::remove_dir_all(&self.directory) {
            Ok(()) => {
                self.ready = false;
                self.error = None
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.ready = false;
                self.progress = 0;
                self.error = None;
            }
            Err(_) => {
                self.error =
                    Some("Could not remove the model. Check folder permissions and retry.".into())
            }
        }
        cx.notify();
    }
}
impl popover::ScrollRailHost for VoiceCard {
    fn rail_bar(&mut self) -> &mut popover::MenuScrollbarState {
        self.scroll.rail_bar()
    }
    fn rail_scroll(&self) -> Option<gpui::ScrollHandle> {
        self.scroll.rail_scroll()
    }
}
impl Render for VoiceCard {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let enabled = settings::current(cx).dictation_enabled;
        let downloading = self.cancel.is_some();
        let status = if downloading {
            format!(
                "Downloading · {:.0} of {:.0} MB",
                self.progress as f64 / 1e6,
                zeron_voice::download_size() as f64 / 1e6
            )
        } else if self.ready {
            if enabled {
                "Ready on this device".into()
            } else {
                "Model downloaded · Dictation off".into()
            }
        } else {
            format!(
                "Optional download · {:.0} MB",
                zeron_voice::download_size() as f64 / 1e6
            )
        };
        let label = if downloading {
            "Cancel download"
        } else if self.ready {
            if enabled {
                "Turn off"
            } else {
                "Enable dictation"
            }
        } else if self.error.is_some() {
            "Retry download"
        } else {
            "Download & enable"
        };
        let weak = cx.entity().downgrade();
        let remove_weak = weak.clone();
        let button = widgets::action_button(&theme, widgets::ActionTone::Outlined)
            .id("voice-enable")
            .role(gpui::Role::Button)
            .aria_label(label)
            .tab_index(0)
            .focus_visible(|s| s.border_2().border_color(theme.accent))
            .child(label)
            .on_click(cx.listener(|this, _, _, cx| this.primary(cx)))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    cx.stop_propagation();
                    this.primary(cx);
                }
            }))
            .on_a11y_action(gpui::AccessibleAction::Click, move |_, _, cx| {
                weak.update(cx, |this, cx| this.primary(cx)).ok();
            });
        let actions = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0))
            .child(button)
            .when(!downloading && (self.ready || self.progress > 0), |d| {
                d.child(
                    widgets::action_button(&theme, widgets::ActionTone::Quiet)
                        .id("voice-remove")
                        .role(gpui::Role::Button)
                        .aria_label("Remove model")
                        .tab_index(0)
                        .child("Remove model")
                        .focus_visible(|s| s.border_2().border_color(theme.accent))
                        .on_click(cx.listener(|this, _, _, cx| this.remove(cx)))
                        .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                cx.stop_propagation();
                                this.remove(cx);
                            }
                        }))
                        .on_a11y_action(gpui::AccessibleAction::Click, move |_, _, cx| {
                            remove_weak.update(cx, |this, cx| this.remove(cx)).ok();
                        }),
                )
            });
        let progress = div()
            .w_full()
            .h(px(3.0))
            .rounded_full()
            .bg(theme.border)
            .child(
                div()
                    .h_full()
                    .w(gpui::relative(
                        (self.progress as f32 / zeron_voice::download_size() as f32)
                            .clamp(0.0, 1.0),
                    ))
                    .bg(theme.accent),
            );
        let model_row = widgets::card_row(&theme, true)
            .child(
                div()
                    .flex_1()
                    .min_w(px(180.0))
                    .child(widgets::row_title(&theme, "Dictation"))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.text_muted)
                            .child("Type with your voice on this device."),
                    ),
            )
            .child(actions);
        let model_details = widgets::card_row(&theme, false)
            .flex_col()
            .items_start()
            .gap(px(8.0))
            .child(widgets::row_title(&theme, "NVIDIA Parakeet TDT 0.6B v3"))
            .child(
                div()
                    .id("voice-status")
                    .role(gpui::Role::Status)
                    .aria_label(status.clone())
                    .text_size(px(12.0))
                    .text_color(theme.text_muted)
                    .child(status),
            )
            .when(downloading, |d| d.child(progress))
            .children(self.error.as_ref().map(|e| {
                div()
                    .id("voice-error")
                    .role(gpui::Role::Status)
                    .aria_label(e.clone())
                    .text_size(px(12.0))
                    .text_color(theme.text_muted)
                    .child(e.clone())
            }));
        let details = widgets::section_card(&theme)
            .child(widgets::card_row(&theme, true).flex_col().items_start().gap(px(4.0))
                .child(widgets::row_title(&theme, "On-device processing"))
                .child(div().text_size(px(12.0)).text_color(theme.text_muted)
                    .child("Audio stays on this computer and is discarded after transcription. Your transcript stays in the draft until you send it.")))
            .child(widgets::card_row(&theme, false).flex_col().items_start().gap(px(4.0))
                .child(widgets::row_title(&theme, "Languages and recording"))
                .child(div().text_size(px(12.0)).text_color(theme.text_muted)
                    .child("25 languages, including English and French, detected automatically. Record up to one minute, then stop to transcribe.")));
        let scrollbar = popover::rail(self, "voice-page-scrollbar", &theme, cx);
        div()
            .id("voice-page-host")
            .relative()
            .size_full()
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if this.scroll.set_list_hovered(*hovered) {
                    cx.notify();
                }
            }))
            .child(
                crate::edge_fade::edge_faded(
                    16.0,
                    true,
                    true,
                    div()
                        .id("voice-page")
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(&self.scroll.scroll)
                        .child(
                            widgets::page_column()
                                .child(widgets::page_header(&theme, "Voice", None))
                                .child(
                                    widgets::section_card(&theme)
                                        .child(model_row)
                                        .child(model_details),
                                )
                                .child(details),
                        ),
                )
                .fade_overflow_y(&self.scroll.scroll),
            )
            .children(scrollbar)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[gpui::test]
    fn dictation_is_opt_in_and_disabling_preserves_download(cx: &mut gpui::TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        cx.update(|cx| settings::init(settings::UiSettings::default(), dir.path(), cx));
        cx.update(|cx| assert!(!enabled(cx)));
        let card = cx.update(card);
        card.update(cx, |card, cx| {
            card.ready = true;
            assert!(!settings::current(cx).dictation_enabled);
            card.primary(cx);
        });
        cx.update(|cx| assert!(enabled(cx)));
        card.update(cx, |card, cx| {
            card.primary(cx);
            assert!(card.ready);
        });
        cx.update(|cx| assert!(!enabled(cx)));
    }
}
