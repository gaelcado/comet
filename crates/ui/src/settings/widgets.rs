//! Shared scaffolding for the settings pages — the original's page rhythm
//! (`mx-auto max-w-3xl px-6 pb-16 pt-8`), section cards, row layout, badges
//! and small buttons, so every page reads as the same product surface
//! (zeron settings.devices.tsx / settings.agents.tsx / settings.archived.tsx).

use gpui::{AnyElement, Context, Pixels, ScrollHandle, SharedString, div, prelude::*, px};

use crate::popover::{self, MenuScrollbarMetrics, MenuScrollbarState, ScrollRailHost};
use crate::theme::Theme;

/// Shared by the modal and its dropdowns, so containment follows resizing.
pub fn modal_bounds(viewport: gpui::Size<Pixels>) -> gpui::Bounds<Pixels> {
    let width = f32::from(viewport.width);
    let height = f32::from(viewport.height);
    let margin = if width < 680.0 { 16.0 } else { 48.0 };
    let size = gpui::size(
        px((width - margin).max(0.0).min(1024.0)),
        px((height - 80.0).max(0.0).min(720.0)),
    );
    gpui::Bounds::new(
        gpui::point(
            (viewport.width - size.width) / 2.0,
            (viewport.height - size.height) / 2.0,
        ),
        size,
    )
}

pub fn dropdown(
    id: impl Into<SharedString>,
    content: gpui::Div,
    closing: Option<std::time::Instant>,
    trigger_height: f32,
) -> AnyElement {
    SettingsDropdown {
        id: id.into(),
        content,
        closing,
        trigger_height,
    }
    .into_any_element()
}

#[derive(IntoElement)]
struct SettingsDropdown {
    id: SharedString,
    content: gpui::Div,
    closing: Option<std::time::Instant>,
    trigger_height: f32,
}

impl RenderOnce for SettingsDropdown {
    fn render(self, window: &mut gpui::Window, _: &mut gpui::App) -> impl IntoElement {
        let limits = dropdown_limits(window.viewport_size());
        popover::contained_menu(
            self.id,
            self.content,
            self.closing,
            self.trigger_height,
            limits,
        )
    }
}

fn dropdown_limits(viewport: gpui::Size<Pixels>) -> gpui::Bounds<Pixels> {
    let bounds = modal_bounds(viewport);
    let sidebar = if f32::from(viewport.width) < 680.0 {
        72.0
    } else {
        216.0
    };
    gpui::Bounds::new(
        bounds.origin + gpui::point(px(sidebar + 8.0), px(8.0)),
        gpui::size(
            (bounds.size.width - px(sidebar + 16.0)).max(px(1.0)),
            (bounds.size.height - px(16.0)).max(px(1.0)),
        ),
    )
}

/// Scroll only a dropdown's options, keeping its frame and optional heading
/// fixed. The available height follows the same placement budget as the card.
pub fn dropdown_rows(
    id: impl Into<SharedString>,
    rows: impl IntoIterator<Item = AnyElement>,
    trigger_height: f32,
    chrome_height: f32,
) -> AnyElement {
    DropdownRows {
        id: id.into(),
        rows: rows.into_iter().collect(),
        trigger_height,
        chrome_height,
    }
    .into_any_element()
}

pub fn dropdown_list_height(
    viewport: gpui::Size<Pixels>,
    trigger_height: f32,
    chrome_height: f32,
) -> f32 {
    let limits = modal_bounds(viewport);
    let card_height = ((f32::from(limits.size.height) - 16.0 - trigger_height) / 2.0 - 6.0)
        .max(1.0)
        .min(320.0);
    (card_height - chrome_height).max(1.0)
}

#[derive(IntoElement)]
struct DropdownRows {
    id: SharedString,
    rows: Vec<AnyElement>,
    trigger_height: f32,
    chrome_height: f32,
}

