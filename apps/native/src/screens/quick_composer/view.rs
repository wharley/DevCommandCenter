//! How the quick composer looks: the header (⌘, "Compositor rápido",
//! refresh, close), the project and Local/Worktree, what that means, the
//! folder, reading or no projects, why it cannot send, the error, the last
//! launch (started, or interrupted with the prompt kept, copy and release),
//! the composer and the foot.

use gpui::{
    AnyElement, ClipboardItem, Context, FontWeight, IntoElement, KeyDownEvent, Render,
    SharedString, Window, div, prelude::*, px,
};

use super::QuickComposer;
use crate::i18n::Translate;
use crate::theme::{ActiveTheme, fonts};
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Icon, Menu, MenuEntry, MenuItem, Toaster, spinner,
};

impl QuickComposer {
    fn project_select(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let current = self
            .project()
            .map(|project| project.display_name.clone())
            .unwrap_or_else(|| cx.t("quickComposer.selectProject").to_string());
        let off = self.busy || self.loading || self.unfinished();
        let projects: Vec<(String, String)> = self
            .projects
            .iter()
            .map(|project| (project.project_id.clone(), project.display_name.clone()))
            .collect();
        let this = cx.entity().downgrade();
        if off {
            return div()
                .flex_1()
                .min_w_0()
                .h(px(36.0))
                .flex()
                .items_center()
                .rounded(px(8.0))
                .border_1()
                .border_color(colors.border)
                .px(px(8.0))
                .text_size(px(14.0))
                .opacity(0.5)
                .child(div().truncate().child(current))
                .into_any_element();
        }
        div()
            .flex_1()
            .min_w_0()
            .child(Menu::dropdown(
                "quick-project",
                move |_| {
                    div()
                        .debug_selector(|| "quick-project".into())
                        .h(px(36.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(6.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(colors.border)
                        .bg(colors.background)
                        .px(px(8.0))
                        .text_size(px(14.0))
                        .cursor_pointer()
                        .child(div().min_w_0().truncate().child(current.clone()))
                        .child(Icon::new("chevron-down").size(14.0))
                        .into_any_element()
                },
                move |_, _| {
                    projects
                        .iter()
                        .map(|(id, name)| {
                            let (this, id) = (this.clone(), id.clone());
                            MenuEntry::Item(MenuItem::new(name.clone(), move |window, cx| {
                                let _ = this.update(cx, |quick, cx| {
                                    quick.set_project(id.clone(), window, cx)
                                });
                            }))
                        })
                        .collect()
                },
            ))
            .into_any_element()
    }

    fn environment(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let off = self.busy || self.unfinished();
        let local = self.mode() == crate::data::quick_composer::LOCAL;
        let segment = |id: &'static str, chosen: bool, label: SharedString| {
            let fg = colors.foreground;
            div()
                .id(id)
                .debug_selector(move || id.to_string())
                .rounded(px(6.0))
                .px(px(12.0))
                .py(px(4.0))
                .text_size(px(12.0))
                .map(|button| {
                    if chosen {
                        button
                            .bg(colors.accent)
                            .text_color(colors.accent_foreground)
                    } else {
                        button
                            .text_color(colors.muted_foreground)
                            .hover(move |button| button.text_color(fg))
                    }
                })
                .when(off, |button| button.opacity(0.5))
                .when(!off, |button| button.cursor_pointer())
                .child(label)
        };
        div()
            .flex()
            .rounded(px(8.0))
            .border_1()
            .border_color(colors.border)
            .p(px(4.0))
            .child(
                segment("quick-local", local, cx.t("quickComposer.localDirect")).when(
                    !off,
                    |button| {
                        button.on_click(cx.listener(|quick, _, _, cx| quick.set_mode(true, cx)))
                    },
                ),
            )
            .child(
                segment(
                    "quick-worktree",
                    !local,
                    cx.t("quickComposer.protectedWorktree"),
                )
                .when(!off, |button| {
                    button.on_click(cx.listener(|quick, _, _, cx| quick.set_mode(false, cx)))
                }),
            )
            .into_any_element()
    }

    fn launch_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let launch = self.launch.clone()?;
        let completed = launch.phase == "completed";
        let shown = !completed
            || (!self.loading
                && self.visible_completed.as_deref() == Some(launch.id.as_str())
                && launch.request.project_id == self.selection.project_id);
        if !shown {
            return None;
        }
        let colors = cx.theme().colors.clone();
        let busy = self.busy;
        let prompt = launch.request.turn.raw_prompt.clone();
        let heading = cx.t(if busy {
            "quickComposer.starting"
        } else if completed {
            "quickComposer.started"
        } else {
            "quickComposer.interrupted"
        });
        let kept = (!busy && !completed).then(|| {
            let copied = prompt.clone();
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .text_color(colors.muted_foreground)
                        .child(cx.t("quickComposer.savedPrompt")),
                )
                .child(
                    div()
                        .id("quick-kept")
                        .max_h(px(96.0))
                        .overflow_y_scroll()
                        .child(prompt.clone()),
                )
                .child(
                    div().child(
                        Button::new("quick-copy")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Sm)
                            .text_size(12.0)
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()));
                                Toaster::success(cx.t("quickComposer.copied"), cx);
                            }))
                            .child(cx.t("quickComposer.copy")),
                    ),
                )
        });
        Some(
            div()
                .debug_selector(|| "quick-launch".into())
                .flex()
                .flex_col()
                .gap(px(8.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(colors.border)
                .bg(colors.muted.opacity(0.3))
                .p(px(12.0))
                .text_size(px(12.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .when(busy, |row| row.child(spinner("quick-starting", 14.0)))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(heading)
                                .when(completed, |text| {
                                    text.child(
                                        div()
                                            .mt(px(4.0))
                                            .truncate()
                                            .text_color(colors.muted_foreground)
                                            .child(prompt.clone()),
                                    )
                                }),
                        )
                        .child(
                            Button::new("quick-open-task")
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Sm)
                                .text_size(12.0)
                                .on_click(cx.listener(|quick, _, _, cx| quick.open_task(cx)))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(4.0))
                                        .child(cx.t("quickComposer.openTask"))
                                        .child(Icon::new("arrow-up-right").size(12.0)),
                                ),
                        ),
                )
                .children(
                    launch
                        .error
                        .clone()
                        .map(|error| div().text_color(colors.destructive).child(error)),
                )
                .children(kept)
                .when(self.unfinished() && !busy, |card| {
                    card.child(
                        div().child(
                            Button::new("quick-recover")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Sm)
                                .text_size(12.0)
                                .on_click(cx.listener(|quick, _, _, cx| quick.recover(cx)))
                                .child(cx.t("quickComposer.stopRecovery")),
                        ),
                    )
                })
                .into_any_element(),
        )
    }
}

