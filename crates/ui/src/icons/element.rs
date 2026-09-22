use super::transition::State;
use gpui::{
    App, Bounds, Element, ElementId, GlobalElementId, Hitbox, InspectorElementId,
    InteractiveElement, Interactivity, IntoElement, LayoutId, Pixels, SharedString,
    StyleRefinement, Styled, Svg, Transformation, Window, px, svg,
};
use std::time::Instant;

/// A native SVG with opt-in, element-local state transitions.
/// The enclosing control must have a stable ID; the glyph ID is scoped beneath it.
pub struct Icon {
    svg: Svg,
    path: &'static str,
    animate: bool,
}
impl Icon {
    pub(super) fn new(path: &'static str) -> Self {
        Self {
            svg: svg().path(path).flex_none(),
            path,
            animate: false,
        }
    }
    pub fn morph(mut self, id: impl Into<ElementId>) -> Self {
        if super::generated::MORPH_PATHS.contains(&self.path) {
            self.animate = true;
            self.svg.interactivity().element_id = Some(id.into());
        }
        self
    }
    pub fn with_transformation(mut self, transformation: Transformation) -> Self {
        self.svg = self.svg.with_transformation(transformation);
        self
    }
}
impl IntoElement for Icon {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Styled for Icon {
    fn style(&mut self) -> &mut StyleRefinement {
        self.svg.style()
    }
}
impl InteractiveElement for Icon {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.svg.interactivity()
    }
}
impl Element for Icon {
    type RequestLayoutState = ();
    type PrepaintState = Option<Hitbox>;
    fn id(&self) -> Option<ElementId> {
        Element::id(&self.svg)
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.svg.source_location()
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        self.svg.request_layout(id, inspector, window, cx)
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Hitbox> {
        self.svg.prepaint(id, inspector, bounds, layout, window, cx)
    }
    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut (),
        hitbox: &mut Option<Hitbox>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let mut path: SharedString = self.path.into();
        if self.animate && super::generated::MORPH_PATHS.contains(&self.path) {
            if let Some(id) = id {
                let now = Instant::now();
                let reduced = crate::motion::reduced_motion(cx);
                let frame = window.with_element_state(id, |state: Option<State>, _| {
                    let mut state = state.unwrap_or_else(|| State::new(self.path, now));
                    let frame = state.update(self.path, reduced, now);
                    (frame, state)
                });
                if let Some(frame) = frame {
                    path = frame.into();
                    window.request_animation_frame();
                }
            }
        } else if bounds.size.width <= px(16.) && bounds.size.height <= px(16.) {
            if let Some(name) = self.path.strip_prefix("custom-icons/") {
                path = format!("custom-icons/small/{name}").into();
            }
        }
        // Stateful icons retain the standard master at rest, avoiding an optical
        // geometry jump at either end. Static icons select optical masters above.
        self.svg = std::mem::replace(&mut self.svg, svg()).path(path);
        self.svg
            .paint(id, inspector, bounds, layout, hitbox, window, cx);
    }
}
