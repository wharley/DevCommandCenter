//! Loops drawn on the shared clock (`pulse`): an element that builds its
//! frame from the clock's phase, and the spinner on it.

use std::time::Duration;

use gpui::{
    AnyElement, App, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, RenderOnce, Transformation, Window, div, percentage, prelude::*, px,
};

use super::pulse::{Rate, phase, request};

/// A repeating animation on the shared clock: `build` gets the fraction of
/// `period` elapsed and returns the frame. Resting (phase zero) under
/// reduced motion or with DCC in the back.
pub fn pulse_loop<E: IntoElement>(
    id: impl Into<ElementId>,
    period: Duration,
    rate: Rate,
    build: impl FnOnce(f32) -> E + 'static,
) -> Looping {
    Looping {
        id: id.into(),
        period,
        rate,
        build: Some(Box::new(move |phase| build(phase).into_any_element())),
    }
}

pub struct Looping {
    id: ElementId,
    period: Duration,
    rate: Rate,
    build: Option<Box<dyn FnOnce(f32) -> AnyElement>>,
}

impl IntoElement for Looping {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Looping {
    type RequestLayoutState = AnyElement;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
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
    ) -> (LayoutId, AnyElement) {
        // Leased while laid out: a loop not drawn (scrolled away, closed)
        // stops costing from the clock's next tick.
        let at = if request(self.rate, window, cx) {
            phase(self.period, cx)
        } else {
            0.0
        };
        let build = self.build.take().expect("a loop lays out once");
        let mut element = build(at);
        (element.request_layout(window, cx), element)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: gpui::Bounds<Pixels>,
        element: &mut AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) {
        element.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: gpui::Bounds<Pixels>,
        element: &mut AnyElement,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        element.paint(window, cx);
    }
}

/// `spinner` on the shared clock: `Loader2` turning once a second in the
/// text color around it.
pub fn pulse_spinner(id: impl Into<ElementId>, size: f32) -> PulseSpinner {
    PulseSpinner {
        id: id.into(),
        size,
    }
}

#[derive(IntoElement)]
pub struct PulseSpinner {
    id: ElementId,
    size: f32,
}

impl RenderOnce for PulseSpinner {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        // An svg is drawn in its own text color, not inherited (see `Icon`).
        let color = window.text_style().color;
        let size = self.size;
        div().flex_shrink_0().child(pulse_loop(
            self.id,
            Duration::from_secs(1),
            Rate::Half,
            move |turn| {
                gpui::svg()
                    .path(crate::assets::icon_path("loader-circle", 2.0))
                    .size(px(size))
                    .text_color(color)
                    .with_transformation(Transformation::rotate(percentage(turn)))
            },
        ))
    }
}
