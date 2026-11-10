//! "Permissão nas tarefas" under "Abrir tarefas" in the agent's editor: what
//! the tasks it opens may do without asking (`ToolAccess::tasks_approval`,
//! Monocode's and Compozy's per-agent mode). Perguntar is the default and
//! is stored as nothing.

use dcc_core::domain::provider::ProviderApprovalPolicy;
use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

use super::editor::AgentEditor;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::ToggleGroup;

const MODES: [ProviderApprovalPolicy; 3] = [
    ProviderApprovalPolicy::Ask,
    ProviderApprovalPolicy::Auto,
    ProviderApprovalPolicy::FullAccess,
];

impl AgentEditor {
    /// The row, while the agent opens tasks.
    pub(super) fn task_mode_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.tool_access.group("tasks") == dcc_core::domain::agent_access::AccessLevel::None {
            return None;
        }
        let colors = cx.theme().colors.clone();
        let current = self
            .tool_access
            .tasks_approval
            .unwrap_or(ProviderApprovalPolicy::Ask);
        let labels: Vec<SharedString> = vec![
            cx.t("agents.access.mode.ask"),
            cx.t("agents.access.mode.auto"),
            cx.t("agents.access.mode.full"),
        ];
        let picked = MODES.iter().position(|mode| *mode == current);
        let editor = cx.entity().downgrade();
        let group = ToggleGroup::new("agent-editor-task-mode", labels, picked).on_select(
            move |index, _, cx| {
                let mode = MODES[index];
                let _ = editor.update(cx, |editor, cx| {
                    editor.tool_access.tasks_approval =
                        (mode != ProviderApprovalPolicy::Ask).then_some(mode);
                    cx.notify();
                });
            },
        );
        Some(
            div()
                .debug_selector(|| "agent-editor-task-mode".into())
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .pl(px(12.0))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(12.5))
                        .text_color(colors.muted_foreground)
                        .child(cx.t("agents.access.mode.title")),
                )
                .child(div().flex_none().child(group))
                .into_any_element(),
        )
    }
}