impl RenderOnce for DropdownRows {
    fn render(self, window: &mut gpui::Window, _: &mut gpui::App) -> impl IntoElement {
        let key: SharedString = format!("{}-list-scroll", self.id).into();
        let scroll = window.with_global_id(key.into(), |id, window| {
            window.with_element_state(id, |previous: Option<ScrollHandle>, _| {
                let scroll = previous.unwrap_or_default();
                (scroll.clone(), scroll)
            })
        });
        let list = div()
            .id(format!("{}-list", self.id))
            .debug_selector(|| "settings-dropdown-list".into())
            .max_h(px(dropdown_list_height(
                window.viewport_size(),
                self.trigger_height,
                self.chrome_height,
            )))
            .overflow_y_scroll()
            .track_scroll(&scroll)
            .flex()
            .flex_col()
            .gap(px(2.0))
            .children(self.rows);
        crate::edge_fade::edge_faded(12.0, true, true, list).fade_overflow_y(&scroll)
    }
}

/// Owned scroll + floating-scrollbar state for one settings page.
///
/// This is the dedicated settings scroll container state. It wraps the same
/// `MenuScrollbarState` treatment as the model-picker (`pickers.rs`) and the
/// composer popups (`composer.rs`): the rail is hidden until hover/drag and
/// floats above content without consuming layout width.
///
/// Every settings page follows the same shape: a `scroll: PageScroll` field,
/// a [`ScrollRailHost`] impl on the page delegating here, and a root
/// `.relative()` host carrying only the list-hover `on_hover` around the
/// `.overflow_y_scroll().track_scroll(&self.scroll.scroll)` page, with
/// [`popover::rail`] supplying the rail and all of its listeners.
pub struct PageScroll {
    /// Tracked by the page's scrolling list directly — including the appshots
    /// half of the shortcuts page, which renders from its own file.
    pub scroll: ScrollHandle,
    bar: MenuScrollbarState,
}

impl Default for PageScroll {
    fn default() -> Self {
        Self {
            scroll: ScrollHandle::new(),
            bar: MenuScrollbarState::default(),
        }
    }
}

impl ScrollRailHost for PageScroll {
    fn rail_bar(&mut self) -> &mut MenuScrollbarState {
        &mut self.bar
    }

    fn rail_scroll(&self) -> Option<ScrollHandle> {
        Some(self.scroll.clone())
    }
}

impl PageScroll {
    pub fn set_list_hovered(&mut self, hovered: bool) -> bool {
        self.bar.set_list_hovered(hovered)
    }

    fn set_bar_hovered(&mut self, hovered: bool) -> bool {
        self.bar.set_bar_hovered(hovered)
    }

    fn begin_press(&mut self, pointer_y: Pixels) -> bool {
        self.bar.begin_press(&self.scroll, pointer_y)
    }

    fn drag_to(&self, pointer_y: Pixels) -> bool {
        self.bar.drag_to(&self.scroll, pointer_y)
    }

    fn end_press(&mut self) -> bool {
        self.bar.end_press()
    }

    /// Rewind to the top and drop the rail's activity baseline — what a page
    /// flip must do when one [`PageScroll`] is rerouted at a different list
    /// (shortcuts ↔ appshots), so the new page opens unscrolled and the
    /// offset jump is not read back as scrolling.
    pub fn reset(&mut self) {
        popover::reset_menu_scroll(&self.scroll, &mut self.bar);
    }

    /// Records scroll activity, then computes the rail geometry. Call once
    /// per render ([`rail`] pairs this with the hide-countdown scheduling).
    fn metrics(&mut self) -> Option<MenuScrollbarMetrics> {
        self.bar.note_scroll(&self.scroll);
        self.bar.metrics(&self.scroll)
    }
}

