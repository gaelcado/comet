//! A project's widgets on the new-chat screen: a row of floating cards under
//! the composer, each with a miniature of what its widget shows. A card
//! opens its widget in a sheet over the window; past what the row holds, the
//! last card opens all of them.
//!
//! Miniatures are drawn from the preview the owner takes whenever a widget's
//! content is published (`ProjectWidgetContent::preview`): its headline
//! figures, its first chart's shape, its first table's top rows or its
//! opening text. They need no bundle read, so a card costs one snapshot
//! row, and a refresh updates the miniature with the new data.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    AnyElement, Background, Bounds, Context, Corners, Entity, EventEmitter, FocusHandle, Hsla,
    PathBuilder, Pixels, Point, SharedString, Task, Window, canvas, div, linear_color_stop,
    linear_gradient, point, prelude::*, px,
};
use zeron_proto::{
    PROJECT_WIDGET_PREVIEW_SCALE, ProjectWidget, ProjectWidgetPreview, ProjectWidgetPreviewChart,
    ProjectWidgetPreviewChartKind, ProjectWidgetRefreshState, ProjectWidgetsSnapshot,
    WatchProjectWidgetsParams, capabilities,
};
use zeron_rpc::{RpcError, methods};

use crate::icons::{self, icon};
use crate::motion;
use crate::project_widgets::WidgetsTarget;
use crate::state::AppState;
use crate::theme::Theme;

pub(crate) const CARD_WIDTH: f32 = 212.0;
pub(crate) const CARD_HEIGHT: f32 = 152.0;
const CARD_GAP: f32 = 12.0;
/// The card around its miniature tile: concentric, 18 = 12 + 6.
const CARD_RADIUS: f32 = 18.0;
const CARD_INSET: f32 = 6.0;
const TILE_RADIUS: f32 = 12.0;
const TILE_HEIGHT: f32 = 94.0;
/// Inside the tile, and the title's inset under it: one shared edge.
const TILE_PADDING: f32 = 8.0;
const MAX_SLOTS: usize = 5;
/// The row's height, room for a hovered card's lift included.
pub(crate) const SHELF_HEIGHT: f32 = CARD_HEIGHT + HOVER_LIFT;
/// First load: cards rise in one after another.
const ENTER: Duration = Duration::from_millis(320);
const ENTER_STAGGER: Duration = Duration::from_millis(100);
const ENTER_RISE: f32 = 8.0;
/// A refresh that lands crossfades the miniature.
const CROSSFADE: Duration = Duration::from_millis(420);
/// The sheen sweeping a refreshing tile.
const SHEEN: motion::MotionSpec =
    motion::MotionSpec::new(1700, motion::CubicBezier::new(0.0, 0.0, 1.0, 1.0));
const HOVER_LIFT: f32 = 2.0;

pub enum WidgetShelfEvent {
    /// Show this widget in the sheet.
    Open {
        target: WidgetsTarget,
        widget_id: String,
        /// The card's window bounds, so the sheet can grow out of it.
        origin: Option<Bounds<gpui::Pixels>>,
    },
    /// The "+N" card: every widget of the project.
    ManageAll(WidgetsTarget),
}

pub struct WidgetShelf {
    state: Entity<AppState>,
    target: Option<WidgetsTarget>,
    /// Bumped with every target, so a reply for an earlier one is dropped.
    generation: u64,
    widgets: Vec<ProjectWidget>,
    watch_task: Option<Task<()>>,
    available_width: f32,
    card_focus: HashMap<String, FocusHandle>,
    more_focus: FocusHandle,
    /// When this target's widgets first arrived, for the entrance.
    entered_at: Option<Instant>,
    /// Widgets whose content changed while shown: the miniature they
    /// showed before, and when it changed.
    changed: HashMap<String, (Option<ProjectWidgetPreview>, Instant)>,
    /// Each shown card's window bounds, from its last paint.
    card_bounds: Rc<RefCell<HashMap<String, Bounds<gpui::Pixels>>>>,
    /// The widget open in the sheet: its card steps aside meanwhile.
    lifted: Option<String>,
    /// Fixtures freeze time to capture motion mid-flight.
    #[cfg(feature = "project-widgets-fixture")]
    frozen_at: Option<Instant>,
}

impl EventEmitter<WidgetShelfEvent> for WidgetShelf {}

