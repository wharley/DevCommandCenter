//! `workspace-dialog.css` for the open mode: header with the mark, the
//! project grid, "Também nesta tarefa", the task setup (base branch, name),
//! the protection box and the footer.

use gpui::{AnyElement, App, Context, FontWeight, Hsla, SharedString, Window, div, prelude::*, px};

use super::OpenProjectDialog;
use crate::data::open_project::ProjectOption;
use crate::i18n::{Args, Translate};
use crate::project::ProjectGlyph;
use crate::theme::ActiveTheme;
use crate::theme::color::mix_oklab;
use crate::theme::tailwind as tw;
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Checkbox, Dialog, Icon, Menu, MenuEntry, MenuItem, spinner,
};

/// `--dcc-surface-edge`
fn edge(cx: &App) -> Hsla {
    cx.theme().colors.border.opacity(0.8)
}

fn label(text: SharedString) -> impl IntoElement {
    div()
        .text_size(px(12.0))
        .line_height(px(16.0))
        .font_weight(FontWeight::MEDIUM)
        .child(text)
}

fn hint(text: SharedString, cx: &App) -> impl IntoElement {
    div()
        .text_size(px(10.5))
        .line_height(px(15.0))
        .text_color(cx.theme().colors.muted_foreground)
        .child(text)
}

/// `.workspace-dialog-scroll input`: 34px, 9px corners, the surface edge.
fn field_box(cx: &App) -> gpui::Div {
    let colors = &cx.theme().colors;
    div()
        .flex()
        .h(px(34.0))
        .w_full()
        .min_w_0()
        .items_center()
        .rounded(px(9.0))
        .border_1()
        .border_color(edge(cx))
        .bg(colors.background)
        .px(px(10.0))
        .text_size(px(12.0))
}

fn base_preview(option: &ProjectOption, loading: bool, cx: &App) -> SharedString {
    if option.base_branch.is_empty() {
        if loading {
            return cx.t("workspaceDialog.loadingBranches");
        }
        return cx.t("workspaceDialog.linkedFolderBranch");
    }
    cx.t_with(
        "workspaceDialog.basePreview",
        &Args::default().with("branch", option.base_branch.clone()),
    )
}

/// "Projeto da tarefa": the listed projects (a picked folder first).
fn project_grid(dialog: &OpenProjectDialog, cx: &mut Context<OpenProjectDialog>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let busy = dialog.busy;
    let calm = colors.accent_calm;
    let cards: Vec<AnyElement> = dialog
        .model
        .single_options()
        .into_iter()
        .enumerate()
        .map(|(index, option)| {
            let selected = dialog.model.selected.as_deref() == Some(option.id.as_str());
            let loading = selected && dialog.model.loading_branches;
            let hover = colors.muted.opacity(0.4);
            let id = option.id.clone();
            div()
                .id(("open-project-option", index))
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(8.0))
                .rounded(px(8.0))
                .border_1()
                .px(px(10.0))
                .py(px(8.0))
                .map(|card| {
                    if selected {
                        card.border_color(mix_oklab(calm, 0.4, colors.border))
                            .bg(mix_oklab(calm, 0.07, colors.popover))
                    } else {
                        card.border_color(gpui::transparent_black())
                    }
                })
                .when(!busy && !selected, |card| {
                    card.hover(move |style| style.bg(hover))
                })
                .when(!busy, |card| {
                    card.cursor_pointer()
                        .on_click(cx.listener(move |dialog, _, _, cx| dialog.select(&id, cx)))
                })
                .child(
                    ProjectGlyph::new(
                        option.icon.as_deref(),
                        option.color.as_deref(),
                        Some(option.root_path.as_str()),
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
                                .text_size(px(11.0))
                                .line_height(px(15.0))
                                .font_weight(FontWeight::MEDIUM)
                                .child(SharedString::from(option.label.clone())),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_size(px(9.5))
                                .line_height(px(13.0))
                                .text_color(colors.muted_foreground)
                                .child(base_preview(&option, loading, cx)),
                        ),
                )
                .when(selected, |card| {
                    card.child(Icon::new("check").size(14.0).color(tw::EMERALD_600.hsla()))
                })
                .into_any_element()
        })
        .collect();
    let mut rows = Vec::new();
    let mut cards = cards.into_iter();
    while let Some(first) = cards.next() {
        rows.push(
            div()
                .flex()
                .gap(px(6.0))
                .child(div().flex_1().min_w_0().child(first))
                .child(div().flex_1().min_w_0().children(cards.next()))
                .into_any_element(),
        );
    }
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(label(cx.t("workspaceDialog.singleProjectLabel")))
                        .child(hint(cx.t("workspaceDialog.singleProjectDescription"), cx)),
                )
                .child(
                    Button::new("open-project-other-folder")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm)
                        .disabled(busy)
                        .muted()
                        .on_click(cx.listener(|dialog, _, _, cx| dialog.pick_folder(cx)))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .text_size(px(11.0))
                                .child(Icon::new("folder-open").size(14.0))
                                .child(cx.t("workspaceDialog.otherFolder")),
                        ),
                ),
        )
        .child(
            div()
                .id("open-project-grid")
                .flex()
                .flex_col()
                .gap(px(6.0))
                .max_h(px(144.0))
                .overflow_y_scroll()
                .rounded(px(12.0))
                .border_1()
                .border_color(colors.border.opacity(0.6))
                .bg(colors.muted.opacity(0.1))
                .p(px(8.0))
                .children(rows),
        )
        .into_any_element()
}