/// [`popover::rail`] for a [`PageScroll`] that is not its view's only rail:
/// `popover::rail` binds to the view's single [`ScrollRailHost`] impl, and a
/// page carrying a second, menu-local scroll host (the appearance page's
/// interface-font dropdown) cannot route that impl at both. The wiring
/// mirrors [`popover::rail`]; `reach` re-borrows the state from the view
/// inside the strip's pointer listeners, which fire with a `&mut V`.
pub fn rail<V: 'static>(
    scroll: &mut PageScroll,
    id: &'static str,
    theme: &Theme,
    cx: &mut Context<V>,
    reach: impl Fn(&mut V) -> &mut PageScroll + Copy + 'static,
) -> Option<AnyElement> {
    let metrics = scroll.metrics()?;
    popover::schedule_scrollbar_hide(&mut scroll.bar, cx);
    let strip = scroll.bar.render_rail(theme, metrics)?;
    Some(
        strip
            .id(id)
            .on_hover(cx.listener(move |view, hovered: &bool, _, cx| {
                if reach(view).set_bar_hovered(*hovered) {
                    cx.notify();
                }
            }))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |view, event: &gpui::MouseDownEvent, _, cx| {
                    if reach(view).begin_press(event.position.y) {
                        cx.stop_propagation();
                        cx.notify();
                    }
                }),
            )
            .on_drag(popover::MenuScrollbarDrag, |_, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| popover::MenuScrollbarDragGhost)
            })
            .on_drag_move(cx.listener(
                move |view, event: &gpui::DragMoveEvent<popover::MenuScrollbarDrag>, _, cx| {
                    if reach(view).drag_to(event.event.position.y) {
                        cx.notify();
                    }
                },
            ))
            .on_mouse_up_out(
                gpui::MouseButton::Left,
                cx.listener(move |view, _: &gpui::MouseUpEvent, _, cx| {
                    reach(view).end_press();
                    cx.notify();
                }),
            )
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(move |view, _: &gpui::MouseUpEvent, _, cx| {
                    reach(view).end_press();
                    cx.notify();
                }),
            )
            .into_any_element(),
    )
}

/// Shared typography for a settings component's title and description. The
/// Shortcuts page established this compact rhythm; list-style settings reuse
/// it instead of drifting by page.
pub const ROW_TITLE_SIZE: f32 = 13.0;
pub const ROW_DESCRIPTION_SIZE: f32 = 12.0;

/// Centered page column: `mx-auto w-full max-w-3xl px-6 pb-16 pt-8`.
pub fn page_column() -> gpui::Div {
    div()
        .w_full()
        .max_w(px(720.0))
        .mx_auto()
        .px(px(24.0))
        .pt(px(32.0))
        .pb(px(32.0))
        .flex()
        .flex_col()
}

