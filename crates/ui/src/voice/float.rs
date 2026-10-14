//! Where the call's bar stands outside its call screen. It is one bar,
//! always in reach: above the composer in a conversation, and in the top
//! trailing corner of every screen that has no composer to sit above (the
//! new-session screen, Settings). As the screen changes it passes from one
//! to the other without crossing the screen: it leaves the place it stands
//! at and arrives at the other ([`passage`]), so a call being run from
//! several chats keeps its controls in reach throughout.
//!
//! The composer keeps room for the bar with a [`slot`] and the shell paints
//! the bar through [`Floating`], which reads that room at prepaint: the bar
//! follows the composer on the very frame it moves.
use std::{cell::Cell, rc::Rc};

use gpui::{
    AnyElement, App, AvailableSpace, Bounds, Element, GlobalElementId, InspectorElementId,
    InteractiveElement, IntoElement, LayoutId, ParentElement, Pixels, Styled, Window, div, point,
    px, size,
};

use crate::theme::Theme;

/// The room a composer keeps for the bar, in window coordinates.
pub(crate) type Slot = Rc<Cell<Option<Bounds<Pixels>>>>;

/// Room for the bar in a composer's column: `height` of it, of which
/// `amount` (0–1) is open. It cancels the column's gap while closed and
/// records where it is for [`Floating`].
pub(crate) fn slot(slot: Slot, height: f32, amount: f32) -> gpui::Div {
    div()
        .debug_selector(|| "voice-dock-slot".into())
        .relative()
        .w_full()
        .h(px(height * amount))
        .mb(px(-Theme::SPACE_SM * (1.0 - amount)))
        .child(
            gpui::canvas(move |bounds, _, _| slot.set(Some(bounds)), |_, _, _, _| {})
                .absolute()
                .inset_0(),
        )
}

/// How far the bar stands from the top and the trailing edge of a screen
/// that has no composer to dock it above.
pub(crate) const CORNER_INSET: f32 = 14.0;
/// How far the bar moves as it leaves a place or arrives at one.
const SETTLE: f32 = 8.0;
/// Space between the docked bar and what stands beside it.
const BESIDE_GAP: f32 = 8.0;

/// Where something `width` wide stands beside the docked bar, on its
/// trailing side and on its line: its distance from the leading edge of a
/// composer `composer` wide, which the bar is centred on. `None` when the
/// composer has no room for it there.
pub(crate) fn beside(bar: f32, composer: f32, width: f32) -> Option<f32> {
    let leading = composer / 2.0 + bar / 2.0 + BESIDE_GAP;
    (leading + width <= composer).then_some(leading)
}

/// The bar, painted over the page at its place for this frame.
pub(crate) struct Floating {
    child: AnyElement,
    slot: Slot,
    /// Where the bar was put, for what stands beside it.
    placed: Slot,
    /// The screen's area beside the sidebar: its left edge and its width.
    area: (f32, f32),
    /// 0 in the top corner, 1 in the composer's slot.
    amount: f32,
}

/// The bar's passage between its two places at `amount` (0 in the top
/// corner, 1 in the composer's room): which place it stands at (`true` for
/// the composer's), and how much of it is there. Over the first half it
/// leaves the place it was at, over the second it arrives at the other; in
/// between it is nowhere, rather than half-way across the screen.
pub(crate) fn passage(amount: f32) -> (bool, f32) {
    let amount = amount.clamp(0.0, 1.0);
    let there = (2.0 * amount - 1.0).abs();
    (amount >= 0.5, there * there * (3.0 - 2.0 * there))
}

pub(crate) fn floating(
    child: impl IntoElement,
    slot: Slot,
    placed: Slot,
    area: (f32, f32),
    amount: f32,
) -> Floating {
    Floating {
        child: div()
            .opacity(passage(amount).1)
            .child(child)
            .into_any_element(),
        slot,
        placed,
        area,
        amount,
    }
}

/// Where a bar of `bar` size stands at `amount` of its [`passage`]: in the
/// top trailing corner of `area`, or centred on the bottom of `slot`, a few
/// points short of either while it is leaving or arriving. Without a slot it
/// stays at the top.
pub(crate) fn place(
    bar: gpui::Size<Pixels>,
    area: (f32, f32),
    slot: Option<Bounds<Pixels>>,
    amount: f32,
) -> gpui::Point<Pixels> {
    // In the top trailing corner, as far from one edge as from the other:
    // centred on the title bar's line it had no air above it.
    let top = point(
        px(area.0 + area.1 - CORNER_INSET) - bar.width,
        px(CORNER_INSET),
    );
    let (docked, there) = passage(amount);
    let away = px(SETTLE * (1.0 - there));
    match slot.filter(|_| docked) {
        // It rises into the composer's room and drops into the corner.
        Some(slot) => point(
            slot.center().x - bar.width / 2.0,
            slot.bottom() - bar.height + away,
        ),
        None => point(top.x, top.y - away),
    }
}

impl Element for Floating {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        // Out of the page's flow: the bar is placed at prepaint.
        let anchor = div()
            .absolute()
            .size_0()
            .into_any_element()
            .request_layout(window, cx);
        (anchor, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let bar = self.child.layout_as_root(
            size(AvailableSpace::MaxContent, AvailableSpace::MaxContent),
            window,
            cx,
        );
        let origin = place(bar, self.area, self.slot.get(), self.amount);
        self.placed.set(Some(Bounds::new(origin, bar)));
        self.child.prepaint_at(origin, window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}

impl IntoElement for Floating {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}
