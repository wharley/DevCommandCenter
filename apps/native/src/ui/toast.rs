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
    Success,
    Info,
    Warning,
    /// sonner's `toast.loading`: stays, turning, until settled.
    Loading,
}

struct Toast {
    id: u64,
    kind: ToastKind,
    message: SharedString,
    /// A second, quieter line (sonner's `description`).
    description: Option<SharedString>,
    /// Its time ran out while the pointer was over the stack.
    expired: bool,
    /// sonner's `action`: a button that runs and closes the toast.
    action: Option<ToastAction>,
    _timer: Task<()>,
}

struct ToastAction {
    label: SharedString,
    run: std::rc::Rc<dyn Fn(&mut App)>,
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

    /// The messages showing, for tests.
    #[cfg(test)]
    pub fn messages(cx: &App) -> Vec<String> {
        Self::handle(cx)
            .map(|toaster| {
                toaster
                    .read(cx)
                    .toasts
                    .iter()
                    .map(|toast| toast.message.to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn error(message: impl Into<SharedString>, cx: &mut App) {
        Self::show(ToastKind::Error, message.into(), None, cx);
    }

    pub fn error_with(
        message: impl Into<SharedString>,
        description: impl Into<SharedString>,
        cx: &mut App,
    ) {
        Self::show(
            ToastKind::Error,
            message.into(),
            Some(description.into()),
            cx,
        );
    }

    pub fn success(message: impl Into<SharedString>, cx: &mut App) {
        Self::show(ToastKind::Success, message.into(), None, cx);
    }

    /// sonner's `toast.success(message, { description })`.
    pub fn success_with(
        message: impl Into<SharedString>,
        description: impl Into<SharedString>,
        cx: &mut App,
    ) {
        Self::show(
            ToastKind::Success,
            message.into(),
            Some(description.into()),
            cx,
        );
    }

    pub fn info(message: impl Into<SharedString>, cx: &mut App) {
        Self::show(ToastKind::Info, message.into(), None, cx);
    }

    /// sonner's `toast.warning(message)`.
    pub fn warning(message: impl Into<SharedString>, cx: &mut App) {
        Self::show(ToastKind::Warning, message.into(), None, cx);
    }

    /// sonner's `toast.warning(message, { description })`.
    pub fn warning_with(
        message: impl Into<SharedString>,
        description: impl Into<SharedString>,
        cx: &mut App,
    ) {
        Self::show(
            ToastKind::Warning,
            message.into(),
            Some(description.into()),
            cx,
        );
    }

    /// `toast.error(message, { description, duration, action })`: stays
    /// `lifetime` and offers one step.
    pub fn error_action(
        message: impl Into<SharedString>,
        description: impl Into<SharedString>,
        label: impl Into<SharedString>,
        lifetime: Duration,
        run: impl Fn(&mut App) + 'static,
        cx: &mut App,
    ) {
        let (message, description) = (message.into(), Some(description.into()));
        let action = ToastAction {
            label: label.into(),
            run: std::rc::Rc::new(run),
        };
        let Some(toaster) = Self::handle(cx) else {
            return eprintln!("[dcc-native] {message}");
        };
        toaster.update(cx, |toaster, cx| {
            let id = toaster.push(ToastKind::Error, message, description, cx);
            if let Some(toast) = toaster.toasts.iter_mut().find(|toast| toast.id == id) {
                toast.action = Some(action);
                toast._timer = Self::timer_for(id, lifetime, cx);
            }
        });
    }

    /// `toast.loading(message)`: the id to settle it with.
    pub fn loading(message: impl Into<SharedString>, cx: &mut App) -> Option<u64> {
        let message = message.into();
        let toaster = Self::handle(cx)?;
        Some(toaster.update(cx, |toaster, cx| {
            toaster.push(ToastKind::Loading, message, None, cx)
        }))
    }

    /// `toast.success(message, { id })` and the like: the loading toast
    /// becomes this one, in place, and its time starts.
    pub fn settle(
        id: Option<u64>,
        kind: ToastKind,
        message: impl Into<SharedString>,
        description: Option<SharedString>,
        cx: &mut App,
    ) {
        let (message, description) = (message.into(), description.filter(|d| !d.is_empty()));
        let Some((toaster, id)) = Self::handle(cx).zip(id) else {
            return Self::show(kind, message, description, cx);
        };
        toaster.update(cx, |toaster, cx| {
            if let Some(at) = toaster.toasts.iter().position(|toast| toast.id == id) {
                let toast = &mut toaster.toasts[at];
                toast.kind = kind;
                toast.message = message;
                toast.description = description;
                toast._timer = if kind == ToastKind::Loading {
                    Task::ready(())
                } else {
                    Self::timer(id, cx)
                };
                cx.notify();
            } else {
                toaster.push(kind, message, description, cx);
            }
        });
    }

    /// `toast.dismiss(id)`
    pub fn dismiss_id(id: Option<u64>, cx: &mut App) {
        if let Some((toaster, id)) = Self::handle(cx).zip(id) {
            toaster.update(cx, |toaster, cx| toaster.dismiss(id, cx));
        }
    }

    fn timer(id: u64, cx: &mut Context<Self>) -> Task<()> {
        Self::timer_for(id, LIFETIME, cx)
    }

    fn timer_for(id: u64, lifetime: Duration, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(lifetime).await;
            let _ = this.update(cx, |toaster, cx| toaster.expire(id, cx));
        })
    }

    fn show(
        kind: ToastKind,
        message: SharedString,
        description: Option<SharedString>,
        cx: &mut App,
    ) {
        let description = description.filter(|description| !description.is_empty());
        match Self::handle(cx) {
            Some(toaster) => {
                toaster.update(cx, |toaster, cx| {
                    toaster.push(kind, message, description, cx);
                });
            }
            None => eprintln!("[dcc-native] {message}"),
        }
    }

    fn push(
        &mut self,
        kind: ToastKind,
        message: SharedString,
        description: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let timer = if kind == ToastKind::Loading {
            Task::ready(())
        } else {
            Self::timer(id, cx)
        };
        self.toasts.push(Toast {
            id,
            kind,
            message,
            description,
            expired: false,
            action: None,
            _timer: timer,
        });
        cx.notify();
        id
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
    description: Hsla,
    close_border: Hsla,
}

fn palette(dark: bool) -> Palette {
    let gray = |lightness: f32| gpui::hsla(0.0, 0.0, lightness, 1.0);
    if dark {
        Palette {
            background: rgb(0x000000).into(),
            border: gray(0.20),
            text: gray(0.99),
            description: gray(0.91),
            close_border: gray(0.20),
        }
    } else {
        Palette {
            background: rgb(0xffffff).into(),
            border: gray(0.93),
            text: gray(0.09),
            description: rgb(0x3f3f3f).into(),
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
            ToastKind::Success => Some("sonner-success"),
            ToastKind::Info => Some("sonner-info"),
            ToastKind::Warning => Some("sonner-warning"),
            ToastKind::Loading => None,
        };
        let loading = toast.kind == ToastKind::Loading;
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
            .when(loading, |toast| {
                toast.child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .size(px(16.0))
                        .items_center()
                        .justify_center()
                        .child(crate::ui::spinner(("toast-loading", id as usize), 16.0)),
                )
            })
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
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .line_height(gpui::relative(1.5))
                            .child(toast.message.clone()),
                    )
                    .when_some(toast.description.clone(), |content, description| {
                        content.child(
                            div()
                                .line_height(gpui::relative(1.4))
                                .text_color(colors.description)
                                .child(description),
                        )
                    }),
            )
            .when_some(toast.action.as_ref(), |row, action| {
                let run = action.run.clone();
                // sonner's `[data-button]`: 24px, the text's color as its fill.
                row.child(
                    div()
                        .id(("toast-action", id as usize))
                        .debug_selector(move || format!("toast-action-{id}"))
                        .flex()
                        .flex_shrink_0()
                        .h(px(24.0))
                        .items_center()
                        .rounded(px(4.0))
                        .px(px(8.0))
                        .bg(colors.text)
                        .text_color(colors.background)
                        .text_size(px(12.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .cursor_pointer()
                        .child(action.label.clone())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.dismiss(id, cx);
                            let run = run.clone();
                            cx.defer(move |cx| run(cx));
                        })),
                )
            })
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
