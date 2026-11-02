//! `EditFromHereDialog`: how many turns leave, how the agent follows, the
//! files those turns changed (restorable, with the first eight named, or why
//! not), and the choices. Restoring files is its own button: cancelling or
//! going back without restoring never touches a file.

use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, prelude::*, px};

use super::super::ConversationView;
use crate::data::rewind::{self, Files};
use crate::i18n::{Args, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, fonts};
use crate::ui::{Button, ButtonVariant, Dialog, Icon, dialog_footer, dialog_header};

const VISIBLE_FILES: usize = 8;

fn section(label: SharedString, body: impl IntoElement, cx: &gpui::App) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .font_weight(FontWeight::MEDIUM)
                .text_color(cx.theme().colors.foreground)
                .child(label),
        )
        .child(body)
        .into_any_element()
}

impl ConversationView {
    pub(in super::super) fn rewind_dialog(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let plan = self.rewind.target.clone()?;
        let colors = cx.theme().colors.clone();
        let working = self.rewind.working;
        let in_project = |project: &str, cx: &gpui::App| {
            cx.t_with(
                "conversation.editFromHere.inProject",
                &Args::default().with("project", project.to_string()),
            )
        };
        let files = match rewind::files(&plan) {
            Files::None => div()
                .text_color(colors.muted_foreground)
                .child(cx.t("conversation.editFromHere.filesNone"))
                .into_any_element(),
            Files::Blocked {
                reason_key,
                project,
            } => div()
                .flex()
                .gap(px(8.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(tw::AMBER_500.hsla().opacity(0.4))
                .bg(tw::AMBER_500.hsla().opacity(0.05))
                .p(px(10.0))
                .text_color(tw::AMBER_700.hsla())
                .child(Icon::new("triangle-alert").size(14.0))
                .child(div().flex_1().child(format!(
                        "{}{}",
                        project
                            .map(|project| format!("{} ", in_project(project, cx)))
                            .unwrap_or_default(),
                        cx.t_with(
                            "conversation.editFromHere.filesBlocked",
                            &Args::default().with("reason", cx.t(&reason_key).to_string()),
                        )
                    )))
                .into_any_element(),
            Files::Restorable { file_count, files } => div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(div().text_color(colors.muted_foreground).child(cx.t_with(
                    "conversation.editFromHere.filesRestorable",
                    &Args::default().with_count(file_count as i64),
                )))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .font_family(fonts::MONO)
                        .text_size(px(11.0))
                        .text_color(colors.muted_foreground)
                        .children(files.iter().take(VISIBLE_FILES).map(|file| {
                            div().truncate().child(match &file.project {
                                Some(project) => format!("{project} · {}", file.display_path),
                                None => file.display_path.clone(),
                            })
                        }))
                        .when(files.len() > VISIBLE_FILES, |list| {
                            list.child(cx.t_with(
                                "conversation.editFromHere.filesMore",
                                &Args::default().with_count((files.len() - VISIBLE_FILES) as i64),
                            ))
                        }),
                )
                .into_any_element(),
        };
        let restorable = match rewind::files(&plan) {
            Files::Restorable { file_count, .. } => Some(file_count),
            _ => None,
        };
        let stopped = self.rewind.stopped.clone().map(|stopped| {
            let tone = if stopped.recovery_required {
                colors.destructive
            } else {
                tw::AMBER_700.hsla()
            };
            div()
                .debug_selector(|| "edit-from-here-stopped".into())
                .flex()
                .flex_col()
                .gap(px(4.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(tone.opacity(0.4))
                .bg(tone.opacity(0.05))
                .p(px(12.0))
                .text_color(tone)
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .child(cx.t("conversation.editFromHere.stoppedTitle")),
                )
                .children(
                    stopped
                        .project
                        .as_deref()
                        .map(|project| in_project(project, cx)),
                )
                .child(cx.t(&stopped.result_key))
                .children(stopped.reason_key.as_deref().map(|key| cx.t(key)))
                .when(stopped.restored_turns > 0, |box_| {
                    box_.child(cx.t_with(
                        "conversation.editFromHere.stoppedRestored",
                        &Args::default().with_count(stopped.restored_turns as i64),
                    ))
                })
        });
        let has_stopped = stopped.is_some();
        let mut actions = vec![
            Button::new("edit-from-here-cancel")
                .variant(ButtonVariant::Outline)
                .disabled(working)
                .on_click(cx.listener(|view, _, _, cx| view.cancel_rewind(cx)))
                .child(cx.t(if has_stopped {
                    "conversation.editFromHere.close"
                } else {
                    "conversation.editFromHere.cancel"
                }))
                .into_any_element(),
        ];
        if !has_stopped {
            actions.push(
                Button::new("edit-from-here-keep")
                    .variant(if restorable.is_some() {
                        ButtonVariant::Outline
                    } else {
                        ButtonVariant::Default
                    })
                    .disabled(working)
                    .on_click(cx.listener(|view, _, _, cx| view.confirm_rewind(false, cx)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(Icon::new("undo-2").size(16.0))
                            .child(cx.t(if restorable.is_some() {
                                "conversation.editFromHere.keepFiles"
                            } else {
                                "conversation.editFromHere.rewind"
                            })),
                    )
                    .into_any_element(),
            );
            if let Some(count) = restorable {
                actions.push(
                    Button::new("edit-from-here-restore")
                        .disabled(working)
                        .on_click(cx.listener(|view, _, _, cx| view.confirm_rewind(true, cx)))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(Icon::new("rotate-ccw").size(16.0))
                                .child(cx.t_with(
                                    "conversation.editFromHere.restoreFiles",
                                    &Args::default().with_count(count as i64),
                                )),
                        )
                        .into_any_element(),
                );
            }
        }
        let view = cx.entity().downgrade();
        Some(
            Dialog::new("edit-from-here-dialog", move |_: &mut Window, cx| {
                let _ = view.update(cx, |view, cx| view.cancel_rewind(cx));
            })
            .max_width(576.0)
            .close_button(!working)
            .child(dialog_header(
                cx.t("conversation.editFromHere.title"),
                Some(cx.t_with(
                    "conversation.editFromHere.summary",
                    &Args::default().with_count(plan.removed_turn_ids.len() as i64),
                )),
                cx,
            ))
            .child(
                div()
                    .debug_selector(|| "edit-from-here-body".into())
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .text_size(px(12.0))
                    .child(section(
                        cx.t("conversation.editFromHere.contextLabel"),
                        div()
                            .text_color(colors.muted_foreground)
                            .child(cx.t(rewind::context_key(&plan))),
                        cx,
                    ))
                    .child(section(
                        cx.t("conversation.editFromHere.filesLabel"),
                        files,
                        cx,
                    ))
                    .when(working, |body| {
                        body.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .text_color(colors.muted_foreground)
                                .child(crate::ui::spinner("edit-from-here-working", 14.0))
                                .child(cx.t("conversation.editFromHere.working")),
                        )
                    })
                    .children(stopped),
            )
            .child(dialog_footer(actions, cx))
            .into_any_element(),
        )
    }
}