impl WidgetShelf {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        Self {
            state,
            target: None,
            generation: 0,
            widgets: Vec::new(),
            watch_task: None,
            available_width: 900.0,
            card_focus: HashMap::new(),
            more_focus: cx.focus_handle(),
            entered_at: None,
            changed: HashMap::new(),
            card_bounds: Default::default(),
            lifted: None,
            #[cfg(feature = "project-widgets-fixture")]
            frozen_at: None,
        }
    }

    fn now(&self) -> Instant {
        #[cfg(feature = "project-widgets-fixture")]
        if let Some(at) = self.frozen_at {
            return at;
        }
        Instant::now()
    }

    /// Follow the space the new-chat screen is in (`None` off it). Same
    /// target: nothing reloads.
    pub fn set_target(&mut self, target: Option<WidgetsTarget>, cx: &mut Context<Self>) {
        let supported = target.as_ref().is_some_and(|target| {
            self.state
                .read(cx)
                .device_supports(&target.device_id, capabilities::PROJECT_WIDGETS_V1)
        });
        let target = target.filter(|_| supported);
        if target == self.target {
            return;
        }
        self.target = target;
        self.generation += 1;
        self.widgets.clear();
        self.card_focus.clear();
        self.changed.clear();
        self.entered_at = None;
        self.watch(cx);
        cx.notify();
    }

    pub fn set_available_width(&mut self, width: f32) {
        self.available_width = width;
    }

    /// Whether there is anything to show.
    pub fn is_empty(&self) -> bool {
        self.widgets.is_empty()
    }

    fn watch(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.target.clone() else {
            self.watch_task = None;
            return;
        };
        let generation = self.generation;
        let routed = {
            let state = self.state.read(cx);
            (state.local_device_id.as_deref() != Some(target.device_id.as_str()))
                .then(|| target.device_id.clone())
        };
        let params = serde_json::to_value(WatchProjectWidgetsParams {
            space_id: target.space_id.clone(),
            target_device_id: routed,
        })
        .unwrap_or_default();
        self.watch_task = Some(cx.spawn(async move |this, cx| {
            let mut retry = 1;
            loop {
                let Ok(engine) =
                    this.update(cx, |shelf, cx| shelf.state.read(cx).engine().cloned())
                else {
                    return;
                };
                if let Some(engine) = engine {
                    match engine
                        .client()
                        .subscribe_checked(methods::WATCH_PROJECT_WIDGETS, params.clone())
                        .await
                    {
                        Ok(mut stream) => {
                            while let Some(value) = stream.recv().await {
                                retry = 1;
                                let Ok(snapshot) =
                                    serde_json::from_value::<ProjectWidgetsSnapshot>(value)
                                else {
                                    continue;
                                };
                                let alive = this.update(cx, |shelf, cx| {
                                    shelf.apply(generation, snapshot);
                                    cx.notify();
                                });
                                if alive.is_err() {
                                    return;
                                }
                            }
                        }
                        // An owner without widgets: nothing to show, ever.
                        Err(RpcError::UnknownMethod(_)) => return,
                        Err(_) => {}
                    }
                }
                // What was shown stays until the owner answers again.
                cx.background_executor()
                    .timer(Duration::from_secs(retry))
                    .await;
                retry = (retry * 2).min(30);
            }
        }));
    }

    fn apply(&mut self, generation: u64, snapshot: ProjectWidgetsSnapshot) {
        let same_space = self
            .target
            .as_ref()
            .is_some_and(|target| target.space_id == snapshot.space_id);
        if generation != self.generation || !same_space {
            return;
        }
        self.card_focus
            .retain(|id, _| snapshot.widgets.iter().any(|w| &w.id == id));
        let now = Instant::now();
        if self.widgets.is_empty() && !snapshot.widgets.is_empty() {
            self.entered_at = Some(now);
        }
        for widget in &snapshot.widgets {
            if let Some(shown) = self.widgets.iter().find(|w| w.id == widget.id)
                && shown.content_revision != widget.content_revision
            {
                self.changed
                    .insert(widget.id.clone(), (shown.content.preview.clone(), now));
            }
        }
        self.changed
            .retain(|id, _| snapshot.widgets.iter().any(|w| &w.id == id));
        self.widgets = snapshot.widgets;
    }

    fn focus_of(&mut self, id: &str, cx: &mut Context<Self>) -> FocusHandle {
        self.card_focus
            .entry(id.to_owned())
            .or_insert_with(|| cx.focus_handle())
            .clone()
    }

    fn open(&mut self, widget_id: &str, cx: &mut Context<Self>) {
        if let Some(target) = self.target.clone() {
            cx.emit(WidgetShelfEvent::Open {
                target,
                widget_id: widget_id.to_owned(),
                origin: self.card_bounds(widget_id),
            });
        }
    }

    /// Where `widget_id`'s card was last painted.
    pub fn card_bounds(&self, widget_id: &str) -> Option<Bounds<gpui::Pixels>> {
        self.card_bounds.borrow().get(widget_id).copied()
    }

    /// The sheet shows `widget_id`: its card steps aside until the sheet
    /// lands back on it.
    pub fn set_lifted(&mut self, widget_id: Option<String>, cx: &mut Context<Self>) {
        if self.lifted != widget_id {
            self.lifted = widget_id;
            cx.notify();
        }
    }

    /// How many cards a spread row can hold, and how many widgets the
    /// "+N" card stands for.
    fn slots(&self) -> (usize, usize) {
        let fit = ((self.available_width - 48.0 + CARD_GAP) / (CARD_WIDTH + CARD_GAP))
            .floor()
            .max(1.0) as usize;
        let cap = fit.min(MAX_SLOTS);
        let n = self.widgets.len();
        if n <= cap || cap < 2 {
            (n.min(cap.max(1)), 0)
        } else {
            (cap - 1, n - (cap - 1))
        }
    }
}

