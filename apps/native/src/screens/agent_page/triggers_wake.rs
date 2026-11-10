//! Under "Gatilhos": when the agent wakes next — paused in the room, inside
//! the quiet hours, at today's ceiling, or at the next 5-minute check, which
//! costs no model until it finds something — and the quiet hours, the same
//! for every agent (`data::agent_quiet`).

use chrono::{Local, Timelike, Utc};
use gpui::{AnyElement, BorrowAppContext, Context, SharedString, div, prelude::*, px};

use super::AgentPage;
use crate::data::agent_quiet::{QUIET_CHOICES, QuietHours};
use crate::data::agent_triggers::AgentTriggers;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::ToggleGroup;

/// `agents.triggers.next.*` and its arguments, for an agent with a trigger on.
pub(crate) fn next_wake(triggers: &AgentTriggers, agent_id: &str) -> Option<(&'static str, Args)> {
    if !triggers.any_on(agent_id) {
        return None;
    }
    let now = Utc::now();
    if triggers.paused() {
        return Some(("agents.triggers.next.paused", Args::default()));
    }
    let hour = now.with_timezone(&Local).hour();
    if let Some(quiet) = triggers.quiet().filter(|quiet| quiet.contains(hour)) {
        return Some((
            "agents.triggers.next.quiet",
            Args::default().with("hour", quiet.to.to_string()),
        ));
    }
    if triggers.at_ceiling(agent_id, now) {
        return Some(("agents.triggers.next.ceiling", Args::default()));
    }
    Some(("agents.triggers.next.check", Args::default()))
}

fn label(quiet: QuietHours) -> SharedString {
    format!("{}h–{}h", quiet.from, quiet.to).into()
}

impl AgentPage {
    pub(super) fn wake_rows(&self, agent_id: &str, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(triggers) = cx.try_global::<AgentTriggers>() else {
            return Vec::new();
        };
        let colors = cx.theme().colors.clone();
        let next = next_wake(triggers, agent_id);
        let current = triggers.quiet();
        let picked = match current {
            None => 0,
            Some(quiet) => QUIET_CHOICES
                .iter()
                .position(|choice| *choice == quiet)
                .map_or(0, |index| index + 1),
        };
        let mut labels: Vec<SharedString> = vec![cx.t("agents.triggers.quiet.off")];
        labels.extend(QUIET_CHOICES.iter().copied().map(label));
        let page = cx.entity().downgrade();
        let group = ToggleGroup::new("agent-triggers-quiet", labels, Some(picked)).on_select(
            move |index, _, cx| {
                let quiet = index.checked_sub(1).map(|index| QUIET_CHOICES[index]);
                if cx.has_global::<AgentTriggers>() {
                    cx.update_global::<AgentTriggers, _>(|triggers, _| triggers.set_quiet(quiet));
                }
                let _ = page.update(cx, |_, cx| cx.notify());
            },
        );
        let mut rows = Vec::new();
        if let Some((key, args)) = next {
            rows.push(
                div()
                    .debug_selector(|| "agent-triggers-next".into())
                    .text_size(px(12.0))
                    .text_color(colors.foreground)
                    .child(cx.t_with(key, &args))
                    .into_any_element(),
            );
        }
        rows.push(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(10.0))
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("agents.triggers.quiet.title"))
                .child(group)
                .into_any_element(),
        );
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::agent_triggers::Trigger;

    #[test]
    fn the_next_wake_says_what_holds_the_agent() {
        let dir = tempfile::tempdir().unwrap();
        let mut triggers = AgentTriggers::load(dir.path().join("agent-triggers.json"));
        assert!(next_wake(&triggers, "rev").is_none(), "no trigger on");
        triggers.set("rev", Trigger::CiFailed, true);
        assert_eq!(
            next_wake(&triggers, "rev").unwrap().0,
            "agents.triggers.next.check"
        );
        triggers.set_paused(true);
        assert_eq!(
            next_wake(&triggers, "rev").unwrap().0,
            "agents.triggers.next.paused"
        );
    }
}
