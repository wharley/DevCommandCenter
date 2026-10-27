//! shadcn's `Command` pieces (`components/ui/command.tsx`) as the command
//! dialogs use them: the card 760px wide at 18% from the top, the field
//! with the search icon, group headings, items and the empty line.

use gpui::{
    App, Div, ElementId, FocusHandle, FontWeight, IntoElement, Stateful, Window, div, prelude::*,
    px,
};

use super::{Dialog, Icon};
use crate::theme::ActiveTheme;

/// `CommandDialog`: `top-[18vh] max-h-[min(620px,84vh)] w-[min(92vw,760px)]
/// rounded-2xl border border-border/60 bg-background p-0 shadow-2xl`,
/// without the close button; the field takes the focus.
pub fn command_dialog(
    id: &'static str,
    focus: FocusHandle,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
    window: &Window,
    cx: &App,
) -> Dialog {
    let viewport = window.viewport_size();
    let theme = cx.theme();
    Dialog::new(id, on_close)
        .top(0.18)
        .radius(f32::from(theme.radius.xxl))
        .surface(theme.colors.background)
        .border(theme.colors.border.opacity(0.6))
        .shadow_2xl()
        .max_width((f32::from(viewport.width) * 0.92).min(760.0))
        .max_height((f32::from(viewport.height) * 0.84).min(620.0))
        .padding(0.0)
        .gap(0.0)
        .close_button(false)
        .initial_focus(focus)
}

/// The band over the field (`border-b border-border/60 bg-muted/20 px-4
/// py-3`): the kicker (10px semibold, uppercase, `tracking-[0.08em]`) and a
/// line of description.
pub fn command_banner(
    title: impl Into<gpui::SharedString>,
    description: impl Into<gpui::SharedString>,
    cx: &App,
) -> Div {
    let colors = &cx.theme().colors;
    let title: gpui::SharedString = title.into();
    div()
        .border_b_1()
        .border_color(colors.border.opacity(0.6))
        .bg(colors.muted.opacity(0.2))
        .px(px(16.0))
        .py(px(12.0))
        .text_color(colors.muted_foreground)
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .child(super::Tracked::new(title.to_uppercase(), 10.0, 0.08)),
        )
        .child(
            div()
                .mt(px(4.0))
                .text_size(px(12.0))
                .line_height(px(19.5))
                .child(description.into()),
        )
}

/// `CommandSeparator`: `-mx-1 h-px bg-border`.
pub fn command_separator(cx: &App) -> Div {
    div().h(px(1.0)).w_full().bg(cx.theme().colors.border)
}

/// `CommandShortcut`: `ml-auto text-xs tracking-widest text-muted-foreground`.
pub fn command_shortcut(text: impl Into<gpui::SharedString>, cx: &App) -> Div {
    div()
        .ml_auto()
        .flex_none()
        .text_color(cx.theme().colors.muted_foreground)
        .child(super::Tracked::new(text, 12.0, 0.1))
}

/// `CommandList className="max-h-[min(58vh,32rem)]"`
pub fn command_list_height(window: &Window) -> f32 {
    (f32::from(window.viewport_size().height) * 0.58).min(512.0)
}

/// `CommandInput`: `p-1`, then a 32px box (`rounded-lg border-input/30
/// bg-input/30 px-2.5`) with the search icon at half opacity. The caller
/// adds the ↑/↓ handling.
pub fn command_field(id: &'static str, input: impl IntoElement, cx: &App) -> Div {
    let colors = &cx.theme().colors;
    div().p(px(4.0)).child(
        div()
            .id(id)
            .debug_selector(move || id.to_string())
            .flex()
            .h(px(32.0))
            .items_center()
            .gap(px(4.0))
            .rounded(cx.theme().radius.lg)
            .border_1()
            .border_color(colors.input.opacity(0.3))
            .bg(colors.input.opacity(0.3))
            .px(px(10.0))
            .text_size(px(14.0))
            .child(div().opacity(0.5).child(Icon::new("search").size(16.0)))
            .child(div().flex_1().min_w_0().child(input)),
    )
}

/// `CommandGroup`: `p-1` around its heading and items.
pub fn command_group() -> Div {
    div().p(px(4.0))
}

/// `CommandGroup`'s heading: `px-2 py-1.5 text-xs font-medium
/// text-muted-foreground`.
pub fn command_heading(content: impl IntoElement, cx: &App) -> Div {
    div()
        .px(px(8.0))
        .py(px(6.0))
        .text_size(px(12.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().colors.muted_foreground)
        .child(content)
}

/// `CommandItem`: `rounded-sm px-2 py-1.5 gap-2 text-sm`, `bg-muted` while
/// selected.
pub fn command_item(id: impl Into<ElementId>, selected: bool, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    div()
        .id(id)
        .flex()
        .gap(px(8.0))
        .rounded(theme.radius.sm)
        .px(px(8.0))
        .py(px(6.0))
        .text_size(px(14.0))
        .line_height(px(20.0))
        .cursor_pointer()
        .when(selected, |item| {
            item.bg(theme.colors.muted)
                .text_color(theme.colors.foreground)
        })
}

/// A line of its own under the field (loading, a hint): `px-4 py-3 text-sm
/// text-muted-foreground`.
pub fn command_note(content: impl IntoElement, cx: &App) -> Div {
    div()
        .px(px(16.0))
        .py(px(12.0))
        .text_size(px(14.0))
        .text_color(cx.theme().colors.muted_foreground)
        .child(content)
}

/// `CommandEmpty`: `py-6 text-center text-sm`.
pub fn command_empty(content: impl IntoElement) -> Div {
    div()
        .py(px(24.0))
        .text_center()
        .text_size(px(14.0))
        .child(content)
}
