//! "Limite mensal" in the agent's editor (`dcc_core::domain::agent_budget`):
//! none by default, or a number of tokens a month; past it, a turn of the
//! agent waits for the person's "Continuar mesmo assim".

use dcc_core::domain::agent_budget::LIMIT_CHOICES;
use gpui::{AnyElement, Context, SharedString};

use super::editor::{AgentEditor, field};
use crate::i18n::Translate;
use crate::ui::ToggleGroup;

/// `2M`, `500k`.
pub(crate) fn tokens_short(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        let millions = tokens as f64 / 1_000_000.0;
        if (millions - millions.round()).abs() < 0.05 {
            format!("{}M", millions.round() as u64)
        } else {
            format!("{millions:.1}M")
        }
    } else if tokens >= 1_000 {
        format!("{}k", tokens / 1_000)
    } else {
        tokens.to_string()
    }
}

impl AgentEditor {
    pub(super) fn limit_field(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut labels: Vec<SharedString> = vec![cx.t("agents.editor.limitNone")];
        labels.extend(
            LIMIT_CHOICES
                .iter()
                .map(|limit| tokens_short(*limit).into()),
        );
        let picked = match self.monthly_token_limit {
            None => Some(0),
            Some(limit) => LIMIT_CHOICES
                .iter()
                .position(|choice| *choice == limit)
                .map(|index| index + 1),
        };
        let editor = cx.entity().downgrade();
        let group = ToggleGroup::new("agent-editor-limit", labels, picked).on_select(
            move |index, _, cx| {
                let limit = index.checked_sub(1).map(|index| LIMIT_CHOICES[index]);
                let _ = editor.update(cx, |editor, cx| {
                    editor.monthly_token_limit = limit;
                    cx.notify();
                });
            },
        );
        field(
            cx.t("agents.editor.limit"),
            Some(cx.t("agents.editor.limitHint")),
            gpui::IntoElement::into_any_element(group),
            cx,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_read_short() {
        assert_eq!(tokens_short(2_000_000), "2M");
        assert_eq!(tokens_short(1_250_000), "1.2M");
        assert_eq!(tokens_short(42_000), "42k");
        assert_eq!(tokens_short(900), "900");
    }
}