/// Page headline row: `flex items-baseline gap-2.5` — `text-base font-semibold`
/// title + `text-[13px]` count sharing a baseline (zeron settings.devices.tsx).
pub fn page_header(theme: &Theme, title: &str, count: Option<usize>) -> gpui::Div {
    div()
        .flex()
        .flex_row()
        .items_baseline()
        .gap(px(10.0))
        .child(
            div()
                .text_size(crate::typography::ui_rems(16.0))
                .line_height(crate::typography::ui_rems(20.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.text)
                .child(SharedString::from(title.to_string())),
        )
        .when_some(count, |el, count| {
            el.child(
                div()
                    .text_size(crate::typography::ui_rems(13.0))
                    .text_color(theme.text_muted)
                    .child(SharedString::from(format!("{count}"))),
            )
        })
}

/// Subtitle under the headline: `mt-1 text-[13px] text-muted-foreground`.
pub fn page_subtitle(theme: &Theme, copy: impl Into<SharedString>) -> gpui::Div {
    div()
        .mt(px(2.0))
        .min_w_0()
        .text_size(crate::typography::ui_rems(13.0))
        .line_height(crate::typography::ui_rems(17.0))
        .text_color(theme.text_muted)
        .child(copy.into())
}

/// Small label above a group of controls (`text-[13px] font-medium`) — the
/// "Theme" caption over a picker, not a page headline.
pub fn field_label(theme: &Theme, label: impl Into<SharedString>) -> gpui::Div {
    div()
        .text_size(crate::typography::ui_rems(13.0))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(theme.text)
        .child(label.into())
}

/// A row of equally-sized preview cards for picking one of N *visual* options.
///
/// Deliberately knows nothing about themes: the caller supplies each preview as
/// an arbitrary element and picks however many cards it wants, so the same
/// control works for a density picker, a layout picker or anything else where
/// the choice is easier to show than to describe. Pair with [`option_card`].
pub fn option_card_row() -> gpui::Div {
    div().flex().flex_row().items_start().gap(px(16.0)).w_full()
}

/// Default height of an [`option_card`] preview frame.
pub const OPTION_CARD_HEIGHT: f32 = 148.0;
/// Corner radius of the preview frame.
///
/// Public because the preview has to round *itself* to this. gpui content masks
/// are axis-aligned rectangles, so `overflow_hidden` on the frame clips to its
/// bounding box and not to its corner radius — a preview that paints its own
/// background will square off the corners and cover the frame's border with it.
pub const OPTION_CARD_RADIUS: f32 = 6.0;

/// One card in an [`option_card_row`]: a fixed-height preview frame with a quiet
/// selected edge and caption underneath. There is deliberately no outer card
/// or ring; the preview itself is the control.
///
/// `preview` fills the frame and **must round its own corners** to
/// [`OPTION_CARD_RADIUS`] if it paints a background — see that constant.
///
/// Returns a plain `Div` like the rest of this module — the caller adds `.id(..)`
/// and `.on_click(..)`, so selection behaviour stays with the page that owns the
/// state.
pub fn option_card(
    theme: &Theme,
    icon_path: &'static str,
    label: impl Into<SharedString>,
    selected: bool,
    preview: AnyElement,
) -> gpui::Div {
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(8.0))
        .cursor_pointer()
        .child(
            div()
                .h(px(OPTION_CARD_HEIGHT))
                .flex_none()
                .w_full()
                .rounded(px(OPTION_CARD_RADIUS))
                .overflow_hidden()
                .border_1()
                .border_color(if selected { theme.accent } else { theme.border })
                .child(preview),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_size(crate::typography::ui_rems(13.0))
                .font_weight(if selected {
                    gpui::FontWeight::MEDIUM
                } else {
                    gpui::FontWeight::NORMAL
                })
                .text_color(if selected {
                    theme.accent
                } else {
                    theme.text_muted
                })
                .child(
                    crate::icons::icon(icon_path)
                        .size(px(16.0))
                        .text_color(if selected {
                            theme.accent
                        } else {
                            theme.text_muted
                        }),
                )
                .child(label.into()),
        )
}