#[cfg(feature = "project-widgets-fixture")]
impl WidgetShelf {
    /// Show a refresh landing on `widget_id`: its miniature crossfades from
    /// `before`, `progress` (0..1) of the way through.
    pub fn fixture_changed(
        &mut self,
        widget_id: &str,
        before: Option<ProjectWidgetPreview>,
        progress: f32,
    ) {
        let now = Instant::now();
        self.frozen_at = Some(now);
        let at = now - CROSSFADE.mul_f32(progress.clamp(0.0, 1.0));
        self.changed.insert(widget_id.to_owned(), (before, at));
    }

    /// Show the card in `slot` hovered.
    pub fn fixture_hover(&mut self, slot: usize, cx: &mut Context<Self>) {
        let key = format!("widget-card-hover-{}-{slot}", cx.entity_id());
        motion::set_hover(&key, true, true);
        cx.notify();
    }

    /// Show `widgets` for `target` without an engine, spread or stacked.
    pub fn fixture_show(
        &mut self,
        target: WidgetsTarget,
        widgets: Vec<ProjectWidget>,
        spread: bool,
        cx: &mut Context<Self>,
    ) {
        self.target = Some(target);
        self.watch_task = None;
        let _ = spread;
        self.widgets = widgets;
        self.lifted = None;
        self.entered_at = None;
        self.frozen_at = None;
        cx.notify();
    }
}

impl Render for WidgetShelf {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.widgets.is_empty() || self.target.is_none() {
            return gpui::Empty.into_any_element();
        }
        let theme = Theme::of(cx).clone();
        let reduced = motion::reduced_motion(cx);
        let now = self.now();
        let (shown, more) = self.slots();
        let slots = shown + usize::from(more > 0);
        let widgets: Vec<ProjectWidget> = self.widgets.iter().take(shown).cloned().collect();
        let behind: Vec<String> = self
            .widgets
            .iter()
            .skip(shown)
            .map(|w| w.title.clone())
            .collect();
        let cards: Vec<(ProjectWidget, FocusHandle)> = widgets
            .into_iter()
            .map(|w| {
                let focus = self.focus_of(&w.id, cx);
                (w, focus)
            })
            .collect();

        // One-shot motion (entrance, crossfades) drives its own frames.
        let entered_at = self.entered_at;
        let entering = |slot: usize| -> f32 {
            match entered_at {
                Some(at) if !reduced => {
                    let start = ENTER_STAGGER * slot as u32;
                    let elapsed = now.saturating_duration_since(at).saturating_sub(start);
                    motion::EASE_OUT_SOFT.eval(elapsed.as_secs_f32() / ENTER.as_secs_f32())
                }
                _ => 1.0,
            }
        };
        self.changed
            .retain(|_, (_, at)| !reduced && now.saturating_duration_since(*at) < CROSSFADE);
        let entrance_done = entering(slots.saturating_sub(1)) >= 1.0;
        if entrance_done {
            self.entered_at = None;
        }
        if !entrance_done || !self.changed.is_empty() {
            window.request_animation_frame();
        }
        let palette = crate::visuals::chart_series(&theme);
        self.card_bounds.borrow_mut().clear();

