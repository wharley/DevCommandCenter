//! `.dcc-task-launch`: the DCC mark, "Vamos construir", the project menu,
//! the Worktree and linked-folders checkboxes and up to four project
//! shortcuts (`.dcc-project-shortcut`).

use gpui::{
    AnyElement, BoxShadow, Context, FontWeight, Hsla, SharedString, Window, div, img, point,
    prelude::*, px,
};

use super::NewTaskView;
use crate::data::tasks::Project;
use crate::i18n::Translate;
use crate::project::ProjectGlyph;
use crate::theme::ActiveTheme;
use crate::theme::color::mix_oklab;
use crate::ui::popover::{Popover, PopoverAlign};
use crate::ui::{Checkbox, Icon, Tracked, spinner};

/// `--dcc-shadow-card`
fn card_shadow() -> Vec<BoxShadow> {
    let shadow = |alpha: f32, y: f32, blur: f32| BoxShadow {
        color: Hsla::black().opacity(alpha),
        offset: point(px(0.0), px(y)),
        blur_radius: px(blur),
        spread_radius: px(0.0),
        inset: false,
    };
    vec![shadow(0.025, 2.0, 3.0), shadow(0.04, 8.0, 24.0)]
}

/// `rounded-full bg-foreground/[0.08] px-1.5 text-[10px] font-semibold`
fn linked_badge(count: usize, cx: &Context<NewTaskView>) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .flex_shrink_0()
        .rounded_full()
        .bg(colors.foreground.opacity(0.08))
        .px(px(6.0))
        .text_size(px(10.0))
        .line_height(px(15.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(colors.muted_foreground)
        .child(format!("+{count}"))
        .into_any_element()
}

/// One project in the menu: glyph, name over its branch, "+N", a spinner
/// while its task is created.
fn menu_row(
    view: &NewTaskView,
    index: usize,
    project: &Project,
    cx: &mut Context<NewTaskView>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let busy = view.busy();
    let pending = view.pending.as_deref() == Some(project.id.as_str());
    let hover = colors.accent;
    let chosen = project.clone();
    div()
        .id(("new-task-project", index))
        .flex()
        .items_center()
        .gap(px(10.0))
        .rounded(px(4.0))
        .px(px(8.0))
        .py(px(8.0))
        .when(busy, |row| row.opacity(0.5))
        .when(!busy, |row| {
            row.cursor_pointer()
                .hover(move |style| style.bg(hover))
                .on_click(cx.listener(move |view, _, _, cx| view.select_project(&chosen, cx)))
        })
        .child(
            ProjectGlyph::new(
                project.icon.as_deref(),
                project.color.as_deref(),
                Some(project.root_path.as_str()),
            )
            .tile(24.0),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(SharedString::from(project.display_name.clone())),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(px(10.0))
                        .line_height(px(14.0))
                        .text_color(colors.muted_foreground)
                        .child(SharedString::from(project.base_branch.clone())),
                ),
        )
        .children(
            view.linked_badge(project)
                .map(|count| linked_badge(count, cx)),
        )
        .when(pending, |row| {
            row.child(spinner(("new-task-menu-busy", index), 14.0))
        })
        .into_any_element()
}

fn project_menu(view: &NewTaskView, cx: &mut Context<NewTaskView>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let busy = view.busy();
    let hover = colors.muted.opacity(0.4);
    let foreground = colors.foreground;
    let trigger = div()
        .mt(px(4.0))
        .flex()
        .h(px(36.0))
        .items_center()
        .gap(px(6.0))
        .rounded(px(6.0))
        .px(px(12.0))
        .text_size(px(17.0))
        .text_color(colors.muted_foreground)
        .when(busy, |trigger| trigger.opacity(0.5))
        .when(!busy, |trigger| {
            trigger
                .cursor_pointer()
                .hover(move |style| style.bg(hover).text_color(foreground))
        })
        .when(busy, |trigger| {
            trigger.child(spinner("new-task-creating", 16.0))
        })
        .child(cx.t("newTask.chooseProject"))
        .child(Icon::new("chevron-down").size(16.0).stroke(1.8));
    let rows: Vec<AnyElement> = view
        .projects()
        .iter()
        .enumerate()
        .map(|(index, project)| menu_row(view, index, project, cx))
        .collect();
    let heading = cx.t("newTask.projects");
    let muted = colors.muted_foreground;
    let entity = cx.entity().downgrade();
    Popover::new(
        "new-task-projects",
        view.menu_open,
        trigger,
        move |_, _| {
            div()
                .id("new-task-project-list")
                .flex()
                .flex_col()
                .max_h(px(384.0))
                .overflow_y_scroll()
                .child(
                    div()
                        .px(px(8.0))
                        .py(px(6.0))
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(muted)
                        .child(heading),
                )
                .children(rows)
                .into_any_element()
        },
        move |open, _, cx| {
            let _ = entity.update(cx, |view, cx| {
                if !view.busy() || !open {
                    view.menu_open = open;
                    cx.notify();
                }
            });
        },
    )
    .align(PopoverAlign::Center)
    .width(320.0)
    .padding(4.0)
    .into_any_element()
}