/// A quiet outline groups related settings without covering the shared material.
pub fn section_card(theme: &Theme) -> gpui::Div {
    div()
        .mt(px(24.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(theme.border.opacity(0.7))
        .overflow_hidden()
        .flex()
        .flex_col()
}

pub fn card_row(theme: &Theme, first: bool) -> gpui::Div {
    div()
        .px(px(16.0))
        .py(px(10.0))
        .min_h(px(56.0))
        .when(!first, |el| {
            el.border_t_1().border_color(theme.border.opacity(0.55))
        })
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(12.0))
}

/// Bare leading glyphs use the same weight and tint as Zeron's picker rows.
pub fn row_tile(theme: &Theme, icon_path: &'static str) -> gpui::Div {
    div()
        .flex_none()
        .w(px(24.0))
        .h(px(32.0))
        .flex()
        .items_center()
        .justify_center()
        .child(
            crate::icons::icon(icon_path)
                .size(px(16.0))
                .text_color(theme.text_muted),
        )
}

/// Row title. These metrics intentionally match the Shortcuts rows, whose
/// title/description rhythm is the reference for the other settings cards.
pub fn row_title(theme: &Theme, title: impl Into<SharedString>) -> gpui::Div {
    div()
        .min_w_0()
        .text_size(crate::typography::ui_rems(ROW_TITLE_SIZE))
        .line_height(crate::typography::ui_rems(17.0))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(theme.text)
        .child(title.into())
}

/// The quiet meta line under a row title: `text-[12px]
/// text-muted-foreground/65` fragments joined by dots.
pub fn meta_line(theme: &Theme, fragments: Vec<AnyElement>) -> gpui::Div {
    let mut line = div()
        .mt(px(Theme::TEXT_STACK_GAP))
        .min_w_0()
        .max_w_full()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap_x(px(8.0))
        .gap_y(px(2.0))
        .text_size(crate::typography::ui_rems(ROW_DESCRIPTION_SIZE))
        .line_height(crate::typography::ui_rems(16.0))
        .text_color(theme.text_muted);
    let mut first = true;
    for fragment in fragments {
        if !first {
            line = line.child(
                div()
                    .text_color(theme.text_muted.opacity(0.3))
                    .child(SharedString::from("·")),
            );
        }
        line = line.child(div().min_w_0().max_w_full().child(fragment));
        first = false;
    }
    line
}

/// Right-anchored badge pill: `rounded-full border px-2 py-0.5 text-[10.5px]`.
pub fn badge(theme: &Theme, label: impl Into<SharedString>) -> gpui::Div {
    div()
        .flex_none()
        .px(px(8.0))
        .py(px(2.0))
        .rounded_full()
        .border_1()
        .border_color(theme.border)
        .text_size(crate::typography::ui_rems(10.5))
        .text_color(theme.text_muted)
        .child(label.into())
}

/// Emerald status pill (the Accounts "Active" badge:
/// `bg-emerald-400/[0.12] text-emerald-300/90`).
pub fn badge_active(theme: &Theme, label: impl Into<SharedString>) -> gpui::Div {
    let emerald = theme.success;
    let emerald_text = theme.success_muted; // emerald-300
    div()
        .flex_none()
        .px(px(8.0))
        .py(px(2.0))
        .rounded_full()
        .bg(emerald.opacity(0.12))
        .text_size(crate::typography::ui_rems(10.5))
        .text_color(emerald_text.opacity(0.9))
        .child(label.into())
}

pub const SWITCH_WIDTH: f32 = 56.0;
const SWITCH_THUMB_WIDTH: f32 = 31.0;
const SWITCH_SIDE_INSET: f32 = 1.0;
const SWITCH_MARK_SIZE: f32 = 9.0;

/// A pill switch with the on/off marks nested beneath a sliding thumb.
/// The caller owns activation and accessibility; only the thumb interpolates.
pub fn toggle_switch(theme: &Theme, on: bool, key: impl Into<SharedString>) -> gpui::Div {
    let key: SharedString = key.into();
    div()
        .flex_none()
        .w(px(SWITCH_WIDTH))
        .h(px(36.0))
        .child(SwitchVisual {
            theme: theme.clone(),
            on,
            key: format!("settings-switch-{key}").into(),
        })
}

#[derive(IntoElement)]
struct SwitchVisual {
    theme: Theme,
    on: bool,
    key: SharedString,
}

struct SwitchTravel {
    from: f32,
    target: f32,
    started: std::time::Instant,
}

impl SwitchTravel {
    fn value(&self, now: std::time::Instant) -> f32 {
        let t = (now.duration_since(self.started).as_secs_f32() / 0.18).min(1.0);
        self.from + (self.target - self.from) * (1.0 - (1.0 - t).powi(3))
    }
}

fn switch_track_color(theme: &Theme, on: bool) -> gpui::Hsla {
    let dark = theme.appearance.is_dark();
    if on {
        if dark {
            theme.accent_strong
        } else {
            // Preserve the current light opaque treatment.
            let accent = theme.accent;
            crate::theme::flatten(
                gpui::hsla(accent.h, accent.s, accent.l + (1.0 - accent.l) * 0.10, 0.98),
                theme.surface,
            )
        }
    } else {
        let opacity = match (dark, theme.is_frost()) {
            (true, true) => 0.22,
            (true, false) => 0.18,
            (false, true) => 0.12,
            (false, false) => 0.10,
        };
        crate::theme::flatten(theme.ink(opacity), theme.surface)
    }
}

fn switch_thumb_color(theme: &Theme) -> gpui::Hsla {
    let white = if theme.is_frost() {
        if theme.appearance.is_dark() {
            0.94
        } else {
            0.96
        }
    } else if theme.appearance.is_dark() {
        0.96
    } else {
        1.0
    };
    crate::theme::flatten(gpui::white().opacity(white), theme.surface)
}

impl RenderOnce for SwitchVisual {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        let now = std::time::Instant::now();
        let target = if self.on { 1.0 } else { 0.0 };
        let reduced = crate::motion::reduced_motion(cx);
        let position = window.with_global_id(self.key.into(), |id, window| {
            window.with_element_state(id, |previous: Option<SwitchTravel>, _| {
                let mut travel = previous.unwrap_or(SwitchTravel {
                    from: target,
                    target,
                    started: now,
                });
                let current = travel.value(now);
                if travel.target != target {
                    travel = SwitchTravel {
                        from: current,
                        target,
                        started: now,
                    };
                }
                if reduced {
                    travel.from = target;
                    travel.target = target;
                }
                (travel.value(now), travel)
            })
        });
        if (position - target).abs() > 0.001 {
            window.request_animation_frame();
        }
        let dark = self.theme.appearance.is_dark();
        let track = switch_track_color(&self.theme, self.on);
        let empty_width = SWITCH_WIDTH - SWITCH_THUMB_WIDTH - SWITCH_SIDE_INSET;
        let mark_padding = (empty_width - SWITCH_MARK_SIZE) / 2.0;
        let thumb_left = SWITCH_SIDE_INSET
            + (SWITCH_WIDTH - SWITCH_THUMB_WIDTH - 2.0 * SWITCH_SIDE_INSET) * position;
        let track_element = div()
            .absolute()
            .top(px(6.0))
            .left_0()
            .w(px(SWITCH_WIDTH))
            .h(px(24.0))
            .rounded_full()
            .bg(track)
            .border_1()
            .border_color(if self.on {
                crate::theme::flatten(
                    gpui::white().opacity(if self.theme.is_frost() { 0.16 } else { 0.12 }),
                    track,
                )
            } else {
                crate::theme::flatten(self.theme.border, track)
            })
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .px(px(mark_padding))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .size(px(SWITCH_MARK_SIZE))
                            .flex()
                            .items_center()
                            .justify_center()
                            .opacity(position)
                            .child(
                                div()
                                    .w(px(1.5))
                                    .h(px(9.0))
                                    .rounded_full()
                                    .bg(gpui::white().opacity(0.96)),
                            ),
                    )
                    .child(
                        div()
                            .size(px(SWITCH_MARK_SIZE))
                            .flex()
                            .items_center()
                            .justify_center()
                            .opacity(1.0 - position)
                            .child(
                                div()
                                    .size(px(8.0))
                                    .rounded_full()
                                    .border(px(1.25))
                                    .border_color(gpui::white().opacity(0.92)),
                            ),
                    ),
            );
        let thumb_element = div()
            .absolute()
            .top(px(7.0))
            .left(px(thumb_left))
            .w(px(SWITCH_THUMB_WIDTH))
            .h(px(22.0))
            .rounded_full()
            .bg(switch_thumb_color(&self.theme))
            .border_1()
            .border_color(crate::theme::flatten(
                gpui::black().opacity(if dark { 0.07 } else { 0.05 }),
                switch_thumb_color(&self.theme),
            ));
        div()
            .relative()
            .w(px(SWITCH_WIDTH))
            .h(px(36.0))
            .child(track_element)
            .child(thumb_element)
    }
}

