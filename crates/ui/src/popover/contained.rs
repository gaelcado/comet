//! Dropdown placement within a dialog, measured after layout.

use gpui::{
    AnyElement, App, Bounds, Element, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    Pixels, Point, Position, SharedString, Size, Style, Window, div, point, prelude::*, px,
};
use std::time::Instant;

/// Mount from a relative trigger. The menu uses the available side of the
/// trigger instead of sliding over it when it approaches the dialog edge.
pub(crate) fn contained_menu(
    id: SharedString,
    content: AnyElement,
    closing: Option<Instant>,
    trigger_height: f32,
    limits: Bounds<Pixels>,
) -> AnyElement {
    let exit = closing.map(super::exit_progress);
    // Half the usable height guarantees room on at least one side, even
    // when the trigger sits in the middle of a small dialog.
    let max_height = ((f32::from(limits.size.height) - trigger_height) / 2.0 - 6.0)
        .max(1.0)
        .min(320.0);
    let scroller = div()
        .id(SharedString::from(format!("{id}-scroll")))
        .debug_selector(|| "contained-menu-scroll".into())
        .max_h(px(max_height))
        .max_w(limits.size.width)
        .overflow_y_scroll()
        .rounded(px(super::CARD_RADIUS))
        .child(content)
        .into_any_element();
    let content = super::frosted_menu(exit, scroller);
    let content = super::menu_motion(id, exit, div().occlude().child(content));
    div()
        .absolute()
        .bottom_0()
        .right_0()
        .size_0()
        .child(
            gpui::deferred(ContainedMenu {
                content,
                limits,
                trigger_height,
            })
            .priority(1),
        )
        .into_any_element()
}

fn menu_origin(
    anchor: Point<Pixels>,
    size: Size<Pixels>,
    trigger_height: Pixels,
    limits: Bounds<Pixels>,
) -> Point<Pixels> {
    let gap = px(6.0);
    let below = anchor.y + gap;
    let above = anchor.y - trigger_height - gap - size.height;
    let y = if below + size.height <= limits.bottom() {
        below
    } else {
        above
    };
    point(
        (anchor.x - size.width).clamp(
            limits.left(),
            (limits.right() - size.width).max(limits.left()),
        ),
        y.clamp(
            limits.top(),
            (limits.bottom() - size.height).max(limits.top()),
        ),
    )
}

struct ContainedMenu {
    content: AnyElement,
    limits: Bounds<Pixels>,
    trigger_height: f32,
}

impl IntoElement for ContainedMenu {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for ContainedMenu {
    type RequestLayoutState = LayoutId;
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
    ) -> (LayoutId, LayoutId) {
        let child = self.content.request_layout(window, cx);
        let layout = window.request_layout(
            Style {
                position: Position::Absolute,
                ..Style::default()
            },
            [child],
            cx,
        );
        (layout, child)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        child: &mut LayoutId,
        window: &mut Window,
        cx: &mut App,
    ) {
        let size = window.layout_bounds(*child).size;
        let origin = menu_origin(bounds.origin, size, px(self.trigger_height), self.limits);
        window.with_element_offset(origin - bounds.origin, |window| {
            self.content.prepaint(window, cx)
        });
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut LayoutId,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.content.paint(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::size;

    #[test]
    fn dropdown_stays_in_dialog_without_covering_trigger() {
        let limits = Bounds::new(point(px(100.0), px(80.0)), size(px(800.0), px(600.0)));
        let menu = size(px(260.0), px(280.0));
        for y in [120.0, 350.0, 650.0] {
            let anchor = point(px(870.0), px(y));
            let origin = menu_origin(anchor, menu, px(34.0), limits);
            let bounds = Bounds::new(origin, menu);
            assert!(bounds.left() >= limits.left() && bounds.right() <= limits.right());
            assert!(bounds.top() >= limits.top() && bounds.bottom() <= limits.bottom());
            assert!(bounds.top() >= anchor.y + px(6.0) || bounds.bottom() <= anchor.y - px(40.0));
        }
    }

    struct MenuFixture;

    impl gpui::Render for MenuFixture {
        fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
            let theme = crate::theme::Theme::of(cx);
            div().size_full().child(
                div()
                    .absolute()
                    .left(px(300.0))
                    .top(px(300.0))
                    .w(px(200.0))
                    .h(px(34.0))
                    .id("test-menu-trigger")
                    .debug_selector(|| "test-menu-trigger".into())
                    .child(contained_menu(
                        "test-contained-menu".into(),
                        super::super::popover_card(theme)
                            .w(px(260.0))
                            .flex()
                            .flex_col()
                            .children((0..30).map(|_| div().h(px(32.0)).flex_none()))
                            .into_any_element(),
                        None,
                        34.0,
                        Bounds::new(point(px(100.0), px(80.0)), size(px(600.0), px(400.0))),
                    )),
            )
        }
    }

    #[gpui::test]
    fn long_menu_scrolls_and_flips_above_trigger(cx: &mut gpui::TestAppContext) {
        use gpui::AppContext;
        cx.update(|cx| {
            gpui_base::init(cx);
            cx.set_global(crate::theme::Theme::default());
        });
        let (_, cx) = cx.add_window_view(|_, _| MenuFixture);
        cx.update(|window, cx| window.draw(cx).clear());
        let trigger = cx.debug_bounds("test-menu-trigger").unwrap();
        let menu = cx.debug_bounds("contained-menu-scroll").unwrap();
        assert!(menu.size.height <= px(177.0));
        assert!(menu.top() >= px(80.0));
        assert!(menu.bottom() <= trigger.top() - px(6.0));
        assert!(menu.right() <= px(700.0));
        assert!(menu.left() >= px(100.0));
    }
}
