//! Vertical resize separator: a 20px hit area with a 1px line in the middle
//! that thickens on hover and while dragging (desktop `ResizeSeparator`).

use gpui::{
    App, BoxShadow, CursorStyle, ElementId, EmptyView, Hsla, MouseButton, MouseDownEvent, Pixels,
    SharedString, Window, div, point, prelude::*, px,
};

use crate::theme::{ActiveTheme, Appearance};

const HIT_AREA: f32 = 20.0;
const LINE: f32 = 1.0;
const LINE_EMPHASIZED: f32 = 2.0;

type StartHandler = Box<dyn Fn(Pixels, &mut Window, &mut App) + 'static>;

/// `T` identifies the drag, so the parent's `on_drag_move::<T>` only sees this handle.
#[derive(IntoElement)]
pub struct ResizeHandle<T: Clone + 'static> {
    id: ElementId,
    drag: T,
    left: Pixels,
    active: bool,
    on_start: Option<StartHandler>,
}

impl<T: Clone + 'static> ResizeHandle<T> {
    pub fn new(id: impl Into<ElementId>, drag: T) -> Self {
        Self {
            id: id.into(),
            drag,
            left: px(0.0),
            active: false,
            on_start: None,
        }
    }

    /// Left edge of the hit area, relative to the positioned parent.
    pub fn left(mut self, left: Pixels) -> Self {
        self.left = left;
        self
    }

    /// Draws the emphasized line while a drag is in progress.
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Called on press with the pointer's window x.
    pub fn on_start(mut self, handler: impl Fn(Pixels, &mut Window, &mut App) + 'static) -> Self {
        self.on_start = Some(Box::new(handler));
        self
    }
}

impl<T: Clone + 'static> RenderOnce for ResizeHandle<T> {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let group: SharedString = format!("resize-{:?}", self.id).into();
        let idle = theme.colors.border;
        let hover = theme.colors.muted_foreground.opacity(0.75);
        let pressed = theme.colors.foreground.opacity(0.8);
        let glow = match theme.appearance {
            Appearance::Light => Hsla::black().opacity(0.12),
            Appearance::Dark => Hsla::white().opacity(0.16),
        };
        let on_start = self.on_start;

        let line = div().absolute().top_0().bottom_0();
        let line = if self.active {
            line.left(px((HIT_AREA - LINE_EMPHASIZED) / 2.0))
                .w(px(LINE_EMPHASIZED))
                .bg(pressed)
                .shadow(vec![BoxShadow {
                    color: glow,
                    offset: point(px(0.0), px(0.0)),
                    blur_radius: px(12.0),
                    spread_radius: px(0.0),
                    inset: false,
                }])
        } else {
            line.left(px((HIT_AREA - LINE) / 2.0))
                .w(px(LINE))
                .bg(idle)
                .group_hover(group.clone(), move |style| {
                    style
                        .left(px((HIT_AREA - LINE_EMPHASIZED) / 2.0))
                        .w(px(LINE_EMPHASIZED))
                        .bg(hover)
                })
        };

        div()
            .id(self.id)
            .group(group)
            .absolute()
            .top_0()
            .bottom_0()
            .left(self.left)
            .w(px(HIT_AREA))
            .cursor(CursorStyle::ResizeLeftRight)
            .on_mouse_down(
                MouseButton::Left,
                move |event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    if let Some(handler) = &on_start {
                        handler(event.position.x, window, cx);
                    }
                },
            )
            .on_drag(self.drag, |_, _, window, cx| {
                cx.set_active_drag_cursor_style(CursorStyle::ResizeLeftRight, window);
                cx.new(|_| EmptyView)
            })
            .child(line)
    }
}