#[cfg(test)]
mod switch_tests {
    use super::*;

    #[test]
    fn dropdowns_stay_in_the_settings_content_pane() {
        for width in [600.0, 1200.0] {
            let viewport = gpui::size(px(width), px(700.0));
            let modal = modal_bounds(viewport);
            let limits = dropdown_limits(viewport);
            let sidebar = if width < 680.0 { 72.0 } else { 216.0 };
            assert!(limits.left() >= modal.left() + px(sidebar));
            assert!(limits.right() <= modal.right());
            assert!(limits.top() >= modal.top());
            assert!(limits.bottom() <= modal.bottom());
        }
    }

    #[test]
    fn switch_material_keeps_dark_accent_and_opaque_fills() {
        use zeron_theme::SurfaceTreatment;

        let mut dark = Theme::dark();
        dark.surface_treatment = SurfaceTreatment::Opaque;
        assert_eq!(switch_track_color(&dark, true), dark.accent_strong);
        assert_eq!(switch_track_color(&dark, false).a, 1.0);
        assert_eq!(switch_thumb_color(&dark).a, 1.0);
        let opaque_off = switch_track_color(&dark, false);

        dark.surface_treatment = SurfaceTreatment::Frosted;
        assert_eq!(switch_track_color(&dark, true), dark.accent_strong);
        assert_eq!(switch_track_color(&dark, false).a, 1.0);
        assert_eq!(switch_thumb_color(&dark).a, 1.0);
        assert_ne!(switch_track_color(&dark, false), opaque_off);

        let mut light = Theme::light();
        light.surface_treatment = SurfaceTreatment::Opaque;
        assert_eq!(switch_track_color(&light, true).a, 1.0);
        assert!(switch_track_color(&light, true).l > light.accent.l);
    }