/// The "Caminho do repositório" field when no project is listed yet.
fn root_field(dialog: &OpenProjectDialog, cx: &mut Context<OpenProjectDialog>) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(label(cx.t("workspaceDialog.repositoryPath")))
                .child(
                    Button::new("open-project-choose-folder")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm)
                        .muted()
                        .disabled(dialog.busy)
                        .on_click(cx.listener(|dialog, _, _, cx| dialog.pick_folder(cx)))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .text_size(px(11.0))
                                .child(Icon::new("folder-open").size(14.0))
                                .child(cx.t("workspaceDialog.chooseFolder")),
                        ),
                ),
        )
        .child(field_box(cx).child(div().flex_1().min_w_0().child(dialog.root.clone())))
        .into_any_element()
}

/// A chip of the task: a project it brings (click takes it out) or a
/// context folder (click leaves it out).
fn chip(
    id: SharedString,
    text: SharedString,
    included: bool,
    context: bool,
    busy: bool,
    on_click: impl Fn(&mut OpenProjectDialog, &mut Context<OpenProjectDialog>) + 'static,
    cx: &mut Context<OpenProjectDialog>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    div()
        .id(id)
        .flex()
        .max_w_full()
        .items_center()
        .gap(px(6.0))
        .rounded_full()
        .border_1()
        .px(px(10.0))
        .py(px(4.0))
        .text_size(px(11.0))
        .font_weight(FontWeight::MEDIUM)
        .map(|chip| match (context, included) {
            (false, _) => chip
                .border_color(tw::EMERALD_500.alpha(0.3))
                .bg(tw::EMERALD_500.alpha(0.07))
                .child(
                    Icon::new("check")
                        .size(12.0)
                        .stroke(2.4)
                        .color(tw::EMERALD_600.hsla()),
                ),
            (true, true) => chip
                .border_color(colors.border)
                .bg(colors.muted.opacity(0.5))
                .child(Icon::new("folder-open").size(12.0)),
            (true, false) => chip
                .border_color(colors.border.opacity(0.7))
                .text_color(colors.muted_foreground)
                .line_through()
                .child(Icon::new("folder-open").size(12.0)),
        })
        .child(div().truncate().child(text))
        .when(context, |chip| {
            chip.child(
                div()
                    .flex_shrink_0()
                    .text_size(px(10.0))
                    .font_weight(FontWeight::NORMAL)
                    .text_color(colors.muted_foreground)
                    .child(cx.t("workspaceDialog.contextFolder")),
            )
        })
        .when(!busy, |chip| {
            chip.cursor_pointer()
                .on_click(cx.listener(move |dialog, _, _, cx| {
                    on_click(dialog, cx);
                    cx.notify();
                }))
        })
        .into_any_element()
}