        // One centered row, each card in its own slot: nothing moves under
        // the pointer. A card lifts a little on hover and rises in, one
        // after another, when the project's widgets first load.
        let mut row = div()
            .id("widget-shelf")
            .role(gpui::Role::Group)
            .aria_label(format!(
                "{} widget{} in this project",
                self.widgets.len(),
                if self.widgets.len() == 1 { "" } else { "s" }
            ))
            .h(px(SHELF_HEIGHT))
            .pt(px(HOVER_LIFT))
            .flex()
            .flex_row()
            .items_start()
            .gap(px(CARD_GAP));
        for slot in 0..slots {
            let hover_key = format!("widget-card-hover-{}-{slot}", cx.entity_id());
            let hover = motion::hover_t(&hover_key);
            let enter = entering(slot);
            let look = CardLook { hover, hover_key };
            let (card, lifted, measured) = match cards.get(slot) {
                Some((widget, focus)) => {
                    let lifted = self.lifted.as_deref() == Some(widget.id.as_str());
                    let (bounds, id) = (self.card_bounds.clone(), widget.id.clone());
                    let measured = gpui::canvas(
                        move |b, _, _| {
                            bounds.borrow_mut().insert(id.clone(), b);
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset_0();
                    let card =
                        self.render_card(widget, slot, focus.clone(), look, &palette, &theme, cx);
                    (card, lifted, Some(measured))
                }
                None => (
                    self.render_more(more, &behind, look, &palette, &theme, cx),
                    false,
                    None,
                ),
            };
            row = row.child(
                div()
                    .relative()
                    .flex_none()
                    .w(px(CARD_WIDTH))
                    .h(px(CARD_HEIGHT))
                    .top(px((1.0 - enter) * ENTER_RISE - HOVER_LIFT * hover))
                    .opacity(enter)
                    // Lifted into the sheet, the card isn't drawn at all
                    // (paths ignore a parent's opacity); its slot stays.
                    .when(!lifted, |el| {
                        el.child(crate::frost::frosted(
                            CARD_RADIUS,
                            crate::frost::MENU_BLUR,
                            card,
                        ))
                    })
                    .children(measured),
            );
        }
        row.into_any_element()
    }
}

/// How a card is drawn this frame.
struct CardLook {
    /// Hover progress, for its lift and edge.
    hover: f32,
    hover_key: String,
}

impl WidgetShelf {
    #[allow(clippy::too_many_arguments)]
    fn render_card(
        &mut self,
        widget: &ProjectWidget,
        slot: usize,
        focus: FocusHandle,
        look: CardLook,
        palette: &[Hsla; 6],
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = widget.id.clone();
        let key_id = id.clone();
        let (status, status_glyph, tone) = card_status(widget, theme, cx);
        let refreshing = matches!(
            widget.refresh.as_ref().map(|r| r.state),
            Some(ProjectWidgetRefreshState::Queued | ProjectWidgetRefreshState::Running)
        );
        let sheen = if refreshing {
            Some(motion::pulse_delta(&SHEEN, cx.entity_id(), cx)).filter(|p| *p > 0.0)
        } else {
            None
        };
        let inner = CARD_WIDTH - 2.0 * CARD_INSET;
        let now_preview = widget.content.preview.clone();
        let before = self.changed.get(&widget.id).map(|(preview, at)| {
            let progress =
                self.now().saturating_duration_since(*at).as_secs_f32() / CROSSFADE.as_secs_f32();
            (preview.clone(), motion::EASE_OUT_SOFT.eval(progress))
        });
        let tile = tile()
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .opacity(before.as_ref().map_or(1.0, |(_, c)| *c))
                    .child(miniature(now_preview.as_ref(), inner, palette, theme)),
            )
            .when_some(before, |el, (preview, c)| {
                el.child(div().absolute().inset_0().opacity(1.0 - c).child(miniature(
                    preview.as_ref(),
                    inner,
                    palette,
                    theme,
                )))
            })
            .when_some(sheen, |el, phase| el.child(sheen_band(phase, inner, theme)));
        card_surface(theme, &look)
            .id(SharedString::from(format!("widget-card-{id}")))
            // Pressed, the card settles back down.
            .active(|s| s.mt(px(1.0)))
            .role(gpui::Role::Button)
            .aria_label(format!("{} · {status}", widget.title))
            .track_focus(&focus.tab_stop(true))
            .tab_index(0)
            .focus_visible(|s| s.border_2().border_color(theme.accent))
            .cursor_pointer()
            .on_hover(motion::hover_listener(look.hover_key.clone()))
            .on_click(cx.listener(move |shelf, _, _, cx| shelf.open(&id, cx)))
            .on_key_down(
                cx.listener(move |shelf, event: &gpui::KeyDownEvent, window, cx| {
                    if event.is_held {
                        return;
                    }
                    match event.keystroke.key.as_str() {
                        "enter" | "space" => shelf.open(&key_id, cx),
                        "left" | "right" => {
                            let ids: Vec<String> =
                                shelf.widgets.iter().map(|w| w.id.clone()).collect();
                            let step: isize = if event.keystroke.key == "left" { -1 } else { 1 };
                            let next = slot as isize + step;
                            if let Some(next) = usize::try_from(next).ok().and_then(|n| ids.get(n))
                            {
                                let handle = shelf.focus_of(next, cx);
                                window.focus(&handle, cx);
                            }
                        }
                        _ => return,
                    }
                    cx.stop_propagation();
                }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .h_full()
                    .child(crate::rounded_clip::rounded_clip(TILE_RADIUS, tile))
                    .child(card_caption(
                        div()
                            .truncate()
                            .text_size(crate::typography::ui_rems(12.5))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(SharedString::from(widget.title.clone())),
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(5.0))
                            .text_size(crate::typography::ui_rems(11.5))
                            .text_color(tone)
                            .children(status_glyph)
                            .child(div().truncate().child(SharedString::from(status))),
                    )),
            )
            .into_any_element()
    }

