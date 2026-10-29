//! "Tarefas", the running count that jumps to the next running task, and
//! the open-project and new-task buttons.

use std::time::Duration;

use gpui::{AnimationExt, ClickEvent, Context, Window, div, prelude::*, px};

use super::labels::emerald_text;
use super::{Sidebar, SidebarEvent};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Icon, ItemStyle, Menu, MenuAlign, MenuItem, tabular_nums,
};

/// The folder-plus menu: open a local project or clone one.
fn open_project_menu(cx: &mut Context<Sidebar>) -> Menu {
    let entity = cx.entity().downgrade();
    Menu::dropdown(
        "open-project-menu",
        |_| {
            Button::new("open-project-menu-trigger")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconXs)
                .muted()
                .child(Icon::new("folder-plus").stroke(1.9))
                .into_any_element()
        },
        move |_, cx| {
            let (open, clone) = (entity.clone(), entity.clone());
            vec![
                MenuItem::new(cx.t("sidebar.openProject"), move |_, cx| {
                    let _ = open.update(cx, |_, cx| cx.emit(SidebarEvent::OpenProject));
                })
                .into(),
                MenuItem::new(cx.t("sidebar.cloneFromUrl"), move |_, cx| {
                    let _ = clone.update(cx, |_, cx| cx.emit(SidebarEvent::CloneProject));
                })
                .into(),
            ]
        },
    )
    .align(MenuAlign::End)
    .side_offset(6.0)
    .min_width(176.0)
    .item_style(ItemStyle {
        text_size: 13.0,
        gap: 8.0,
    })
}

pub fn render(sidebar: &Sidebar, cx: &mut Context<Sidebar>) -> impl IntoElement {
    let running = sidebar.state.running_ids().len();
    let theme = cx.theme();
    let muted = theme.colors.muted_foreground;
    let running_text = emerald_text(cx);
    let running_hover = tw::EMERALD_500.alpha(0.1);
    let running_label = cx.t_with("sidebar.runningCount", &Args::count(running as i64));
    let title = cx.t("sidebar.tasksTitle");
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .px(px(12.0))
        .child(
            div()
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(muted)
                        .child(title),
                )
                .when(running > 0, |row| {
                    row.child(
                        div()
                            .id("jump-to-running")
                            .mx(px(-4.0))
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .px(px(4.0))
                            .py(px(2.0))
                            .rounded(theme.radius.md)
                            .cursor_pointer()
                            .text_color(running_text)
                            .hover(move |style| style.bg(running_hover))
                            .on_click(cx.listener(|this, _: &ClickEvent, _: &mut Window, cx| {
                                this.jump_to_running(cx)
                            }))
                            .child(
                                // `animate-pulse`: opacity 1 → 0.5 → 1 every 2s.
                                div()
                                    .size(px(6.0))
                                    .rounded_full()
                                    .bg(tw::EMERALD_500.hsla())
                                    .with_animation(
                                        "running-pulse",
                                        crate::ui::looping(Duration::from_secs(2)),
                                        |dot, delta| {
                                            let wave = (delta * std::f32::consts::TAU).cos();
                                            dot.opacity(0.75 + 0.25 * wave)
                                        },
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .font_features(tabular_nums())
                                    .child(running_label),
                            ),
                    )
                }),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .child(open_project_menu(cx))
                .child(super::repo_picker::render(
                    sidebar,
                    super::repo_picker::Place::Header,
                    cx,
                )),
        )
}