/// "Também nesta tarefa": the other projects, the context folders, the
/// project list to add from and "Lembrar estas pastas".
fn also_in_task(
    dialog: &OpenProjectDialog,
    primary: &ProjectOption,
    cx: &mut Context<OpenProjectDialog>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let busy = dialog.busy;
    let extras = dialog.model.extra_options();
    let contexts = dialog.model.primary_context_folders();
    let mut chips: Vec<AnyElement> = Vec::new();
    for (index, option) in extras.iter().enumerate() {
        let id = option.id.clone();
        chips.push(chip(
            SharedString::from(format!("open-project-extra-{index}")),
            option.label.clone().into(),
            true,
            false,
            busy,
            move |dialog, _| dialog.model.toggle_extra(&id),
            cx,
        ));
    }
    for (index, path) in contexts.iter().enumerate() {
        let included = dialog.model.included_context.contains(path);
        let toggle = path.clone();
        let name = dcc_core::domain::repository::path_leaf(path).unwrap_or_else(|| path.clone());
        chips.push(chip(
            SharedString::from(format!("open-project-context-{index}")),
            name.into(),
            included,
            true,
            busy,
            move |dialog, _| dialog.model.toggle_context(&toggle),
            cx,
        ));
    }
    let list = dialog.model.adding.then(|| {
        let rows: Vec<AnyElement> = dialog
            .model
            .multi_options()
            .into_iter()
            .filter(|option| option.id != primary.id)
            .enumerate()
            .map(|(index, option)| {
                let checked = dialog.model.extras.contains(&option.id);
                let id = option.id.clone();
                let hover = colors.muted.opacity(0.4);
                div()
                    .id(("open-project-add", index))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .rounded(px(8.0))
                    .border_1()
                    .px(px(10.0))
                    .py(px(6.0))
                    .map(|row| {
                        if checked {
                            row.border_color(tw::EMERALD_500.alpha(0.3))
                                .bg(tw::EMERALD_500.alpha(0.055))
                        } else {
                            row.border_color(gpui::transparent_black())
                                .hover(move |style| style.bg(hover))
                        }
                    })
                    .when(!busy, |row| {
                        row.cursor_pointer()
                            .on_click(cx.listener(move |dialog, _, _, cx| {
                                dialog.model.toggle_extra(&id);
                                cx.notify();
                            }))
                    })
                    .child(
                        div()
                            .flex()
                            .size(px(20.0))
                            .flex_shrink_0()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.0))
                            .border_1()
                            .map(|mark| {
                                if checked {
                                    mark.border_color(tw::EMERALD_500.alpha(0.4))
                                        .bg(tw::EMERALD_500.alpha(0.15))
                                        .child(
                                            Icon::new("check")
                                                .size(12.0)
                                                .stroke(2.4)
                                                .color(tw::EMERALD_600.hsla()),
                                        )
                                } else {
                                    mark.border_color(colors.border.opacity(0.7))
                                        .bg(colors.background)
                                }
                            }),
                    )
                    .child(ProjectGlyph::new(
                        option.icon.as_deref(),
                        option.color.as_deref(),
                        Some(option.root_path.as_str()),
                    ))
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
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(SharedString::from(option.label.clone())),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(6.0))
                                    .text_size(px(10.0))
                                    .text_color(colors.muted_foreground)
                                    .child(
                                        div()
                                            .font_family(cx.theme().fonts.mono.clone())
                                            .child(SharedString::from(option.root_path.clone())),
                                    )
                                    .child("·")
                                    .child(base_preview(&option, false, cx)),
                            ),
                    )
                    .into_any_element()
            })
            .collect();
        div()
            .id("open-project-add-list")
            .flex()
            .flex_col()
            .gap(px(4.0))
            .max_h(px(176.0))
            .overflow_y_scroll()
            .rounded(px(12.0))
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.muted.opacity(0.1))
            .p(px(8.0))
            .children(rows)
    });
    let can_remember = dialog.model.can_remember();
    let remember = dialog.model.remember;
    let entity = cx.entity().downgrade();
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .min_w_0()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .child(label(cx.t("workspaceDialog.alsoInTask")))
                        .child(hint(cx.t("workspaceDialog.alsoInTaskDescription"), cx)),
                )
                .child(
                    Button::new("open-project-add-toggle")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm)
                        .muted()
                        .disabled(busy)
                        .on_click(cx.listener(|dialog, _, _, cx| {
                            dialog.model.adding = !dialog.model.adding;
                            cx.notify();
                        }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .text_size(px(11.0))
                                .child(Icon::new("plus").size(14.0))
                                .child(cx.t("workspaceDialog.addProject")),
                        ),
                ),
        )
        .child(if chips.is_empty() {
            div()
                .text_size(px(11.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("workspaceDialog.onlyThisProject"))
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_wrap()
                .gap(px(6.0))
                .min_w_0()
                .children(chips)
                .into_any_element()
        })
        .children(list)
        .when(can_remember, |section| {
            section.child(
                div()
                    .id("open-project-remember")
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(11.5))
                    .text_color(colors.muted_foreground)
                    .cursor_pointer()
                    .on_click(cx.listener(|dialog, _, _, cx| {
                        dialog.model.remember = !dialog.model.remember;
                        cx.notify();
                    }))
                    .child(
                        Checkbox::new("open-project-remember-box", remember)
                            .side(14.0)
                            .disabled(busy)
                            .on_toggle(move |next, _, cx| {
                                let _ = entity.update(cx, |dialog, cx| {
                                    dialog.model.remember = next;
                                    cx.notify();
                                });
                            }),
                    )
                    .child(cx.t("workspaceDialog.rememberLinkedFolders")),
            )
        })
        .into_any_element()
}

