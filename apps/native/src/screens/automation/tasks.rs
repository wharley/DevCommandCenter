//! "Tarefas do projeto": "Executar checks" and "Adicionar tarefa", then a
//! card per task (id, name, folder, kind, command, timeout, the hooks it
//! runs in) with run and remove.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::ProjectAutomation;
use super::view::{field, label};
use crate::data::automation::{Hook, Kind};
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Checkbox, Icon, Menu, MenuEntry, MenuItem, card_shadow,
    spinner,
};

fn cell(span: u16, title: SharedString, body: AnyElement, cx: &gpui::App) -> AnyElement {
    div()
        .col_span(span)
        .min_w_0()
        .child(label(title, cx))
        .child(div().mt(px(4.0)).child(body))
        .into_any_element()
}

impl ProjectAutomation {
    pub(super) fn tasks_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let busy = !self.running.is_empty();
        let dirty = self.dirty();
        let checks = self.draft.check_ids();
        let run_checks = Button::new("automation-run-checks")
            .variant(ButtonVariant::Outline)
            .size(ButtonSize::Sm)
            .disabled(checks.is_empty() || busy || dirty)
            .on_click(cx.listener(move |this, _, window, cx| this.run(checks.clone(), window, cx)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(Icon::new("play").size(14.0))
                    .child(cx.t("automation.runChecks")),
            );
        let run_checks: AnyElement = if dirty {
            crate::ui::Tooltip::text(
                "automation-run-checks-tip",
                run_checks,
                cx.t("automation.saveBeforeRun"),
            )
            .into_any_element()
        } else {
            run_checks.into_any_element()
        };
        let cards: Vec<AnyElement> = (0..self.draft.tasks.len())
            .map(|index| self.task_card(index, cx))
            .collect();
        let list: AnyElement = if cards.is_empty() {
            div()
                .rounded(px(14.0))
                .border_1()
                .border_dashed()
                .border_color(colors.border)
                .px(px(16.0))
                .py(px(32.0))
                .text_center()
                .text_size(px(14.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("automation.emptyTasks"))
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .children(cards)
                .into_any_element()
        };
        div()
            .child(
                div()
                    .mb(px(12.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(cx.t("automation.tasksTitle")),
                            )
                            .child(
                                div()
                                    .mt(px(4.0))
                                    .text_size(px(12.0))
                                    .text_color(colors.muted_foreground)
                                    .child(cx.t("automation.tasksDescription")),
                            ),
                    )
                    .child(
                        div().flex().gap(px(8.0)).child(run_checks).child(
                            Button::new("automation-add-task")
                                .size(ButtonSize::Sm)
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.add_task(window, cx)),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(6.0))
                                        .child(Icon::new("plus").size(14.0))
                                        .child(cx.t("automation.addTask")),
                                ),
                        ),
                    ),
            )
            .child(list)
            .into_any_element()
    }

    fn task_card(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let task = self.draft.tasks[index].clone();
        let Some(fields) = self.fields.get(index) else {
            return div().into_any_element();
        };
        let fix = task.kind == Kind::Fix;
        let busy = !self.running.is_empty();
        let dirty = self.dirty();
        let running = self.running.contains(&task.id);
        let (icon, tone) = if fix {
            ("wrench", tw::AMBER_600.hsla())
        } else {
            ("circle-check", tw::EMERALD_600.hsla())
        };
        let run_id = task.id.clone();
        let this = cx.entity().downgrade();
        let kind = Menu::dropdown(
            SharedString::from(format!("automation-kind-{index}")),
            {
                let colors = colors.clone();
                let label = cx.t(&format!("automation.kind.{}", task.kind.key()));
                move |open| {
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .w_full()
                        .h(px(36.0))
                        .px(px(8.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(if open { colors.ring } else { colors.input })
                        .text_size(px(12.0))
                        .child(label)
                        .child(
                            Icon::new("chevron-down")
                                .size(12.0)
                                .color(colors.muted_foreground),
                        )
                        .into_any_element()
                }
            },
            {
                let check = cx.t("automation.kind.check");
                let fix_label = cx.t("automation.kind.fix");
                move |_, _| {
                    [(Kind::Check, check.clone()), (Kind::Fix, fix_label.clone())]
                        .into_iter()
                        .map(|(kind, label)| {
                            let this = this.clone();
                            MenuEntry::Item(MenuItem::new(label, move |_, cx| {
                                let _ = this.update(cx, |this, cx| this.set_kind(index, kind, cx));
                            }))
                        })
                        .collect()
                }
            },
        );
        let hook = |hook: Hook, key: &str, cx: &mut Context<Self>| {
            let this = cx.entity().downgrade();
            let id = task.id.clone();
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .when(fix, |row| row.opacity(0.5).cursor_not_allowed())
                .child(
                    Checkbox::new(
                        SharedString::from(format!("automation-{key}-{index}")),
                        self.draft.hooked(hook, &task.id),
                    )
                    .disabled(fix)
                    .on_toggle(move |_, _, cx| {
                        let id = id.clone();
                        let _ = this.update(cx, |this, cx| {
                            this.draft.toggle_hook(hook, &id);
                            cx.notify();
                        });
                    }),
                )
                .child(cx.t(&format!("automation.{key}")))
        };
        let before_merge = hook(Hook::BeforeMerge, "beforeMerge", cx);
        let before_push = hook(Hook::BeforePush, "beforePush", cx);
        div()
            .debug_selector(move || format!("automation-task-{index}"))
            .rounded(px(14.0))
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.background)
            .shadow(card_shadow())
            .p(px(16.0))
            .child(
                div()
                    .mb(px(12.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .min_w_0()
                            .items_center()
                            .gap(px(8.0))
                            .child(Icon::new(icon).size(16.0).color(tone))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(14.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(task.title()),
                            )
                            .child(
                                div()
                                    .h(px(20.0))
                                    .px(px(8.0))
                                    .rounded(px(26.0))
                                    .border_1()
                                    .border_color(colors.border)
                                    .text_size(px(10.0))
                                    .child(cx.t(&format!("automation.kind.{}", task.kind.key()))),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .gap(px(4.0))
                            .child(
                                Button::new(SharedString::from(format!("automation-run-{index}")))
                                    .variant(ButtonVariant::Ghost)
                                    .size(ButtonSize::IconSm)
                                    .disabled(busy || dirty)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.run(vec![run_id.clone()], window, cx)
                                    }))
                                    .child(if running {
                                        spinner(
                                            SharedString::from(format!(
                                                "automation-running-{index}"
                                            )),
                                            16.0,
                                        )
                                        .into_any_element()
                                    } else {
                                        Icon::new("play").size(16.0).into_any_element()
                                    }),
                            )
                            .child(
                                Button::new(SharedString::from(format!(
                                    "automation-remove-{index}"
                                )))
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::IconSm)
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.remove_task(index, cx)),
                                )
                                .child(Icon::new("trash-2").size(16.0)),
                            ),
                    ),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(12)
                    .gap(px(12.0))
                    .child(cell(
                        3,
                        cx.t("automation.id"),
                        field(format!("automation-id-{index}"), &fields.id, true, cx),
                        cx,
                    ))
                    .child(cell(
                        4,
                        cx.t("automation.label"),
                        field(
                            format!("automation-label-{index}"),
                            &fields.label,
                            false,
                            cx,
                        ),
                        cx,
                    ))
                    .child(cell(
                        3,
                        cx.t("automation.cwd"),
                        field(format!("automation-cwd-{index}"), &fields.cwd, true, cx),
                        cx,
                    ))
                    .child(cell(
                        2,
                        cx.t("automation.kindLabel"),
                        kind.into_any_element(),
                        cx,
                    ))
                    .child(cell(
                        10,
                        cx.t("automation.command"),
                        field(
                            format!("automation-command-{index}"),
                            &fields.command,
                            true,
                            cx,
                        ),
                        cx,
                    ))
                    .child(cell(
                        2,
                        cx.t("automation.timeout"),
                        field(
                            format!("automation-timeout-{index}"),
                            &fields.timeout,
                            false,
                            cx,
                        ),
                        cx,
                    )),
            )
            .child(
                div()
                    .mt(px(12.0))
                    .flex()
                    .flex_wrap()
                    .gap(px(16.0))
                    .border_t_1()
                    .border_color(colors.border.opacity(0.5))
                    .pt(px(12.0))
                    .text_size(px(12.0))
                    .child(before_merge)
                    .child(before_push)
                    .when(fix, |row| {
                        row.child(
                            div()
                                .text_color(colors.muted_foreground)
                                .child(cx.t("automation.fixManualOnly")),
                        )
                    }),
            )
            .into_any_element()
    }
}