    /// The last slot when the row can't hold every widget: the titles
    /// waiting behind it, and the way to all of them.
    #[allow(clippy::too_many_arguments)]
    fn render_more(
        &mut self,
        more: usize,
        titles: &[String],
        look: CardLook,
        palette: &[Hsla; 6],
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let emit = |shelf: &mut Self, cx: &mut Context<Self>| {
            if let Some(target) = shelf.target.clone() {
                cx.emit(WidgetShelfEvent::ManageAll(target));
            }
        };
        let mut list = div().p(px(TILE_PADDING)).flex().flex_col().gap(px(6.0));
        for (ix, title) in titles.iter().take(3).enumerate() {
            list = list.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(7.0))
                    .child(
                        div()
                            .size(px(6.0))
                            .flex_none()
                            .rounded_full()
                            .bg(palette[ix % palette.len()]),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(crate::typography::ui_rems(11.5))
                            .text_color(theme.text_muted)
                            .child(SharedString::from(title.clone())),
                    ),
            );
        }
        card_surface(theme, &look)
            .id("widget-card-more")
            .active(|s| s.mt(px(1.0)))
            .role(gpui::Role::Button)
            .aria_label(format!("All widgets, {more} more"))
            .track_focus(&self.more_focus.clone().tab_stop(true))
            .tab_index(0)
            .focus_visible(|s| s.border_2().border_color(theme.accent))
            .cursor_pointer()
            .on_hover(motion::hover_listener(look.hover_key.clone()))
            .on_click(cx.listener(move |shelf, _, _, cx| emit(shelf, cx)))
            .on_key_down(
                cx.listener(move |shelf, event: &gpui::KeyDownEvent, _, cx| {
                    if !event.is_held && matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        emit(shelf, cx);
                        cx.stop_propagation();
                    }
                }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .h_full()
                    .child(crate::rounded_clip::rounded_clip(
                        TILE_RADIUS,
                        tile().child(list),
                    ))
                    .child(card_caption(
                        div()
                            .text_size(crate::typography::ui_rems(12.5))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child("All widgets"),
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(4.0))
                            .text_size(crate::typography::ui_rems(11.5))
                            .text_color(theme.text_faint)
                            .child(SharedString::from(format!("{more} more")))
                            .child(
                                icon(icons::ARROW_RIGHT)
                                    .size(px(11.0))
                                    .text_color(theme.text_faint),
                            ),
                    )),
            )
            .into_any_element()
    }
}

/// A card is the composer's own material, the pill it floats beside: its
/// frosted tint and edge, its shadow off glass. Hover warms the edge a
/// little (a state, so a border).
fn card_surface(theme: &Theme, look: &CardLook) -> gpui::Div {
    let edge = motion::mix(
        theme.composer_surface_border(),
        theme.border_strong,
        0.5 * look.hover,
    );
    div()
        .w(px(CARD_WIDTH))
        .h(px(CARD_HEIGHT))
        .p(px(CARD_INSET))
        .rounded(px(CARD_RADIUS))
        .border_1()
        .border_color(edge)
        .bg(theme.composer_surface_bg())
        .when(!theme.is_frost(), |el| el.shadow_lg())
}

/// The miniature's tile: a quiet inset well with a hairline edge, clipped
/// to its corners so a chart can bleed to them.
fn tile() -> gpui::Stateful<gpui::Div> {
    div()
        .id("widget-tile")
        .relative()
        .w_full()
        .h(px(TILE_HEIGHT))
        .flex_none()
        .rounded(px(TILE_RADIUS))
        .bg(crate::theme::ink(0.04))
        .border_1()
        .border_color(crate::theme::hairline(0.06))
        .overflow_hidden()
}

/// Tabular figures, so values keep their width as they change.
fn tabular() -> gpui::FontFeatures {
    gpui::FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)]))
}

/// Title and status under the tile, on the tile content's leading edge.
fn card_caption(title: gpui::Div, status: gpui::Div) -> gpui::Div {
    div()
        .pt(px(9.0))
        .px(px(TILE_PADDING + 1.0))
        .flex()
        .flex_col()
        .gap(px(2.0))
        .min_w_0()
        .child(title)
        .child(status)
}

/// A soft band of light sweeping a refreshing tile, `phase` 0..1.
fn sheen_band(phase: f32, width: f32, theme: &Theme) -> AnyElement {
    let band = 72.0;
    let x = -band + phase * (width + band);
    let glow = gpui::hsla(
        0.0,
        0.0,
        1.0,
        if theme.appearance.is_light() {
            0.55
        } else {
            0.07
        },
    );
    let clear = glow.opacity(0.0);
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .left(px(x))
        .w(px(band))
        .flex()
        .flex_row()
        .child(div().flex_1().h_full().bg(linear_gradient(
            90.0,
            linear_color_stop(clear, 0.0),
            linear_color_stop(glow, 1.0),
        )))
        .child(div().flex_1().h_full().bg(linear_gradient(
            90.0,
            linear_color_stop(glow, 0.0),
            linear_color_stop(clear, 1.0),
        )))
        .into_any_element()
}

/// The card's status line and its glyph, from its latest refresh.
fn card_status(
    widget: &ProjectWidget,
    theme: &Theme,
    cx: &mut Context<WidgetShelf>,
) -> (String, Option<AnyElement>, Hsla) {
    let updated = || {
        let at = widget.last_successful_at.unwrap_or(widget.updated_at);
        let at = chrono::DateTime::from_timestamp_millis(at).unwrap_or_else(chrono::Utc::now);
        format!(
            "Updated {}",
            crate::pull_requests::compact_relative_time(at, chrono::Utc::now())
        )
    };
    match widget.refresh.as_ref().map(|r| r.state) {
        Some(ProjectWidgetRefreshState::Queued | ProjectWidgetRefreshState::Running) => (
            "Refreshing…".into(),
            Some(
                crate::loaders::mini_glyph_spinner(
                    SharedString::from(format!("widget-card-spinner-{}", widget.id)),
                    2.0,
                    theme.glyph,
                    cx.entity_id(),
                    cx,
                )
                .into_any_element(),
            ),
            theme.text_muted,
        ),
        Some(ProjectWidgetRefreshState::AwaitingInput) => (
            "Needs a response".into(),
            Some(
                div()
                    .size(px(6.0))
                    .rounded_full()
                    .bg(theme.accent)
                    .into_any_element(),
            ),
            theme.text_muted,
        ),
        Some(
            ProjectWidgetRefreshState::Failed
            | ProjectWidgetRefreshState::Interrupted
            | ProjectWidgetRefreshState::Conflict,
        ) => (
            "Refresh failed".into(),
            Some(
                icon(icons::DANGER_TRIANGLE)
                    .size(px(11.0))
                    .text_color(theme.danger_muted)
                    .into_any_element(),
            ),
            theme.danger_muted,
        ),
        _ => (updated(), None, theme.text_faint),
    }
}