    #[test]
    fn switch_reversal_keeps_current_position_and_settles() {
        use std::time::{Duration, Instant};
        let now = Instant::now();
        let forward = SwitchTravel {
            from: 0.0,
            target: 1.0,
            started: now,
        };
        let halfway = now + Duration::from_millis(90);
        let current = forward.value(halfway);
        let reverse = SwitchTravel {
            from: current,
            target: 0.0,
            started: halfway,
        };
        assert_eq!(reverse.value(halfway), current);
        assert_eq!(reverse.value(halfway + Duration::from_millis(180)), 0.0);
        assert_eq!(forward.value(now + Duration::from_millis(180)), 1.0);
    }
}

/// Small choices reuse the selected/hover treatment of the app's picker rows.
pub fn choice(theme: &Theme, selected: bool, key: impl Into<SharedString>) -> gpui::Div {
    crate::popover::menu_row(theme, selected, key)
        .min_h(px(32.0))
        .px(px(10.0))
        .font_weight(if selected {
            gpui::FontWeight::MEDIUM
        } else {
            gpui::FontWeight::NORMAL
        })
}

/// Settings actions share one size, corner radius, and hover language. Use
/// outlined for selectors, quiet for secondary actions, and solid for commits.
#[derive(Clone, Copy)]
pub enum ActionTone {
    Quiet,
    Outlined,
    Solid,
}

pub fn action_button(theme: &Theme, tone: ActionTone) -> gpui::Div {
    let button = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .rounded(px(8.0))
        .min_h(px(32.0))
        .px(px(10.0))
        .py(px(5.0))
        .text_size(crate::typography::ui_rems(12.5))
        .cursor_pointer();
    match tone {
        ActionTone::Quiet => button
            .text_color(theme.text_muted)
            .hover(|s| s.bg(theme.glass_hover()).text_color(theme.text)),
        ActionTone::Outlined => button
            .bg(theme.input_glass_bg())
            .border_1()
            .border_color(theme.border)
            .text_color(theme.text)
            .hover(|s| s.bg(theme.glass_hover()).border_color(theme.border_strong)),
        ActionTone::Solid => button
            .bg(theme.solid)
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(theme.on_solid)
            .hover(|s| s.opacity(0.9)),
    }
}

