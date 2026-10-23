//! shadcn's Dialog (`components/ui/dialog.tsx`): a centered card over a dim
//! backdrop. The owner decides when it is open; a click outside, Esc or the
//! close button ask it to close (`on_close`), unless it is busy.

use std::rc::Rc;

use gpui::{
    AnyElement, App, BoxShadow, ElementId, FocusHandle, Hsla, KeyDownEvent, SharedString, Window,
    anchored, deferred, div, point, prelude::*, px,
};
use smallvec::SmallVec;

use super::{Button, ButtonSize, ButtonVariant, Icon};
use crate::theme::ActiveTheme;

type OnClose = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Dialog {
    id: ElementId,
    max_width: f32,
    max_height: Option<f32>,
    padding: f32,
    gap: f32,
    close_button: bool,
    on_close: OnClose,
    children: SmallVec<[AnyElement; 4]>,
}

impl Dialog {
    pub fn new(
        id: impl Into<ElementId>,
        on_close: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            max_width: 384.0,
            max_height: None,
            padding: 16.0,
            gap: 16.0,
            close_button: true,
            on_close: Rc::new(on_close),
            children: SmallVec::new(),
        }
    }

    /// `sm:max-w-sm` is 384 (the default), `sm:max-w-md` 448.
    pub fn max_width(mut self, width: f32) -> Self {
        self.max_width = width;
        self
    }

    /// A tall dialog keeps its footer in reach; its body scrolls.
    pub fn max_height(mut self, height: f32) -> Self {
        self.max_height = Some(height);
        self
    }

    /// `p-4` by default.
    pub fn padding(mut self, padding: f32) -> Self {
        self.padding = padding;
        self
    }

    /// `gap-4` by default.
    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    /// `showCloseButton`; hidden while an action runs.
    pub fn close_button(mut self, show: bool) -> Self {
        self.close_button = show;
        self
    }
}

impl ParentElement for Dialog {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

/// `--dcc-shadow-dialog` plus the `ring-foreground/10` edge.
fn dialog_shadow(ring: Hsla) -> Vec<BoxShadow> {
    let shadow = |alpha: f32, y: f32, blur: f32| BoxShadow {
        color: Hsla::black().opacity(alpha),
        offset: point(px(0.0), px(y)),
        blur_radius: px(blur),
        spread_radius: px(0.0),
        inset: false,
    };
    vec![
        shadow(0.16, 24.0, 80.0),
        shadow(0.06, 4.0, 14.0),
        BoxShadow {
            color: ring,
            offset: point(px(0.0), px(0.0)),
            blur_radius: px(0.0),
            spread_radius: px(1.0),
            inset: false,
        },
    ]
}

impl RenderOnce for Dialog {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let focus = window
            .use_keyed_state(self.id.clone(), cx, |window, cx| {
                let handle: FocusHandle = cx.focus_handle();
                window.focus(&handle, cx);
                handle
            })
            .read(cx)
            .clone();
        let viewport = window.viewport_size();
        let theme = cx.theme();
        let colors = &theme.colors;
        let on_backdrop = self.on_close.clone();
        let on_escape = self.on_close.clone();
        let on_button = self.on_close.clone();
        let busy = !self.close_button;

        let card = div()
            .id("dialog-content")
            .track_focus(&focus)
            .relative()
            .flex()
            .flex_col()
            .gap(px(self.gap))
            .w(px(self.max_width.min(f32::from(viewport.width) - 32.0)))
            .when_some(self.max_height, |card, height| {
                card.max_h(px(height)).overflow_hidden()
            })
            .p(px(self.padding))
            .rounded(theme.radius.dialog)
            .bg(colors.popover)
            .text_color(colors.popover_foreground)
            .text_size(px(14.0))
            .line_height(px(20.0))
            .shadow(dialog_shadow(colors.foreground.opacity(0.1)))
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && !busy {
                    on_escape(window, cx);
                }
            })
            .children(self.children)
            .when(self.close_button, |card| {
                card.child(
                    div().absolute().top(px(8.0)).right(px(8.0)).child(
                        Button::new("dialog-close")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::IconSm)
                            .child(Icon::new("x"))
                            .on_click(move |_, window, cx| on_button(window, cx)),
                    ),
                )
            });

        deferred(
            anchored()
                .position(point(px(0.0), px(0.0)))
                .position_mode(gpui::AnchoredPositionMode::Window)
                .child(
                    div()
                        .id(self.id)
                        .occlude()
                        .w(viewport.width)
                        .h(viewport.height)
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(Hsla::black().opacity(0.15))
                        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                            if !busy {
                                on_backdrop(window, cx);
                            }
                        })
                        .child(card),
                ),
        )
        .priority(10)
    }
}

/// `DialogHeader` with `DialogTitle` and `DialogDescription`.
pub fn dialog_header(
    title: impl Into<SharedString>,
    description: Option<SharedString>,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().colors.muted_foreground;
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .pr(px(28.0))
        .child(
            div()
                .text_size(px(16.0))
                .line_height(px(16.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(title.into()),
        )
        .when_some(description, |header, description| {
            header.child(div().text_color(muted).child(description))
        })
        .into_any_element()
}

/// `DialogFooter`: right-aligned actions on a muted strip at the bottom.
pub fn dialog_footer(children: impl IntoIterator<Item = AnyElement>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .mx(px(-16.0))
        .mb(px(-16.0))
        .flex()
        .justify_end()
        .gap(px(8.0))
        .p(px(16.0))
        .rounded_b(theme.radius.dialog)
        .border_t_1()
        .border_color(theme.colors.border)
        .bg(theme.colors.muted.opacity(0.5))
        .children(children)
        .into_any_element()
}

/// `DialogFooter` with a note on the left and the actions on the right
/// (`sm:justify-between`).
pub fn dialog_footer_split(
    note: AnyElement,
    actions: impl IntoIterator<Item = AnyElement>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    div()
        .mx(px(-16.0))
        .mb(px(-16.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .p(px(16.0))
        .rounded_b(theme.radius.dialog)
        .border_t_1()
        .border_color(theme.colors.border)
        .bg(theme.colors.muted.opacity(0.5))
        .child(note)
        .child(div().flex().gap(px(8.0)).children(actions))
        .into_any_element()
}