// ---- miniatures ------------------------------------------------------------

/// What a tile shows of its widget, `width` wide and [`TILE_HEIGHT`] tall.
pub(crate) fn miniature(
    preview: Option<&ProjectWidgetPreview>,
    width: f32,
    palette: &[Hsla; 6],
    theme: &Theme,
) -> AnyElement {
    let Some(preview) = preview.filter(|p| !p.is_empty() && !only_graphic(p)) else {
        return placeholder(preview.is_some_and(|p| p.graphic), theme);
    };
    if let Some(chart) = &preview.chart {
        let bleeds = chart.kind != ProjectWidgetPreviewChartKind::Bar;
        let surface = crate::theme::flatten(
            crate::theme::ink(0.04),
            crate::theme::flatten(theme.composer_surface_bg(), theme.bg),
        );
        let element = chart_canvas(chart.clone(), *palette, surface, theme);
        return match preview.stats.first() {
            Some(stat) => div()
                .size_full()
                .flex()
                .flex_col()
                .child(
                    div()
                        .px(px(TILE_PADDING))
                        .pt(px(TILE_PADDING - 1.0))
                        .child(stat_line(stat, theme, 17.0)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .pt(px(4.0))
                        .when(!bleeds, |el| el.px(px(TILE_PADDING)))
                        .child(element),
                )
                .into_any_element(),
            None => div()
                .size_full()
                .pt(px(14.0))
                .when(!bleeds, |el| el.px(px(TILE_PADDING)))
                .child(element)
                .into_any_element(),
        };
    }
    if !preview.stats.is_empty() {
        let shown = if width >= 170.0 { 2 } else { 1 };
        return div()
            .size_full()
            .p(px(TILE_PADDING))
            .pb(px(TILE_PADDING + 2.0))
            .flex()
            .flex_row()
            .items_end()
            .gap(px(12.0))
            .children(preview.stats.iter().take(shown).map(|stat| {
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(5.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(crate::typography::ui_rems(if shown > 1 {
                                19.0
                            } else {
                                24.0
                            }))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .font_features(tabular())
                            .child(SharedString::from(stat.value.clone())),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(5.0))
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(crate::typography::ui_rems(11.0))
                                    .text_color(theme.text_faint)
                                    .child(SharedString::from(stat.label.clone())),
                            )
                            .when_some(stat.delta.clone(), |el, delta| {
                                el.child(delta_pill(&delta, theme))
                            }),
                    )
            }))
            .into_any_element();
    }
    if let Some(table) = &preview.table {
        let mut rows = div()
            .size_full()
            .px(px(TILE_PADDING))
            .py(px(TILE_PADDING - 2.0))
            .flex()
            .flex_col()
            .justify_center();
        for (ix, row) in table.rows.iter().take(3).enumerate() {
            let first = row.first().cloned().unwrap_or_default();
            let last = if row.len() > 1 {
                row.last().cloned().unwrap_or_default()
            } else {
                String::new()
            };
            rows = rows.child(
                div()
                    .h(px(25.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .when(ix > 0, |el| {
                        el.border_t_1().border_color(crate::theme::hairline(0.06))
                    })
                    .text_size(crate::typography::ui_rems(11.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(theme.text_muted)
                            .child(SharedString::from(first)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_color(theme.text)
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .font_features(tabular())
                            .child(SharedString::from(last)),
                    ),
            );
        }
        return rows.into_any_element();
    }
    let text = preview.text.clone().unwrap_or_default();
    div()
        .size_full()
        .p(px(TILE_PADDING))
        .overflow_hidden()
        .text_size(crate::typography::ui_rems(11.5))
        .line_height(px(16.5))
        .text_color(theme.text_muted)
        .child(SharedString::from(text))
        .into_any_element()
}

fn only_graphic(preview: &ProjectWidgetPreview) -> bool {
    preview.graphic
        && preview.stats.is_empty()
        && preview.chart.is_none()
        && preview.table.is_none()
        && preview.text.is_none()
}

/// No preview yet, or a drawing a preview can't summarize.
fn placeholder(graphic: bool, theme: &Theme) -> AnyElement {
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .child(
            icon(if graphic {
                icons::GALLERY
            } else {
                icons::WIDGET
            })
            .size(px(20.0))
            .text_color(theme.text_faint),
        )
        .child(
            div()
                .text_size(crate::typography::ui_rems(11.0))
                .text_color(theme.text_faint)
                .child(if graphic {
                    "Interactive"
                } else {
                    "No preview yet"
                }),
        )
        .into_any_element()
}

/// A change note's color by its sign: up reads as good, down as bad.
fn delta_tone(delta: &str, theme: &Theme) -> Hsla {
    match delta.trim_start().chars().next() {
        Some('+') | Some('↑') => theme.success,
        Some('-') | Some('−') | Some('↓') => theme.danger_muted,
        _ => theme.text_muted,
    }
}

/// A change note as a tinted pill, so its sign reads at a glance.
fn delta_pill(delta: &str, theme: &Theme) -> gpui::Div {
    let tone = delta_tone(delta, theme);
    div()
        .flex_none()
        .px(px(5.0))
        .h(px(16.0))
        .flex()
        .items_center()
        .rounded(px(5.0))
        .bg(tone.opacity(0.12))
        .text_size(crate::typography::ui_rems(11.0))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(tone)
        .font_features(tabular())
        .child(SharedString::from(delta.to_owned()))
}

fn stat_line(stat: &zeron_proto::ProjectWidgetPreviewStat, theme: &Theme, size: f32) -> gpui::Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .min_w_0()
        .child(
            div()
                .flex_none()
                .text_size(crate::typography::ui_rems(size))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.text)
                .font_features(tabular())
                .child(SharedString::from(stat.value.clone())),
        )
        .when_some(stat.delta.clone(), |el, delta| {
            el.child(delta_pill(&delta, theme))
        })
}

/// How far in from a side edge the tile's rounded bottom corner is at
/// height `y`, so paths (which the rounded clip doesn't cut) stay inside.
fn corner_inset(y: f32, bottom: f32) -> f32 {
    let r = TILE_RADIUS;
    let into = y - (bottom - r);
    if into <= 0.0 {
        0.0
    } else {
        r - (r * r - into.min(r) * into.min(r)).sqrt()
    }
}

/// The chart's shape. Lines and areas bleed to the tile's edges: the first
/// series as a filled area with a dot on its latest value, the others as
/// quieter lines. Bars round their tops and the latest one is brightest.
/// Every series on the preview's shared scale.
fn chart_canvas(
    chart: ProjectWidgetPreviewChart,
    palette: [Hsla; 6],
    surface: Hsla,
    theme: &Theme,
) -> AnyElement {
    let light = theme.appearance.is_light();
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let left = f32::from(bounds.origin.x);
            let right = left + f32::from(bounds.size.width);
            let top = f32::from(bounds.origin.y) + 3.0;
            let bottom = f32::from(bounds.origin.y) + f32::from(bounds.size.height);
            let w = right - left;
            let len = chart
                .series
                .iter()
                .map(|s| s.values.len())
                .max()
                .unwrap_or(0);
            if len == 0 {
                return;
            }
            let scale = f32::from(PROJECT_WIDGET_PREVIEW_SCALE);
            match chart.kind {
                ProjectWidgetPreviewChartKind::Bar => {
                    let h = (bottom - top - 8.0).max(1.0);
                    let base = bottom - 8.0;
                    let band = w / len as f32;
                    let groups = if chart.stacked {
                        1
                    } else {
                        chart.series.len().max(1)
                    };
                    let bar = (band * 0.68 / groups as f32).max(1.0);
                    let radius = (bar / 2.5).min(3.0);
                    for ix in 0..len {
                        let latest = ix + 1 == len;
                        let mut stacked_top = base;
                        for (s, series) in chart.series.iter().enumerate() {
                            let Some(v) = series.values.get(ix).copied().flatten() else {
                                continue;
                            };
                            let height = (f32::from(v) / scale * h).max(2.0);
                            let (x, y0) = if chart.stacked {
                                stacked_top -= height;
                                (left + ix as f32 * band + band * 0.16, stacked_top)
                            } else {
                                (
                                    left + ix as f32 * band + band * 0.16 + s as f32 * bar,
                                    base - height,
                                )
                            };
                            let color = palette[s % palette.len()];
                            window.paint_quad(
                                gpui::fill(
                                    gpui::Bounds::new(
                                        point(px(x), px(y0)),
                                        gpui::size(px(bar), px(height)),
                                    ),
                                    if latest { color } else { color.opacity(0.55) },
                                )
                                .corner_radii(Corners {
                                    top_left: px(radius),
                                    top_right: px(radius),
                                    bottom_left: px(0.0),
                                    bottom_right: px(0.0),
                                }),
                            );
                        }
                    }
                }
                ProjectWidgetPreviewChartKind::Scatter => {
                    let h = (bottom - top - 10.0).max(1.0);
                    for (s, series) in chart.series.iter().enumerate() {
                        let n = series.values.len().max(2) as f32 - 1.0;
                        for (ix, v) in series.values.iter().enumerate() {
                            let Some(v) = v else { continue };
                            let x = left + 10.0 + ix as f32 / n * (w - 20.0);
                            let y = top + 4.0 + h - f32::from(*v) / scale * h;
                            window.paint_quad(
                                gpui::fill(
                                    gpui::Bounds::new(
                                        point(px(x - 2.0), px(y - 2.0)),
                                        gpui::size(px(4.0), px(4.0)),
                                    ),
                                    palette[s % palette.len()].opacity(0.8),
                                )
                                .corner_radii(px(2.0)),
                            );
                        }
                    }
                }
                _ => {
                    // Lines keep clear of the very bottom, areas fill to it.
                    let h = (bottom - top - 6.0).max(1.0);
                    let y = |v: u16| top + h - f32::from(v) / scale * h;
                    let inside = |x: f32, y: f32| {
                        let inset = corner_inset(y, bottom);
                        point(px(x.clamp(left + inset, right - inset)), px(y))
                    };
                    for (s, series) in chart.series.iter().enumerate().rev() {
                        let primary = s == 0;
                        let color = palette[s % palette.len()];
                        let n = series.values.len().max(2) as f32 - 1.0;
                        for run in runs(&series.values) {
                            let points: Vec<Point<Pixels>> = run
                                .iter()
                                .map(|(ix, v)| inside(left + *ix as f32 / n * w, y(*v)))
                                .collect();
                            if primary && points.len() > 1 {
                                let mut fill = points.clone();
                                let (last, first) = (points[points.len() - 1], points[0]);
                                // Down the right edge, along the bottom
                                // around its corners, and up the left.
                                for step in 0..=6 {
                                    let yy = f32::from(last.y)
                                        + (bottom - f32::from(last.y)) * step as f32 / 6.0;
                                    fill.push(inside(f32::from(last.x), yy));
                                }
                                for step in 0..=6 {
                                    let yy =
                                        bottom - (bottom - f32::from(first.y)) * step as f32 / 6.0;
                                    fill.push(inside(f32::from(first.x), yy));
                                }
                                let fill_alpha = if light { 0.22 } else { 0.30 };
                                fill_polygon(
                                    &fill,
                                    linear_gradient(
                                        180.0,
                                        linear_color_stop(color.opacity(fill_alpha), 0.0),
                                        linear_color_stop(color.opacity(0.0), 1.0),
                                    ),
                                    window,
                                );
                            }
                            let stroke = if primary { color } else { color.opacity(0.7) };
                            stroke_line(&points, if primary { 1.75 } else { 1.4 }, stroke, window);
                        }
                    }
                    // The latest value of the first series: where it is now.
                    if let Some(series) = chart.series.first() {
                        let n = series.values.len().max(2) as f32 - 1.0;
                        if let Some((ix, v)) = series
                            .values
                            .iter()
                            .enumerate()
                            .rev()
                            .find_map(|(ix, v)| v.map(|v| (ix, v)))
                        {
                            let p = inside((left + ix as f32 / n * w).min(right - 6.0), y(v));
                            let (cx, cy) = (f32::from(p.x), f32::from(p.y));
                            for (size, color) in [(10.0, surface), (6.0, palette[0])] {
                                window.paint_quad(
                                    gpui::fill(
                                        gpui::Bounds::new(
                                            point(px(cx - size / 2.0), px(cy - size / 2.0)),
                                            gpui::size(px(size), px(size)),
                                        ),
                                        color,
                                    )
                                    .corner_radii(px(size / 2.0)),
                                );
                            }
                        }
                    }
                }
            }
        },
    )
    .size_full()
    .into_any_element()
}

