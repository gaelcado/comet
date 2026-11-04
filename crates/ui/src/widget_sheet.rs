//! The widget sheet's motion: it grows out of the card that opened it and
//! settles back into it on close. Opened from elsewhere (a chat's "View
//! widget") it rises in place instead.
//!
//! The page inside keeps its final width the whole time, so nothing reflows
//! mid-flight: the sheet's frame (material, edge, rounding) morphs from the
//! card's rectangle to its own while the content, anchored to its top-left
//! corner, unfolds with it and fades in once there is room. Closing is quicker and softer than opening.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{AnyElement, Bounds, Pixels, div, prelude::*, px};

use crate::motion;
use crate::popover;
use crate::theme::Theme;

const OPEN: Duration = Duration::from_millis(360);
const CLOSE: Duration = Duration::from_millis(220);
/// Opened without a card, the sheet rises from this scale.
const RISE_SCALE: f32 = 0.96;
/// The card's rounding, which the sheet starts from.
const CARD_RADIUS: f32 = 18.0;
/// The sheet's own, as the command palette's.
const SHEET_RADIUS: f32 = 16.0;

pub struct SheetMotion {
    origin: Option<Bounds<Pixels>>,
    opened: Instant,
    /// When closing started, how open it was, and where it lands.
    closing: Option<(Instant, f32, Option<Bounds<Pixels>>)>,
    /// The page's laid-out height, from its last paint.
    height: Rc<Cell<Option<f32>>>,
    /// Fixtures freeze time to capture the morph mid-flight.
    #[cfg(feature = "project-widgets-fixture")]
    frozen_at: Option<Instant>,
}

