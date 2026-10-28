//! Radix-style tooltip (`components/ui/tooltip.tsx`): shown next to its
//! trigger on one side, after a delay (none by default), hidden on press.
//! GPUI's own tooltip follows the mouse after a fixed delay, so this one
//! keeps its open state per element instead.

use std::rc::Rc;
use std::time::Duration;

use gpui::{
    Anchor, AnyElement, App, BoxShadow, ElementId, Hsla, Task, Window, anchored, deferred, div,
    point, prelude::*, px, relative,
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
            let (spot, anchor, shift) = match (self.side, self.align) {
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
            .children(bubble)
    }
}
