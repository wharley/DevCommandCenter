//! What the list shows with no tasks: a nudge to start one, or, with no
//! project at all, the first-run invitation to open or clone a project.

use gpui::{AnyElement, Context, div, prelude::*, px};

use super::{Sidebar, SidebarEvent};
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon};

fn action(
    id: &'static str,
    variant: ButtonVariant,
    icon: &'static str,
    label: gpui::SharedString,
    event: SidebarEvent,
    cx: &mut Context<Sidebar>,
) -> Button {
    Button::new(id)
        .variant(variant)
        .size(ButtonSize::Sm)
        .child(Icon::new(icon).size(14.0).stroke(2.0))
        .child(div().text_size(px(14.0)).child(label))
        .on_click(cx.listener(move |_, _, _, cx| cx.emit(event.clone())))
}

pub fn render(sidebar: &Sidebar, cx: &mut Context<Sidebar>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    if !sidebar.state.data.projects.is_empty() {
        let label = cx.t("sidebar.newWorkspace");
        return div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(12.0))
            .px(px(16.0))
            .py(px(32.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(20.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("sidebar.noActiveTasks")),
            )
            .child(action(
                "empty-new-task",
                ButtonVariant::Outline,
                "plus",
                label,
                SidebarEvent::NewTask,
                cx,
            ))
            .into_any_element();
    }
    let open = cx.t("sidebar.openProject");
    let clone = cx.t("sidebar.cloneFromUrl");
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .px(px(16.0))
        .py(px(32.0))
        .child(
            div()
                .mb(px(12.0))
                .flex()
                .items_center()
                .justify_center()
                .size(px(44.0))
                .rounded_full()
                .border_1()
                .border_color(colors.border.opacity(0.7))
                .bg(colors.muted.opacity(0.2))
                .text_color(colors.muted_foreground)
                .child(Icon::new("folder-plus").size(20.0).stroke(1.9)),
        )
        .child(
            div()
                .text_size(px(15.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(colors.foreground)
                .child(cx.t("sidebar.noWorkspacesYet")),
        )
        .child(
            div()
                .mt(px(8.0))
                .max_w(px(288.0))
                .text_center()
                .text_size(px(13.0))
                .line_height(px(24.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("sidebar.noWorkspacesHint")),
        )
        .child(
            div()
                .mt(px(20.0))
                .flex()
                .flex_wrap()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .child(action(
                    "empty-open",
                    ButtonVariant::Default,
                    "folder-plus",
                    open,
                    SidebarEvent::OpenProject,
                    cx,
                ))
                .child(action(
                    "empty-clone",
                    ButtonVariant::Outline,
                    "plus",
                    clone,
                    SidebarEvent::CloneProject,
                    cx,
                )),
        )
        .into_any_element()
}
