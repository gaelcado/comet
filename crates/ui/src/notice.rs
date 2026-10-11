//! The composer's notice chip — its tinted failure card (zeron composer.tsx
//! `Notice`).

use gpui::{Div, FontWeight, SharedString, div, prelude::*, px};

use crate::theme::Theme;

/// The composer's failure notice (zeron composer.tsx `Notice`): a tinted
/// rounded chip — `border <accent>/[0.16]` over a `<accent>/[0.05]` wash,
/// never a bare stroke — with a header row (DangerTriangle + medium label, a
/// tiny copy button pinned to its top-right corner — failure payloads are
/// meant to be pasted, not screenshotted) and the message below. The message WRAPS instead of truncating: failure payloads carry
/// exit statuses and stderr, and a one-line ellipsis was exactly what made
/// zeronsh/comet#95 undiagnosable from the screenshot. `warning` picks the
/// amber palette (amber-400/amber-200) over the default red
/// (red-400/red-300). The caller chains its own id and dismiss click. The
/// bare 14px triangle rem-scales with the composer's text: 12px inset and
/// radius, 12px text on a 16px line height.
pub fn notice_chip(
    theme: &Theme,
    warning: bool,
    label: &'static str,
    message: impl Into<SharedString>,
) -> Div {
    let (accent, muted) = if warning {
        (theme.warning, theme.warning_muted)
    } else {
        (theme.danger, theme.danger_muted)
    };
    let message = message.into();
    let copy_message = message.clone();
    let chip = div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(accent.opacity(0.16))
        .bg(accent.opacity(0.05))
        .px(px(12.0))
        .py(px(8.0))
        .text_size(crate::typography::ui_rems(12.0))
        .line_height(px(16.0))
        .text_color(muted.opacity(0.9));
    chip.child(
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                crate::icons::icon(crate::icons::DANGER_TRIANGLE)
                    .size(px(14.0))
                    .text_color(muted.opacity(0.9)),
            )
            .child(div().font_weight(FontWeight::MEDIUM).child(label))
            .child(div().flex_1())
            .child(
                div()
                    .id("notice-copy")
                    .flex_none()
                    .size(px(20.0))
                    .rounded(px(4.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(move |s| s.bg(accent.opacity(0.12)))
                    // The composer chip dismisses on click; copying must
                    // not take the notice with it.
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                            copy_message.to_string(),
                        ));
                    })
                    .tooltip(crate::settings::widgets::text_tooltip("Copy message"))
                    .child(
                        crate::icons::icon(crate::icons::COPY)
                            .size(px(12.0))
                            .text_color(muted.opacity(0.8)),
                    ),
            ),
    )
    .child(div().min_w_0().w_full().child(message))
}
