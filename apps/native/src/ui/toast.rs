//! The app's toasts, drawn like the desktop's sonner `<Toaster
//! position="bottom-right" visibleToasts={6} closeButton />`: sonner's own
//! colors and system font, 356px wide, 24px from the corner, gone after 4s
//! (held while the pointer is over them). One difference: sonner stacks
//! them folded until hovered; here they are always laid out with its 14px gap.

use std::time::Duration;

use gpui::{
    Anchor, App, BoxShadow, Context, Entity, Global, Hsla, SharedString, Task, Window, anchored,
    deferred, div, point, prelude::*, px, rgb,
};

use super::Icon;
use crate::theme::{ActiveTheme, Appearance};

const LIFETIME: Duration = Duration::from_millis(4000);
const VISIBLE: usize = 6;
const WIDTH: f32 = 356.0;
const OFFSET: f32 = 24.0;
const GAP: f32 = 14.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Error,
    Info,
}

struct Toast {
    id: u64,
    kind: ToastKind,
    message: SharedString,
    /// Its time ran out while the pointer was over the stack.
    expired: bool,
    _timer: Task<()>,
}

pub struct Toaster {
    toasts: Vec<Toast>,
    next_id: u64,
    hovered: bool,
}

struct GlobalToaster(Entity<Toaster>);

impl Global for GlobalToaster {}

impl Toaster {
    pub fn init(cx: &mut App) {
        let toaster = cx.new(|_| Self {
            toasts: Vec::new(),
            next_id: 0,
            hovered: false,
        });
        cx.set_global(GlobalToaster(toaster));
    }

    /// The host the window renders (once, over everything).
    pub fn handle(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalToaster>()
            .map(|global| global.0.clone())
    }

    pub fn error(message: impl Into<SharedString>, cx: &mut App) {
        Self::show(ToastKind::Error, message.into(), cx);
    }

    pub fn info(message: impl Into<SharedString>, cx: &mut App) {
        Self::show(ToastKind::Info, message.into(), cx);
    }

    fn show(kind: ToastKind, message: SharedString, cx: &mut App) {
        match Self::handle(cx) {
            Some(toaster) => toaster.update(cx, |toaster, cx| toaster.push(kind, message, cx)),
            None => eprintln!("[dcc-native] {message}"),
        }
    }

    fn push(&mut self, kind: ToastKind, message: SharedString, cx: &mut Context<Self>) {
        let id = self.next_id;
        self.next_id += 1;
        let timer = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(LIFETIME).await;
            let _ = this.update(cx, |toaster, cx| toaster.expire(id, cx));
        });
        self.toasts.push(Toast {
            id,
            kind,
            message,
            expired: false,
            _timer: timer,
        });
        cx.notify();
    }

    fn expire(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.hovered {
            if let Some(toast) = self.toasts.iter_mut().find(|toast| toast.id == id) {
                toast.expired = true;
            }
            return;
        }
        self.dismiss(id, cx);
    }

    fn dismiss(&mut self, id: u64, cx: &mut Context<Self>) {
        self.toasts.retain(|toast| toast.id != id);
        cx.notify();
    }

    fn set_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        self.hovered = hovered;
        if !hovered {
            self.toasts.retain(|toast| !toast.expired);
        }
        cx.notify();
    }
}

/// sonner's `--normal-*` colors for each theme.
struct Palette {
    background: Hsla,
    border: Hsla,
    text: Hsla,
    close_border: Hsla,
}

fn palette(dark: bool) -> Palette {
    let gray = |lightness: f32| gpui::hsla(0.0, 0.0, lightness, 1.0);
    if dark {
        Palette {
            background: rgb(0x000000).into(),
            border: gray(0.20),
            text: gray(0.99),
            close_border: gray(0.20),
        }
    } else {
        Palette {
            background: rgb(0xffffff).into(),
            border: gray(0.93),
            text: gray(0.09),
            close_border: gray(0.93),
        }
    }
}

impl Render for Toaster {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.toasts.is_empty() {
            return div().into_any_element();
        }
        let colors = palette(cx.theme().appearance == Appearance::Dark);
        let viewport = window.viewport_size();
        let start = self.toasts.len().saturating_sub(VISIBLE);
        let mut items = Vec::new();
        for toast in &self.toasts[start..] {
            items.push(self.toast(toast, &colors, cx));
        }
        deferred(
            anchored()
                .position_mode(gpui::AnchoredPositionMode::Window)
                .position(point(
                    viewport.width - px(OFFSET),
                    viewport.height - px(OFFSET),
                ))
                .anchor(Anchor::BottomRight)
                .child(
                    div()
                        .id("toaster")
                        .occlude()
                        .w(px(WIDTH))
                        .flex()
                        .flex_col()
                        .gap(px(GAP))
                        .font_family(".SystemUIFont")
                        .on_hover(
                            cx.listener(|this, hovered: &bool, _, cx| {
                                this.set_hovered(*hovered, cx)
                            }),
                        )
                        .children(items),
                ),
        )
        .priority(20)
        .into_any_element()
    }
}

impl Toaster {
    fn toast(&self, toast: &Toast, colors: &Palette, cx: &mut Context<Self>) -> gpui::AnyElement {
        let id = toast.id;
        let icon = match toast.kind {
            ToastKind::Error => Some("sonner-error"),
            ToastKind::Info => None,
        };
        div()
            .id(("toast", id as usize))
            .relative()
            .flex()
            .items_center()
            .gap(px(6.0))
            .w_full()
            .p(px(16.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(colors.border)
            .bg(colors.background)
            .text_color(colors.text)
            .text_size(px(13.0))
            .shadow(vec![BoxShadow {
                color: Hsla::black().opacity(0.1),
                offset: point(px(0.0), px(4.0)),
                blur_radius: px(12.0),
                spread_radius: px(0.0),
                inset: false,
            }])
            .when_some(icon, |toast, icon| {
                toast.child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .size(px(16.0))
                        .child(Icon::new(icon).size(20.0)),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .line_height(gpui::relative(1.5))
                    .child(toast.message.clone()),
            )
            .child(
                // `translate(-35%, -35%)` of a 20px button at the top-left corner.
                div()
                    .id(("toast-close", id as usize))
                    .absolute()
                    .left(px(-7.0))
                    .top(px(-7.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(20.0))
                    .rounded_full()
                    .border_1()
                    .border_color(colors.close_border)
                    .bg(colors.background)
                    .cursor_pointer()
                    .child(Icon::new("x").size(12.0).stroke(1.5))
                    .on_click(cx.listener(move |this, _, _, cx| this.dismiss(id, cx))),
            )
            .into_any_element()
    }
}
