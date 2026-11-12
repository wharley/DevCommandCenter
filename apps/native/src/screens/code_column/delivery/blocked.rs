//! A push the project's before-push checks stopped: instead of a toast
//! ("Push bloqueado"), a dialog with each check, what it printed, and
//! "Pedir ao agente para corrigir", which hands the failures to the task's
//! agent in the composer.

use dcc_core::domain::merge_conflict::WorkspaceGitValidationReport;
use gpui::{AnyElement, Context, FontWeight, Window, div, prelude::*, px};

use super::super::{CodeColumn, CodeColumnEvent};
use crate::data::desktop_api::ApiError;
use crate::i18n::Translate;
use crate::screens::validation_report::report_card;
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Dialog, dialog_footer};

/// What the agent gets of each failed check's output: its end, where the
/// error usually is.
const OUTPUT_TAIL_CHARS: usize = 4_000;

/// The request to the agent: every failed check with its exit code and the
/// end of what it printed.
pub(crate) fn fix_prompt(report: &WorkspaceGitValidationReport) -> String {
    let mut prompt = String::from(
        "The project's before-push checks failed, so the push did not run. Fix what they report, run the same commands again until they pass, and then tell me.\n",
    );
    for step in report.steps.iter().filter(|step| !step.success) {
        let exit = match (step.timed_out, step.exit_code) {
            (true, _) => "timed out".to_string(),
            (false, Some(code)) => format!("exit {code}"),
            (false, None) => "failed".to_string(),
        };
        prompt.push_str(&format!("\n`{}` ({exit}):\n", step.command));
        let output = step.output.trim();
        if !output.is_empty() {
            let chars = output.chars().count();
            let tail: String = output
                .chars()
                .skip(chars.saturating_sub(OUTPUT_TAIL_CHARS))
                .collect();
            prompt.push_str("```\n");
            prompt.push_str(&tail);
            prompt.push_str("\n```\n");
        }
    }
    prompt
}

impl CodeColumn {
    /// The checks stopped the push: their report, in a dialog.
    pub(crate) fn show_blocked(
        &mut self,
        report: Box<WorkspaceGitValidationReport>,
        cx: &mut Context<Self>,
    ) {
        self.delivery.blocked = Some(report);
        cx.notify();
    }

    /// A step that failed: the checks' report when they stopped the push,
    /// else the dock's toast.
    pub(super) fn delivery_failed(&mut self, error: ApiError, cx: &mut Context<Self>) {
        match error {
            ApiError::Refused {
                code,
                report: Some(report),
                ..
            } if code == "checks_failed" => self.show_blocked(report, cx),
            error => super::dock::dock_failed(error, cx),
        }
    }

    fn close_blocked(&mut self, cx: &mut Context<Self>) {
        self.delivery.blocked = None;
        cx.notify();
    }

    pub(crate) fn ask_agent_to_fix_checks(&mut self, cx: &mut Context<Self>) {
        let Some(report) = self.delivery.blocked.take() else {
            return;
        };
        cx.emit(CodeColumnEvent::AskAgent {
            prompt: fix_prompt(&report),
            delivered: cx.t("delivery.checksBlocked.sentToAgent"),
        });
        cx.notify();
    }
}

pub(super) fn blocked_dialog(
    column: &CodeColumn,
    _window: &Window,
    cx: &mut Context<CodeColumn>,
) -> Option<AnyElement> {
    let report = column.delivery.blocked.as_ref()?;
    let colors = cx.theme().colors.clone();
    let header = div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(16.0))
                .line_height(px(16.0))
                .font_weight(FontWeight::MEDIUM)
                .child(cx.t("delivery.checksBlocked.title")),
        )
        .child(
            div()
                .text_size(px(12.0))
                .line_height(px(19.5))
                .text_color(colors.muted_foreground)
                .child(cx.t("delivery.checksBlocked.description")),
        );
    let body = div()
        .id("delivery-blocked-report")
        .max_h(px(420.0))
        .overflow_y_scroll()
        .child(report_card(report, cx));
    let footer = dialog_footer(
        [
            Button::new("delivery-blocked-close")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Default)
                .child(cx.t("delivery.checksBlocked.close"))
                .on_click(cx.listener(|column, _, _, cx| column.close_blocked(cx)))
                .into_any_element(),
            Button::new("delivery-blocked-ask")
                .variant(ButtonVariant::Default)
                .size(ButtonSize::Default)
                .child(cx.t("delivery.checksBlocked.askAgent"))
                .on_click(cx.listener(|column, _, _, cx| column.ask_agent_to_fix_checks(cx)))
                .into_any_element(),
        ],
        cx,
    );
    let close = cx.entity().downgrade();
    Some(
        Dialog::new("delivery-blocked-dialog", move |_: &mut Window, cx| {
            let _ = close.update(cx, |column, cx| column.close_blocked(cx));
        })
        .close_button(false)
        .child(header)
        .child(body)
        .child(footer)
        .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::merge_conflict::{
        WorkspaceGitValidationStatus, WorkspaceGitValidationStep,
    };

    fn step(command: &str, success: bool, output: &str) -> WorkspaceGitValidationStep {
        WorkspaceGitValidationStep {
            command: command.into(),
            success,
            exit_code: Some(if success { 0 } else { 1 }),
            output: output.into(),
            timed_out: false,
            duration_ms: 1200,
            truncated: false,
        }
    }

    #[test]
    fn the_agent_gets_only_the_failed_checks_with_the_end_of_their_output() {
        let long = format!("{}FIM", "x".repeat(OUTPUT_TAIL_CHARS + 50));
        let report = WorkspaceGitValidationReport {
            status: WorkspaceGitValidationStatus::Failed,
            source_path: None,
            steps: vec![
                step("npm run lint", true, "ok"),
                step("npm test", false, &long),
            ],
        };
        let prompt = fix_prompt(&report);
        assert!(!prompt.contains("npm run lint"));
        assert!(prompt.contains("`npm test` (exit 1):"));
        assert!(prompt.contains("FIM\n```"));
        assert!(prompt.chars().count() < OUTPUT_TAIL_CHARS + 400);
    }
}