/// "Configuração da tarefa": base branch and name.
fn task_setup(dialog: &OpenProjectDialog, cx: &mut Context<OpenProjectDialog>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let model = &dialog.model;
    let disabled = dialog.busy || model.loading_branches || model.branches.is_empty();
    let shown: SharedString = if model.loading_branches {
        cx.t("workspaceDialog.loadingBranches")
    } else if model.branches.is_empty() {
        cx.t("workspaceDialog.chooseFolderForBranches")
    } else {
        model.base_branch.clone().into()
    };
    let branches = model.branches.clone();
    let entity = cx.entity().downgrade();
    let muted = colors.muted_foreground;
    let trigger_box = field_box(cx)
        .justify_between()
        .when(disabled, |field| field.opacity(0.5))
        .when(!disabled, |field| field.cursor_pointer())
        .child(div().truncate().child(shown))
        .child(Icon::new("chevron-down").size(14.0).color(muted));
    let select: AnyElement = if disabled {
        trigger_box.into_any_element()
    } else {
        Menu::dropdown(
            "open-project-branch",
            move |_| trigger_box.into_any_element(),
            move |_, _| {
                branches
                    .iter()
                    .map(|branch| {
                        let (entity, chosen) = (entity.clone(), branch.clone());
                        MenuEntry::from(MenuItem::new(branch.clone(), move |_, cx| {
                            let _ = entity.update(cx, |dialog, cx| {
                                dialog.model.base_branch = chosen.clone();
                                cx.notify();
                            });
                        }))
                    })
                    .collect()
            },
        )
        .min_width(260.0)
        .into_any_element()
    };
    let calm = colors.accent_calm;
    div()
        .flex()
        .flex_col()
        .gap(px(14.0))
        .p(px(16.0))
        .border_1()
        .border_color(edge(cx))
        .rounded(px(12.0))
        .bg(mix_oklab(colors.muted, 0.2, colors.popover))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .child(Icon::new("git-branch").size(15.0).color(calm))
                .child(cx.t("workspaceDialog.design.taskSetup")),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(label(cx.t("workspaceDialog.baseBranch")))
                .child(select),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .flex()
                        .gap(px(4.0))
                        .child(label(cx.t("workspaceDialog.displayName")))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(colors.muted_foreground)
                                .child(format!("({})", cx.t("workspaceDialog.optional"))),
                        ),
                )
                .child(field_box(cx).child(div().flex_1().min_w_0().child(dialog.name.clone()))),
        )
        .into_any_element()
}

