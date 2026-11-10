//! The agent's modes in its editor (Monocode's and Compozy's per-agent
//! mode): "Permissão nas tarefas" under "Abrir tarefas", what the tasks it
//! opens may do without asking (`ToolAccess::tasks_approval`), and
//! "Permissão na conversa", what its own conversation may do on a message
//! the person sends (`ToolAccess::conversation_approval`; a wake still
//! asks). Perguntar is the default and is stored as nothing.

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

/// The mode as a turn takes it (`ask`, `auto`, `full_access`).
pub(crate) fn wire_mode(mode: Option<ProviderApprovalPolicy>) -> String {
    serde_json::to_value(mode.unwrap_or(ProviderApprovalPolicy::Ask))
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "ask".to_string())
}

/// Which mode a row sets.
#[derive(Clone, Copy)]
enum Mode {
    Tasks,
    Conversation,
}

impl AgentEditor {
    /// "Permissão na conversa", always there.
    pub(super) fn conversation_mode_row(&self, cx: &mut Context<Self>) -> AnyElement {
        self.mode_row(Mode::Conversation, cx)
    }

    /// "Permissão nas tarefas", while the agent opens tasks.
    pub(super) fn task_mode_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.tool_access.group("tasks") == dcc_core::domain::agent_access::AccessLevel::None {
            return None;
        }
        Some(self.mode_row(Mode::Tasks, cx))
    }

    fn mode_row(&self, which: Mode, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let (set, key, selector) = match which {
            Mode::Tasks => (
                self.tool_access.tasks_approval,
                "agents.access.mode.title",
                "agent-editor-task-mode",
            ),
            Mode::Conversation => (
                self.tool_access.conversation_approval,
                "agents.access.mode.conversation",
                "agent-editor-conversation-mode",
            ),
        };
        let current = set.unwrap_or(ProviderApprovalPolicy::Ask);
        let labels: Vec<SharedString> = vec![
            cx.t("agents.access.mode.ask"),
            cx.t("agents.access.mode.auto"),
            cx.t("agents.access.mode.full"),
        ];
        let picked = MODES.iter().position(|mode| *mode == current);
        let editor = cx.entity().downgrade();
        let group = ToggleGroup::new(selector, labels, picked).on_select(move |index, _, cx| {
            let mode = MODES[index];
            let _ = editor.update(cx, |editor, cx| {
                let kept = (mode != ProviderApprovalPolicy::Ask).then_some(mode);
                match which {
                    Mode::Tasks => editor.tool_access.tasks_approval = kept,
                    Mode::Conversation => editor.tool_access.conversation_approval = kept,
                }
                cx.notify();
            });
        });
        div()
            .debug_selector(move || selector.into())
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
                    .child(cx.t(key)),
            )
            .child(div().flex_none().child(group))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mode_goes_to_the_turn_as_its_wire_name() {
        assert_eq!(wire_mode(None), "ask");
        assert_eq!(
            wire_mode(Some(ProviderApprovalPolicy::FullAccess)),
            "full_access"
        );
        assert_eq!(wire_mode(Some(ProviderApprovalPolicy::Auto)), "auto");
    }
}