pub fn text_action(theme: &Theme, tone: ActionTone, label: impl Into<SharedString>) -> gpui::Div {
    action_button(theme, tone).child(label.into())
}

pub fn icon_action(theme: &Theme) -> gpui::Div {
    action_button(theme, ActionTone::Outlined)
        .size(px(32.0))
        .min_h(px(32.0))
        .p(px(0.0))
        .justify_center()
}

pub fn ghost_action(theme: &Theme) -> gpui::Div {
    action_button(theme, ActionTone::Quiet)
}

/// The dismissible red error strip (`flex items-start gap-2 rounded-xl border
/// border-red-400/20 bg-red-400/[0.06] text-red-300/90` with a leading
/// `DangerTriangle mt-0.5 size-4`).
pub fn error_strip(theme: &Theme, message: impl Into<SharedString>) -> gpui::Div {
    let red = theme.danger; // red-400
    let red_text = theme.danger_muted; // red-300
    div()
        .mt(px(16.0))
        .px(px(16.0))
        .py(px(12.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(red.opacity(0.2))
        .bg(red.opacity(0.06))
        .text_size(crate::typography::ui_rems(12.5))
        .text_color(red_text.opacity(0.9))
        .flex()
        .flex_row()
        .items_start()
        .gap(px(8.0))
        .child(
            div().flex_none().mt(px(2.0)).child(
                crate::icons::icon(crate::icons::DANGER_TRIANGLE)
                    .size(px(16.0))
                    .text_color(red_text.opacity(0.9)),
            ),
        )
        .child(div().min_w_0().child(message.into()))
}

/// The amber warning strip (`flex items-start gap-2 border-amber-400/20
/// bg-amber-400/[0.06] text-amber-200/90` with a leading `DangerTriangle
/// mt-0.5 size-3.5`).
pub fn warning_strip(theme: &Theme, message: impl Into<SharedString>) -> gpui::Div {
    let amber = theme.warning; // amber-400
    let amber_text = theme.warning_muted; // amber-200
    div()
        .mt(px(8.0))
        .px(px(16.0))
        .py(px(10.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(amber.opacity(0.2))
        .bg(amber.opacity(0.06))
        .text_size(crate::typography::ui_rems(12.0))
        .text_color(amber_text.opacity(0.9))
        .flex()
        .flex_row()
        .items_start()
        .gap(px(8.0))
        .child(
            div().flex_none().mt(px(2.0)).child(
                crate::icons::icon(crate::icons::DANGER_TRIANGLE)
                    .size(px(14.0))
                    .text_color(amber_text.opacity(0.9)),
            ),
        )
        .child(div().min_w_0().child(message.into()))
}

/// The sidebar's paint-time overflow fade, with a persistent scroll handle per
/// settings surface. No fade is painted when the content fits or at a reached edge.
pub fn scroll_faded(
    key: impl Into<SharedString>,
    area: gpui::Stateful<gpui::Div>,
) -> impl IntoElement {
    SettingsScroll {
        key: key.into(),
        area,
    }
}

#[derive(IntoElement)]
struct SettingsScroll {
    key: SharedString,
    area: gpui::Stateful<gpui::Div>,
}

impl RenderOnce for SettingsScroll {
    fn render(self, window: &mut gpui::Window, _: &mut gpui::App) -> impl IntoElement {
        let scroll = window.with_global_id(self.key.into(), |id, window| {
            window.with_element_state(id, |previous: Option<gpui::ScrollHandle>, _| {
                let scroll = previous.unwrap_or_default();
                (scroll.clone(), scroll)
            })
        });
        crate::edge_fade::edge_faded(16.0, true, true, self.area.track_scroll(&scroll))
            .fade_overflow_y(&scroll)
    }
}
