//! shadcn's Popover (`components/ui/popover.tsx`): a card under its trigger.
//! The owner keeps whether it is open; a click on the trigger, a click
//! outside or Esc ask for the change (`on_open_change`).

use std::rc::Rc;

use gpui::{
    Anchor, AnyElement, App, BoxShadow, ClickEvent, ElementId, Hsla, KeyDownEvent, Window,
    anchored, deferred, div, point, prelude::*, px,
};

use crate::theme::ActiveTheme;

type Content = Box<dyn FnOnce(&mut Window, &mut App) -> AnyElement>;
type OnOpenChange = Rc<dyn Fn(bool, &mut Window, &mut App)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopoverAlign {
    Start,
    End,
}

#[derive(IntoElement)]
pub struct Popover {
    id: ElementId,
    open: bool,
    trigger: AnyElement,
    content: Content,
    align: PopoverAlign,
    side_offset: f32,
    /// `w-72` by default.
    width: f32,
    /// `p-2.5` by default; a command list sets `p-0`.
    padding: f32,
    on_open_change: OnOpenChange,
}

impl Popover {
    pub fn new(
        id: impl Into<ElementId>,
        open: bool,
        trigger: impl IntoElement,
        content: impl FnOnce(&mut Window, &mut App) -> AnyElement + 'static,
        on_open_change: impl Fn(bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            open,
            trigger: trigger.into_any_element(),
            content: Box::new(content),
            align: PopoverAlign::Start,
            side_offset: 4.0,
            width: 288.0,
            padding: 10.0,
            on_open_change: Rc::new(on_open_change),
        }
    }

    pub fn align(mut self, align: PopoverAlign) -> Self {
        self.align = align;
        self
    }

    pub fn side_offset(mut self, offset: f32) -> Self {
        self.side_offset = offset;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn padding(mut self, padding: f32) -> Self {
        self.padding = padding;
        self
    }
}

/// `shadow-md ring-1 ring-foreground/10`, as the menus draw it.
fn shadow(ring: Hsla) -> Vec<BoxShadow> {
    let drop = |y: f32, blur: f32, spread: f32| BoxShadow {
        color: Hsla::black().opacity(0.1),
        offset: point(px(0.0), px(y)),
        blur_radius: px(blur),
        spread_radius: px(spread),
        inset: false,
    };
    vec![
        drop(4.0, 6.0, -1.0),
        drop(2.0, 4.0, -2.0),
        BoxShadow {
            color: ring,
            offset: point(px(0.0), px(0.0)),
            blur_radius: px(0.0),
            spread_radius: px(1.0),
            inset: false,
        },
    ]
}

impl RenderOnce for Popover {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let open = self.open;
        let toggle = self.on_open_change.clone();
        let popup = open.then(|| {
            let theme = cx.theme();
            let colors = &theme.colors;
            let card = div()
                .id("popover-content")
                .occlude()
                .flex()
                .flex_col()
                .w(px(self.width))
                .p(px(self.padding))
                .rounded(theme.radius.lg)
                .bg(colors.popover)
                .text_color(colors.popover_foreground)
                .text_size(px(14.0))
                .line_height(px(20.0))
                .shadow(shadow(colors.foreground.opacity(0.1)));
            let outside = self.on_open_change.clone();
            let escape = self.on_open_change.clone();
            let card = card
                .on_mouse_down_out(move |_, window, cx| outside(false, window, cx))
                .on_key_down(move |event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" {
                        cx.stop_propagation();
                        escape(false, window, cx);
                    }
                })
                .child((self.content)(window, cx));
            let (spot, anchor) = match self.align {
                PopoverAlign::Start => (div().left_0().bottom_0(), Anchor::TopLeft),
                PopoverAlign::End => (div().right_0().bottom_0(), Anchor::TopRight),
            };
            spot.absolute().size_0().child(
                deferred(
                    anchored()
                        .anchor(anchor)
                        .offset(point(px(0.0), px(self.side_offset)))
                        .snap_to_window_with_margin(px(8.0))
                        .child(card),
                )
                .priority(2),
            )
        });
        div()
            .id(self.id)
            .relative()
            .flex_shrink_0()
            .on_click(move |_: &ClickEvent, window, cx| {
                cx.stop_propagation();
                toggle(!open, window, cx);
            })
            .child(self.trigger)
            .children(popup)
    }
}