/// A checkbox row (`.dcc-task-launch-worktree`): label · explanation.
fn option_row(
    id: &'static str,
    checked: bool,
    title: SharedString,
    description: SharedString,
    disabled: bool,
    toggle: impl Fn(&mut NewTaskView, bool) + 'static,
    cx: &mut Context<NewTaskView>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let hover = colors.muted.opacity(0.4);
    let foreground = colors.foreground;
    let toggle = std::rc::Rc::new(toggle);
    let on_box = toggle.clone();
    let entity = cx.entity().downgrade();
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(8.0))
        .rounded(px(8.0))
        .px(px(10.0))
        .py(px(6.0))
        .text_size(px(12.0))
        .line_height(px(16.0))
        .text_color(colors.muted_foreground)
        .when(disabled, |row| row.opacity(0.6))
        .when(!disabled, |row| {
            row.cursor_pointer()
                .hover(move |style| style.bg(hover).text_color(foreground))
                .on_click(cx.listener(move |view, _, _, cx| {
                    toggle(view, !checked);
                    cx.notify();
                }))
        })
        .child(
            Checkbox::new(SharedString::from(format!("{id}-box")), checked)
                .side(14.0)
                .disabled(disabled)
                .on_toggle(move |next, _, cx| {
                    let on_box = on_box.clone();
                    let _ = entity.update(cx, |view, cx| {
                        on_box(view, next);
                        cx.notify();
                    });
                }),
        )
        .child(
            div()
                .font_weight(FontWeight::MEDIUM)
                .text_color(foreground)
                .child(title),
        )
        .child("·")
        .child(description)
        .into_any_element()
}