/// Consecutive non-gap values, with their indexes.
fn runs(values: &[Option<u16>]) -> Vec<Vec<(usize, u16)>> {
    let mut out: Vec<Vec<(usize, u16)>> = Vec::new();
    let mut current = Vec::new();
    for (ix, v) in values.iter().enumerate() {
        match v {
            Some(v) => current.push((ix, *v)),
            None if !current.is_empty() => out.push(std::mem::take(&mut current)),
            None => {}
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn stroke_line(points: &[Point<Pixels>], width: f32, color: Hsla, window: &mut Window) {
    let Some(first) = points.first() else {
        return;
    };
    if points.len() == 1 {
        window.paint_quad(
            gpui::fill(
                gpui::Bounds::new(
                    point(first.x - px(1.5), first.y - px(1.5)),
                    gpui::size(px(3.0), px(3.0)),
                ),
                color,
            )
            .corner_radii(px(1.5)),
        );
        return;
    }
    let mut builder = PathBuilder::stroke(px(width));
    builder.move_to(*first);
    for p in &points[1..] {
        builder.line_to(*p);
    }
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

fn fill_polygon(points: &[Point<Pixels>], color: impl Into<Background>, window: &mut Window) {
    let Some(first) = points.first() else {
        return;
    };
    let mut builder = PathBuilder::fill();
    builder.move_to(*first);
    for p in &points[1..] {
        builder.line_to(*p);
    }
    builder.close();
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaps_split_a_series_into_runs() {
        let values = [Some(1), None, Some(2), Some(3), None, None, Some(4)];
        let runs = runs(&values);
        assert_eq!(runs, vec![vec![(0, 1)], vec![(2, 2), (3, 3)], vec![(6, 4)]]);
    }
}