impl SheetMotion {
    pub fn new(origin: Option<Bounds<Pixels>>) -> Self {
        Self {
            origin,
            opened: Instant::now(),
            closing: None,
            height: Rc::default(),
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

    /// Open from `origin`, frozen `progress` (0..1, linear time) of the way.
    #[cfg(feature = "project-widgets-fixture")]
    pub fn fixture_frozen(origin: Option<Bounds<Pixels>>, progress: f32) -> Self {
        let now = Instant::now();
        Self {
            opened: now - OPEN.mul_f32(progress.clamp(0.0, 1.0)),
            frozen_at: Some(now),
            ..Self::new(origin)
        }
    }

    /// Start closing toward `target` (the card, when it is on screen).
    pub fn close(&mut self, target: Option<Bounds<Pixels>>, reduced: bool) {
        if self.closing.is_none() {
            let now = Instant::now();
            let open = if reduced {
                0.0
            } else {
                self.openness(now, false)
            };
            self.closing = Some((now, open, target));
        }
    }

    pub fn is_closing(&self) -> bool {
        self.closing.is_some()
    }

    /// Closed all the way: the sheet can go.
    pub fn finished(&self, reduced: bool) -> bool {
        self.closing.is_some_and(|(at, open, _)| {
            reduced || open <= 0.0 || at.elapsed() >= CLOSE.mul_f32(open.max(0.01))
        })
    }

    /// 0 at the card (or risen from below), 1 fully open.
    fn openness(&self, now: Instant, reduced: bool) -> f32 {
        if reduced {
            return if self.closing.is_some() { 0.0 } else { 1.0 };
        }
        match self.closing {
            Some((at, open, _)) => {
                let elapsed = now.saturating_duration_since(at).as_secs_f32();
                // Close from wherever it was, at the closing pace.
                let span = (CLOSE.as_secs_f32() * open).max(0.001);
                open * (1.0 - motion::EASE_OUT_SOFT.eval(elapsed / span))
            }
            None => {
                let elapsed = now.saturating_duration_since(self.opened).as_secs_f32();
                motion::EASE_OUT_SOFT.eval(elapsed / OPEN.as_secs_f32())
            }
        }
    }

    pub fn animating(&self, reduced: bool) -> bool {
        let p = self.openness(self.now(), reduced);
        if self.closing.is_some() {
            p > 0.0
        } else {
            p < 1.0
        }
    }

    /// The overlay: scrim, then the sheet's frame around `page`.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &self,
        viewport: gpui::Size<Pixels>,
        width: f32,
        max_height: f32,
        theme: &Theme,
        reduced: bool,
        page: AnyElement,
        frame: impl FnOnce(gpui::Stateful<gpui::Div>) -> gpui::Stateful<gpui::Div>,
    ) -> AnyElement {
        let p = self.openness(self.now(), reduced);
        let (vw, vh) = (f32::from(viewport.width), f32::from(viewport.height));
        let height = self.height.get().unwrap_or(max_height).min(max_height);
        let target = Rect {
            x: (vw - width) / 2.0,
            y: (vh - height) / 2.0,
            w: width,
            h: height,
        };
        // Where the sheet comes from (and goes back to).
        let anchor = match self.closing {
            Some((_, _, landing)) => landing,
            None => self.origin,
        };
        let from = anchor
            .map(Rect::of)
            .unwrap_or_else(|| target.scaled(RISE_SCALE));
        let rect = from.lerp(target, p);
        let morphing = anchor.is_some();
        // From a card, the content waits for room; risen, it fades whole.
        let content_alpha = if morphing {
            crate::composer_dock::stage(p, 0.7, 1.0)
        } else {
            1.0
        };
        let frame_alpha = if morphing { 1.0 } else { p };
        let radius = if morphing {
            motion::lerp(CARD_RADIUS, SHEET_RADIUS, p)
        } else {
            SHEET_RADIUS
        };
        // The card's material warms into the sheet's.
        let fill = if morphing {
            motion::mix(theme.composer_surface_bg(), popover::surface_bg(theme), p)
        } else {
            popover::surface_bg(theme)
        };
        let edge = if morphing {
            motion::mix(theme.composer_surface_border(), theme.border, p)
        } else {
            theme.border
        };
        let measured = self.height.clone();
        let content = div()
            .absolute()
            // Anchored to the frame's leading top corner: the sheet unfolds
            // from the card, its title first, never a cropped middle.
            .left_0()
            .top_0()
            .w(px(target.w))
            .opacity(content_alpha)
            .child(page)
            .child(
                gpui::canvas(
                    move |bounds, _, _| measured.set(Some(f32::from(bounds.size.height))),
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            );
        let sheet = frame(
            div()
                .id("widget-sheet")
                .role(gpui::Role::Dialog)
                .aria_label("Widget")
                .relative()
                .w(px(rect.w))
                .h(px(rect.h))
                .rounded(px(radius))
                .border_1()
                .border_color(edge)
                .bg(fill)
                .when(!theme.is_frost(), |el| el.shadow_lg())
                .overflow_hidden(),
        )
        .child(content);
        let sheet = crate::rounded_clip::rounded_clip(
            radius,
            crate::frost::frosted(radius, crate::frost::MENU_BLUR, sheet),
        );
        gpui::deferred(
            gpui::anchored()
                .position(gpui::point(px(0.0), px(0.0)))
                .child(
                    div()
                        .occlude()
                        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                        .w(viewport.width)
                        .h(viewport.height)
                        .bg(popover::scrim_alpha(0.35 * p))
                        .child(
                            div()
                                .absolute()
                                .left(px(rect.x))
                                .top(px(rect.y))
                                .opacity(frame_alpha)
                                .child(sheet),
                        ),
                ),
        )
        .priority(2)
        .into_any_element()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Rect {
    fn of(bounds: Bounds<Pixels>) -> Self {
        Self {
            x: f32::from(bounds.origin.x),
            y: f32::from(bounds.origin.y),
            w: f32::from(bounds.size.width),
            h: f32::from(bounds.size.height),
        }
    }

    fn scaled(self, scale: f32) -> Self {
        let (w, h) = (self.w * scale, self.h * scale);
        Self {
            x: self.x + (self.w - w) / 2.0,
            y: self.y + (self.h - h) / 2.0,
            w,
            h,
        }
    }

    fn lerp(self, to: Self, t: f32) -> Self {
        Self {
            x: motion::lerp(self.x, to.x, t),
            y: motion::lerp(self.y, to.y, t),
            w: motion::lerp(self.w, to.w, t),
            h: motion::lerp(self.h, to.h, t),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sheet_closes_from_wherever_it_was() {
        let mut motion = SheetMotion::new(None);
        // Interrupted a third of the way open, it closes from there, at the
        // closing pace, and is done once fully closed.
        motion.opened = Instant::now() - OPEN / 3;
        let open = motion.openness(Instant::now(), false);
        assert!(open > 0.0 && open < 1.0);
        motion.close(None, false);
        let (at, from, _) = motion.closing.unwrap();
        assert!((from - open).abs() < 0.05);
        assert!(motion.openness(at, false) <= from);
        assert_eq!(motion.openness(at + CLOSE, false), 0.0);
        assert!(!motion.finished(false) || at.elapsed() >= CLOSE.mul_f32(from));
        // Reduced motion: open at once, closed at once.
        assert_eq!(SheetMotion::new(None).openness(Instant::now(), true), 1.0);
    }
}