impl Render for QuickComposer {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        let muted = colors.muted_foreground;
        let local = self.mode() == crate::data::quick_composer::LOCAL;
        let blocked = self.blocked(cx);
        let busy = self.busy;
        let header = div()
            .flex()
            .flex_none()
            .h(px(40.0))
            .items_center()
            .gap(px(8.0))
            .border_b_1()
            .border_color(colors.border.opacity(0.6))
            .px(px(20.0))
            .child(Icon::new("command").size(16.0).color(muted))
            .child(
                div()
                    .flex_1()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child(cx.t("quickComposer.title")),
            )
            .child(
                Button::new("quick-refresh")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Icon)
                    .disabled(busy)
                    .on_click(cx.listener(|quick, _, window, cx| quick.refresh(window, cx)))
                    .child(Icon::new("refresh-cw").size(14.0)),
            )
            .child(
                Button::new("quick-close")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Icon)
                    .on_click(cx.listener(|quick, _, _, cx| quick.close(cx)))
                    .child(Icon::new("x").size(16.0)),
            );
        let small = |text: SharedString| div().text_size(px(12.0)).text_color(muted).child(text);
        let body = div()
            .id("quick-body")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(px(12.0))
            .overflow_y_scroll()
            .px(px(20.0))
            .py(px(16.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(Icon::new("folder").size(16.0).color(muted))
                    .child(self.project_select(cx))
                    .child(self.environment(cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(12.0))
                    .text_color(muted)
                    .child(Icon::new("git-branch").size(14.0))
                    .child(cx.t(if local {
                        "quickComposer.localHint"
                    } else {
                        "quickComposer.worktreeHint"
                    })),
            )
            .children(self.project().map(|project| {
                div()
                    .truncate()
                    .font_family(fonts::MONO)
                    .text_size(px(11.0))
                    .text_color(muted)
                    .child(project.root_path.clone())
            }))
            .when(self.loading, |body| {
                body.child(small(cx.t("quickComposer.loading")))
            })
            .when(!self.loading && self.projects.is_empty(), |body| {
                body.child(
                    div()
                        .debug_selector(|| "quick-no-projects".into())
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_size(px(14.0))
                                .text_color(muted)
                                .child(cx.t("quickComposer.noProjects")),
                        )
                        .child(
                            div().child(
                                Button::new("quick-open-main")
                                    .variant(ButtonVariant::Outline)
                                    .size(ButtonSize::Sm)
                                    .on_click(cx.listener(|quick, _, _, cx| quick.open_task(cx)))
                                    .child(cx.t("quickComposer.openTask")),
                            ),
                        ),
                )
            })
            .when(
                !self.loading && self.project().is_some() && blocked,
                |body| {
                    body.child(
                        div()
                            .debug_selector(|| "quick-blocked".into())
                            .text_size(px(12.0))
                            .text_color(colors.destructive)
                            .child(cx.t("quickComposer.selectModel")),
                    )
                },
            )
            .children(self.error.clone().map(|error| {
                div()
                    .text_size(px(12.0))
                    .text_color(colors.destructive)
                    .child(error)
            }))
            .children(self.launch_card(cx))
            .child(
                div()
                    .debug_selector(|| "quick-composer-field".into())
                    .when(
                        busy || self.loading || self.project().is_none() || self.unfinished(),
                        |field| field.opacity(0.6),
                    )
                    .child(self.composer.clone()),
            )
            .child(
                div()
                    .flex()
                    .justify_center()
                    .text_size(px(11.0))
                    .text_color(muted)
                    .child(cx.t("quickComposer.footer")),
            );
        div()
            .id("quick-composer")
            .debug_selector(|| "quick-composer".into())
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|quick, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    quick.close(cx);
                }
            }))
            .size_full()
            .flex()
            .flex_col()
            .bg(colors.background)
            .text_color(colors.foreground)
            .child(header)
            .child(body)
    }
}