/// `.dcc-project-shortcut`: a card that starts a task in the project.
fn shortcut(
    view: &NewTaskView,
    index: usize,
    project: &Project,
    cx: &mut Context<NewTaskView>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let theme = cx.theme();
    let busy = view.busy();
    let pending = view.pending.as_deref() == Some(project.id.as_str());
    let edge = colors.border.opacity(0.8);
    let rest = mix_oklab(colors.sidebar, 0.55, colors.background);
    let (lift, ring) = (
        colors.popover,
        mix_oklab(colors.accent_calm, 0.35, colors.border),
    );
    let chosen = project.clone();
    let radius = theme.radius.xl;
    div()
        .id(("new-task-shortcut", index))
        .flex()
        .min_w_0()
        .items_center()
        .gap(px(12.0))
        .p(px(18.0))
        .border_1()
        .border_color(edge)
        .rounded_tl(px(16.0))
        .rounded_tr(px(16.0))
        .rounded_br(px(7.0))
        .rounded_bl(px(16.0))
        .bg(rest)
        .when(busy, |card| card.opacity(0.6))
        .when(!busy, |card| {
            card.cursor_pointer()
                .hover(move |style| style.bg(lift).border_color(ring).shadow(card_shadow()))
                .on_click(cx.listener(move |view, _, _, cx| view.select_project(&chosen, cx)))
        })
        .child(
            ProjectGlyph::new(
                project.icon.as_deref(),
                project.color.as_deref(),
                Some(project.root_path.as_str()),
            )
            .tile(36.0)
            .radius(radius),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .text_size(px(13.0))
                        .line_height(px(18.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(SharedString::from(project.display_name.clone())),
                )
                .child(
                    div()
                        .mt(px(4.0))
                        .truncate()
                        .text_size(px(11.0))
                        .line_height(px(15.0))
                        .text_color(colors.muted_foreground)
                        .child(SharedString::from(project.base_branch.clone())),
                ),
        )
        .children(
            view.linked_badge(project)
                .map(|count| linked_badge(count, cx)),
        )
        .child(if pending {
            div()
                .text_color(colors.muted_foreground)
                .child(spinner(("new-task-shortcut-busy", index), 16.0))
                .into_any_element()
        } else {
            Icon::new("arrow-up-right")
                .size(16.0)
                .color(colors.muted_foreground.opacity(0.6))
                .into_any_element()
        })
        .into_any_element()
}

impl Render for NewTaskView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        let busy = self.busy();
        let calm = colors.accent_calm;
        // `padding: clamp(28px, 7vh, 80px) 24px 36px`
        let top = (f32::from(window.viewport_size().height) * 0.07).clamp(28.0, 80.0);
        let mark = div()
            .mb(px(20.0))
            .flex()
            .size(px(52.0))
            .items_center()
            .justify_center()
            .rounded_tl(px(18.0))
            .rounded_tr(px(18.0))
            .rounded_br(px(7.0))
            .rounded_bl(px(18.0))
            .border_1()
            .border_color(mix_oklab(calm, 0.2, colors.border))
            .bg(mix_oklab(calm, 0.08, colors.popover))
            .shadow(card_shadow())
            .child(img(crate::assets::dcc_glyph()).size(px(24.0)));
        let worktree = option_row(
            "new-task-worktree",
            self.worktree,
            cx.t("newTask.execution.worktree"),
            if self.worktree {
                cx.t("newTask.execution.protectedDescription")
            } else {
                cx.t("newTask.execution.localDescription")
            },
            busy,
            |view, next| view.worktree = next,
            cx,
        );
        let linked = self.any_linked().then(|| {
            option_row(
                "new-task-linked",
                self.include_linked,
                cx.t("newTask.linkedFolders"),
                cx.t("newTask.linkedFoldersDescription"),
                busy,
                |view, next| view.include_linked = next,
                cx,
            )
        });
        let shortcuts: Vec<AnyElement> = self
            .projects()
            .iter()
            .take(4)
            .enumerate()
            .map(|(index, project)| shortcut(self, index, project, cx))
            .collect();
        let pairs: Vec<AnyElement> = {
            let mut cards = shortcuts.into_iter();
            let mut rows = Vec::new();
            while let Some(first) = cards.next() {
                let second = cards.next();
                rows.push(
                    div()
                        .flex()
                        .gap(px(12.0))
                        .child(div().flex_1().min_w_0().child(first))
                        .child(div().flex_1().min_w_0().children(second))
                        .into_any_element(),
                );
            }
            rows
        };
        div()
            .id("new-task")
            .debug_selector(|| "new-task".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .size_full()
            .overflow_y_scroll()
            .bg(colors.background)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .pt(px(top))
                    .px(px(24.0))
                    .pb(px(36.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .w_full()
                            .max_w(px(576.0))
                            .items_center()
                            .child(mark)
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .line_height(px(40.0))
                                    .child(Tracked::new(cx.t("newTask.title"), 32.0, -0.045)),
                            )
                            .child(
                                div()
                                    .mt(px(8.0))
                                    .max_w(px(384.0))
                                    .text_center()
                                    .text_size(px(14.0))
                                    .line_height(px(24.0))
                                    .text_color(colors.muted_foreground)
                                    .child(cx.t("newTask.designDescription")),
                            )
                            .child(project_menu(self, cx))
                            .child(div().mt(px(12.0)).child(worktree))
                            .children(linked)
                            .when(!pairs.is_empty(), |column| {
                                column.child(
                                    div()
                                        .mt(px(28.0))
                                        .flex()
                                        .flex_col()
                                        .gap(px(12.0))
                                        .w_full()
                                        .children(pairs),
                                )
                            }),
                    ),
            )
    }
}