/// `.workspace-dialog-protection`: what the task creates, and the choice.
fn protection(dialog: &OpenProjectDialog, cx: &mut Context<OpenProjectDialog>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let model = &dialog.model;
    let local = !model.worktree;
    let count = Args::count(model.worktree_count() as i64);
    let calm = colors.accent_calm;
    let (tile_bg, tile_fg, icon) = if local {
        (
            tw::AMBER_500.alpha(0.12),
            tw::AMBER_600.hsla(),
            "folder-open",
        )
    } else {
        (
            tw::EMERALD_500.alpha(0.12),
            tw::EMERALD_600.hsla(),
            "shield-check",
        )
    };
    let branch = (!model.base_branch.trim().is_empty()).then(|| model.base_branch.clone());
    let entity = cx.entity().downgrade();
    let worktree = model.worktree;
    let busy = dialog.busy;
    div()
        .flex()
        .flex_wrap()
        .items_start()
        .gap(px(12.0))
        .p(px(14.0))
        .border_1()
        .border_color(mix_oklab(calm, 0.25, colors.border))
        .bg(mix_oklab(calm, 0.05, colors.popover))
        .rounded(px(12.0))
        .child(
            div()
                .flex()
                .size(px(36.0))
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .rounded(px(8.0))
                .bg(tile_bg)
                .child(Icon::new(icon).size(18.0).stroke(1.9).color(tile_fg)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(if local {
                            cx.t_with("workspaceDialog.localPreview", &count)
                        } else {
                            cx.t_with("workspaceDialog.protectionPreview", &count)
                        }),
                )
                .child(
                    div()
                        .mt(px(4.0))
                        .text_size(px(10.5))
                        .line_height(px(16.0))
                        .text_color(colors.muted_foreground)
                        .child(if local {
                            cx.t("workspaceDialog.localDescription")
                        } else {
                            cx.t("workspaceDialog.protectionDescription")
                        }),
                )
                .child(
                    div()
                        .id("open-project-worktree")
                        .mt(px(8.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .text_size(px(11.5))
                        .text_color(colors.muted_foreground)
                        .when(!busy, |row| {
                            row.cursor_pointer()
                                .on_click(cx.listener(|dialog, _, _, cx| {
                                    dialog.model.worktree = !dialog.model.worktree;
                                    cx.notify();
                                }))
                        })
                        .child(
                            Checkbox::new("open-project-worktree-box", worktree)
                                .side(14.0)
                                .disabled(busy)
                                .on_toggle(move |next, _, cx| {
                                    let _ = entity.update(cx, |dialog, cx| {
                                        dialog.model.worktree = next;
                                        cx.notify();
                                    });
                                }),
                        )
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(colors.foreground)
                                .child(cx.t("newTask.execution.worktree")),
                        )
                        .child("·")
                        .child(if worktree {
                            cx.t("newTask.execution.protectedDescription")
                        } else {
                            cx.t("newTask.execution.localDescription")
                        }),
                ),
        )
        .children(branch.map(|branch| {
            div()
                .flex()
                .items_start()
                .gap(px(5.0))
                .max_w_full()
                .px(px(8.0))
                .py(px(5.0))
                .border_1()
                .border_color(edge(cx))
                .rounded(px(7.0))
                .bg(colors.background)
                .text_size(px(10.0))
                .text_color(colors.muted_foreground)
                .child(Icon::new("git-branch").size(12.0).stroke(1.8))
                .child(cx.t_with(
                    "workspaceDialog.basePreview",
                    &Args::default().with("branch", branch),
                ))
        }))
        .into_any_element()
}

impl Render for OpenProjectDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        let calm = colors.accent_calm;
        let busy = self.busy;
        let primary = self.model.primary();
        let listed = !self.model.single_options().is_empty();
        let height = f32::from(window.viewport_size().height) - 32.0;
        let entity = cx.entity().downgrade();
        let header = div()
            .flex()
            .flex_shrink_0()
            .flex_col()
            .gap(px(14.0))
            .p(px(24.0))
            .border_b_1()
            .border_color(edge(cx))
            .bg(mix_oklab(calm, 0.07, colors.popover))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(13.0))
                    .pr(px(18.0))
                    .child(
                        div()
                            .flex()
                            .size(px(44.0))
                            .flex_shrink_0()
                            .items_center()
                            .justify_center()
                            .rounded(px(13.0))
                            .bg(calm.opacity(0.1))
                            .child(Icon::new("folder-open").size(22.0).color(calm)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(18.0))
                                    .line_height(px(25.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(if self.multi {
                                        cx.t("workspaceDialog.createTitle")
                                    } else {
                                        cx.t("workspaceDialog.design.openTitle")
                                    }),
                            )
                            .child(
                                div()
                                    .mt(px(5.0))
                                    .text_size(px(12.0))
                                    .line_height(px(19.0))
                                    .text_color(colors.muted_foreground)
                                    .child(cx.t("workspaceDialog.createDescription")),
                            ),
                    ),
            );
        let body = div()
            .id("open-project-scroll")
            .flex()
            .flex_col()
            .gap(px(18.0))
            .min_h_0()
            .flex_1()
            .overflow_y_scroll()
            .px(px(24.0))
            .py(px(20.0))
            .when(listed, |body| body.child(project_grid(self, cx)))
            .when(self.model.selected.is_none(), |body| {
                body.child(root_field(self, cx))
            })
            .children(
                primary
                    .as_ref()
                    .map(|primary| also_in_task(self, primary, cx)),
            )
            .child(task_setup(self, cx))
            .child(protection(self, cx));
        let submit_label = if busy {
            cx.t("workspaceDialog.creating")
        } else if self.model.brings_projects() {
            cx.t("workspaceDialog.multiSubmit")
        } else {
            cx.t("workspaceDialog.createSubmit")
        };
        let footer = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_end()
            .gap(px(8.0))
            .px(px(24.0))
            .py(px(14.0))
            .border_t_1()
            .border_color(edge(cx))
            .bg(mix_oklab(colors.muted, 0.25, colors.popover))
            .child(
                Button::new("open-project-cancel")
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::Sm)
                    .disabled(busy)
                    .on_click(cx.listener(|dialog, _, _, cx| dialog.close(cx)))
                    .child(cx.t("workspaceDialog.cancel")),
            )
            .child(
                Button::new("open-project-submit")
                    .size(ButtonSize::Sm)
                    .disabled(busy || !self.model.can_submit())
                    .on_click(cx.listener(|dialog, _, _, cx| dialog.submit(cx)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .when(busy, |label| {
                                label.child(spinner("open-project-creating", 16.0))
                            })
                            .child(submit_label),
                    ),
            );
        Dialog::new("open-project-dialog", move |_, cx| {
            let _ = entity.update(cx, |dialog, cx| dialog.close(cx));
        })
        .max_width(660.0)
        .max_height(height)
        .padding(0.0)
        .gap(0.0)
        .close_button(!busy)
        .child(
            div()
                .debug_selector(|| "open-project".into())
                .flex()
                .flex_col()
                .min_h_0()
                .max_h(px(height))
                .child(header)
                .child(body)
                .child(footer),
        )
    }
}
