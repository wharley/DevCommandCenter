//! Radix-style tooltip (`components/ui/tooltip.tsx`): shown next to its
//! trigger on one side, after a delay (none by default), hidden on press.
//! GPUI's own tooltip follows the mouse after a fixed delay, so this one
//! keeps its open state per element instead. Like Radix's `avoidCollisions`,
//! a bubble that would leave the window on its side opens on the other one
//! (the header's labels go under their icons instead of over them).

use std::rc::Rc;
use std::time::Duration;

use gpui::{
    Anchor, AnyElement, App, Bounds, BoxShadow, ElementId, Hsla, Pixels, Size, Task, Window,
    anchored, canvas, deferred, div, point, prelude::*, px, relative, size,
};

use crate::theme::ActiveTheme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    /// Over or under: the bubble's right edge at the trigger's.
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    /// `bg-foreground text-background`, the default label.
    Label,
    /// A popover card (`bg-popover`, `rounded-xl`, `p-3`), like the task card.
    Card,
}

type Content = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

#[derive(Default)]
struct TooltipState {
    visible: bool,
    pending: Option<Task<()>>,
    /// Where the trigger was last painted, in window coordinates.
    trigger: Option<Bounds<Pixels>>,
    /// The bubble's size once painted; a label's height is known before.
    bubble: Option<Size<Pixels>>,
}

/// The space kept between a bubble and the window's edge.
const EDGE: f32 = 12.0;
/// A one-line label: `py-1.5` around a 16px line.
const LABEL_HEIGHT: f32 = 28.0;

/// The side the bubble opens on: the one asked for, unless the bubble would
/// leave the window there and fits on the opposite side.
fn fitting_side(
    side: Side,
    trigger: Bounds<Pixels>,
    bubble: Size<Pixels>,
    offset: f32,
    window: Size<Pixels>,
) -> Side {
    let (offset, edge) = (px(offset), px(EDGE));
    let above = trigger.top() - offset - bubble.height >= edge;
    let below = trigger.bottom() + offset + bubble.height <= window.height - edge;
    let before = trigger.left() - offset - bubble.width >= edge;
    let after = trigger.right() + offset + bubble.width <= window.width - edge;
    match side {
        Side::Top if !above && below => Side::Bottom,
        Side::Bottom if !below && above => Side::Top,
        Side::Left if !before && after => Side::Right,
        Side::Right if !after && before => Side::Left,
        side => side,
    }
}

#[derive(IntoElement)]
pub struct Tooltip {
    id: ElementId,
    trigger: AnyElement,
    content: Content,
    side: Side,
    align: Align,
    offset: f32,
    delay: Duration,
    surface: Surface,
    enabled: bool,
    /// The trigger takes the free width of a flex row (`min-w-0 flex-1`).
    fill: bool,
}

impl Tooltip {
    pub fn new(
        id: impl Into<ElementId>,
        trigger: impl IntoElement,
        content: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            trigger: trigger.into_any_element(),
            content: Rc::new(content),
            side: Side::Top,
            align: Align::Center,
            offset: 4.0,
            delay: Duration::ZERO,
            surface: Surface::Label,
            enabled: true,
            fill: false,
        }
    }

    /// A plain text label.
    pub fn text(
        id: impl Into<ElementId>,
        trigger: impl IntoElement,
        text: impl Into<gpui::SharedString>,
    ) -> Self {
        let text = text.into();
        Self::new(id, trigger, move |_, _| text.clone().into_any_element())
    }

    pub fn side(mut self, side: Side) -> Self {
        self.side = side;
        self
    }

    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    /// `sideOffset`
    pub fn offset(mut self, offset: f32) -> Self {
        self.offset = offset;
        self
    }

    /// `delayDuration`
    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    pub fn surface(mut self, surface: Surface) -> Self {
        self.surface = surface;
        self
    }

    /// Off while something else owns the spot (a rename in progress).
    pub fn fill(mut self) -> Self {
        self.fill = true;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

fn ring(color: Hsla) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(px(0.0), px(0.0)),
        blur_radius: px(0.0),
        spread_radius: px(1.0),
        inset: false,
    }
}

fn shadow_xl() -> [BoxShadow; 2] {
    let shadow = |y: f32, blur: f32, spread: f32| BoxShadow {
        color: Hsla::black().opacity(0.1),
        offset: point(px(0.0), px(y)),
        blur_radius: px(blur),
        spread_radius: px(spread),
        inset: false,
    };
    [shadow(20.0, 25.0, -5.0), shadow(8.0, 10.0, -6.0)]
}

impl RenderOnce for Tooltip {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| TooltipState::default());
        let visible = self.enabled && state.read(cx).visible;
        let delay = self.delay;
        let hover_state = state.clone();
        let press_state = state.clone();
        let trigger_state = state.clone();

        let bubble = visible.then(|| {
            let content = (self.content)(window, cx);
            let theme = cx.theme();
            let base = div().flex().items_center().gap(px(6.0));
            let bubble = match self.surface {
                Surface::Label => base
                    .max_w(px(320.0))
                    .px(px(12.0))
                    .py(px(6.0))
                    .rounded(theme.radius.md)
                    .bg(theme.colors.foreground)
                    .text_color(theme.colors.background)
                    .text_size(px(12.0))
                    .line_height(px(16.0)),
                Surface::Card => {
                    let mut shadows = shadow_xl().to_vec();
                    shadows.push(ring(theme.colors.border.opacity(0.8)));
                    base.p(px(12.0))
                        .rounded(theme.radius.xl)
                        .bg(theme.colors.popover)
                        .text_color(theme.colors.popover_foreground)
                        .shadow(shadows)
                }
            };
            let offset = self.offset;
            let measured = state.read(cx).bubble.unwrap_or(match self.surface {
                Surface::Label => size(px(0.0), px(LABEL_HEIGHT)),
                Surface::Card => Size::default(),
            });
            let side = match state.read(cx).trigger {
                Some(trigger) => {
                    fitting_side(self.side, trigger, measured, offset, window.viewport_size())
                }
                None => self.side,
            };
            let measure_state = state.clone();
            let bubble = bubble.relative().child(
                canvas(
                    move |bounds, _, cx| {
                        measure_state.update(cx, |state, cx| {
                            if state.bubble != Some(bounds.size) {
                                state.bubble = Some(bounds.size);
                                cx.notify();
                            }
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
            let (spot, anchor, shift) = match (side, self.align) {
                (Side::Top, Align::End) => (
                    div().top_0().right_0(),
                    Anchor::BottomRight,
                    point(px(0.0), px(-offset)),
                ),
                (Side::Bottom, Align::End) => (
                    div().bottom_0().right_0(),
                    Anchor::TopRight,
                    point(px(0.0), px(offset)),
                ),
                (Side::Top, _) => (
                    div().top_0().left(relative(0.5)),
                    Anchor::BottomCenter,
                    point(px(0.0), px(-offset)),
                ),
                (Side::Bottom, _) => (
                    div().bottom_0().left(relative(0.5)),
                    Anchor::TopCenter,
                    point(px(0.0), px(offset)),
                ),
                (Side::Right, Align::Center | Align::End) => (
                    div().right_0().top(relative(0.5)),
                    Anchor::LeftCenter,
                    point(px(offset), px(0.0)),
                ),
                (Side::Right, Align::Start) => (
                    div().right_0().top_0(),
                    Anchor::TopLeft,
                    point(px(offset), px(0.0)),
                ),
                (Side::Left, Align::Center | Align::End) => (
                    div().left_0().top(relative(0.5)),
                    Anchor::RightCenter,
                    point(px(-offset), px(0.0)),
                ),
                (Side::Left, Align::Start) => (
                    div().left_0().top_0(),
                    Anchor::TopRight,
                    point(px(-offset), px(0.0)),
                ),
            };
            spot.absolute().size_0().child(
                deferred(
                    anchored()
                        .anchor(anchor)
                        .offset(shift)
                        .snap_to_window_with_margin(px(12.0))
                        .child(bubble.child(content)),
                )
                .priority(3),
            )
        });

        div()
            .id(self.id)
            .relative()
            .when(self.fill, |wrapper| wrapper.flex_1().min_w_0())
            .on_hover(move |hovered, _, cx| {
                let hovered = *hovered;
                hover_state.update(cx, |state, cx| {
                    state.pending = None;
                    if !hovered || delay.is_zero() {
                        state.visible = hovered;
                        cx.notify();
                        return;
                    }
                    state.pending = Some(cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(delay).await;
                        let _ = this.update(cx, |state, cx| {
                            state.visible = true;
                            cx.notify();
                        });
                    }));
                });
            })
            .on_any_mouse_down(move |_, _, cx| {
                press_state.update(cx, |state, cx| {
                    state.pending = None;
                    state.visible = false;
                    cx.notify();
                });
            })
            .child(self.trigger)
            .child(
                canvas(
                    move |bounds, _, cx| {
                        trigger_state.update(cx, |state, _| state.trigger = Some(bounds));
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .children(bubble)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f32, y: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(28.0), px(28.0)))
    }

    fn window() -> Size<Pixels> {
        size(px(1400.0), px(900.0))
    }

    #[test]
    fn a_header_label_opens_under_its_icon() {
        let label = size(px(140.0), px(LABEL_HEIGHT));
        assert_eq!(
            fitting_side(Side::Top, at(900.0, 10.0), label, 4.0, window()),
            Side::Bottom
        );
        assert_eq!(
            fitting_side(Side::Top, at(900.0, 400.0), label, 4.0, window()),
            Side::Top
        );
    }

    #[test]
    fn each_side_flips_only_when_the_other_one_fits() {
        let label = size(px(140.0), px(LABEL_HEIGHT));
        assert_eq!(
            fitting_side(Side::Bottom, at(400.0, 860.0), label, 4.0, window()),
            Side::Top
        );
        assert_eq!(
            fitting_side(Side::Right, at(1360.0, 400.0), label, 4.0, window()),
            Side::Left
        );
        assert_eq!(
            fitting_side(Side::Left, at(10.0, 400.0), label, 4.0, window()),
            Side::Right
        );
        let tall = size(px(140.0), px(2000.0));
        assert_eq!(
            fitting_side(Side::Top, at(400.0, 10.0), tall, 4.0, window()),
            Side::Top
        );
    }
}
