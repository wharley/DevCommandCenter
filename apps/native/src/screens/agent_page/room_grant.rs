//! What an agent would do without asking, proposed by the Guide
//! (`Change::Grants`): what its card lists, and writing it. The app writes
//! it, into its own file (`AgentTriggers`), when the person clicks
//! "Aplicar" on that card — the server stores the proposal and turns
//! nothing on, and no agent does.

use dcc_core::domain::agent_change::Change;

use super::room_adjust::Line;
use crate::data::agent_auto_send::ReviewSend;
use crate::data::agent_triggers::AgentTriggers;

/// What the card lists for a grant. `project`: a project's name by id;
/// `word`: a text by its whole key.
pub(super) fn grant_lines(
    change: &Change,
    project: &dyn Fn(&str) -> String,
    word: &dyn Fn(&str) -> String,
) -> Vec<Line> {
    let Change::Grants {
        review,
        review_projects,
        reply,
        reply_rooms,
        deliver,
        deliver_projects,
        fix_in_task,
        triggers,
        ..
    } = change
    else {
        return Vec::new();
    };
    let places = |ids: &[String]| {
        if ids.is_empty() {
            word("agents.change.nowhere")
        } else {
            ids.iter()
                .map(|id| match id.as_str() {
                    "" => word("agents.change.general"),
                    id => project(id),
                })
                .collect::<Vec<_>>()
                .join(", ")
        }
    };
    let mut lines = Vec::new();
    let mut push = |label: &str, before: String, after: String| {
        lines.push(Line {
            label: label.to_string(),
            before,
            after,
        })
    };
    // "Perguntar" or the choice that sends, in the words of the agent's page.
    let choice = |group: &str, on: bool| {
        word(&format!(
            "agents.autoSend.{group}.{}",
            if on { "send" } else { "ask" }
        ))
    };
    if let Some((before, after)) = review {
        let send = |send: &str| word(&format!("agents.autoSend.review.{send}"));
        push("agents.autoSend.review.title", send(before), send(after));
    }
    if let Some((before, after)) = review_projects {
        push("agents.change.reviewIn", places(before), places(after));
    }
    if let Some((before, after)) = reply {
        push(
            "agents.autoSend.reply.title",
            choice("reply", *before),
            choice("reply", *after),
        );
    }
    if let Some((before, after)) = reply_rooms {
        push("agents.change.replyIn", places(before), places(after));
    }
    if let Some((before, after)) = deliver {
        push(
            "agents.autoSend.deliver.title",
            choice("deliver", *before),
            choice("deliver", *after),
        );
    }
    if let Some((before, after)) = deliver_projects {
        push("agents.change.deliverIn", places(before), places(after));
    }
    if let Some((before, after)) = fix_in_task {
        push(
            "agents.autoSend.fix.title",
            choice("fix", *before),
            choice("fix", *after),
        );
    }
    for switch in triggers {
        let state = |on: bool| {
            word(if on {
                "agents.change.triggerOn"
            } else {
                "agents.change.triggerOff"
            })
        };
        push(
            &format!("agents.triggers.{}.title", switch.trigger.id()),
            state(switch.before),
            state(switch.after),
        );
    }
    lines.retain(|line| line.before != line.after);
    lines
}

/// Writes the grant as the card showed it: each place named is on or off
/// as its "after" says, then each switch. Places the person picked since
/// the proposal, which it does not name, stay.
pub(super) fn apply_grants(change: &Change, triggers: &mut AgentTriggers) {
    let Change::Grants {
        agent_id,
        review,
        review_projects,
        reply,
        reply_rooms,
        deliver,
        deliver_projects,
        fix_in_task,
        triggers: switches,
    } = change
    else {
        return;
    };
    let each = |pair: &Option<(Vec<String>, Vec<String>)>| -> Vec<(String, bool)> {
        let Some((before, after)) = pair else {
            return Vec::new();
        };
        let mut all: Vec<String> = before.iter().chain(after).cloned().collect();
        all.sort();
        all.dedup();
        all.into_iter()
            .map(|id| {
                let on = after.contains(&id);
                (id, on)
            })
            .collect()
    };
    for (project, on) in each(review_projects) {
        triggers.set_review_send_in(agent_id, &project, on);
    }
    for (room, on) in each(reply_rooms) {
        triggers.set_reply_send_in(agent_id, &room, on);
    }
    for (project, on) in each(deliver_projects) {
        triggers.set_deliver_in(agent_id, &project, on);
    }
    // What starts a grant comes before the grant.
    for switch in switches {
        triggers.set(agent_id, switch.trigger, switch.after);
    }
    if let Some((_, after)) = review {
        triggers.set_review_send(
            agent_id,
            match after.as_str() {
                "comment" => ReviewSend::Comment,
                "decide" => ReviewSend::Decide,
                _ => ReviewSend::Ask,
            },
        );
    }
    if let Some((_, after)) = reply {
        triggers.set_reply_send(agent_id, *after);
    }
    if let Some((_, after)) = deliver {
        triggers.set_deliver(agent_id, *after);
    }
    if let Some((_, after)) = fix_in_task {
        triggers.set_fix_in_task(agent_id, *after);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::agent_triggers::Trigger;
    use dcc_core::domain::agent_change::TriggerSwitch;

    fn grant() -> Change {
        Change::Grants {
            agent_id: "rev".into(),
            review: Some(("ask".into(), "decide".into())),
            review_projects: Some((Vec::new(), vec!["app".into()])),
            reply: Some((false, true)),
            reply_rooms: Some((vec!["old".into()], vec!["".into(), "app".into()])),
            deliver: None,
            deliver_projects: None,
            fix_in_task: Some((false, true)),
            triggers: vec![
                TriggerSwitch {
                    trigger: Trigger::ReviewReceived,
                    before: false,
                    after: true,
                },
                TriggerSwitch {
                    trigger: Trigger::CiFailed,
                    before: true,
                    after: true,
                },
            ],
        }
    }

    #[test]
    fn the_card_says_each_grant_in_the_words_of_the_agents_page() {
        let project = |id: &str| {
            if id == "app" {
                "Checkout".to_string()
            } else {
                id.to_string()
            }
        };
        let word = |key: &str| key.rsplit('.').next().unwrap_or(key).to_string();
        let listed = grant_lines(&grant(), &project, &word);
        let shown: Vec<(&str, &str, &str)> = listed
            .iter()
            .map(|line| {
                (
                    line.label.as_str(),
                    line.before.as_str(),
                    line.after.as_str(),
                )
            })
            .collect();
        assert_eq!(
            shown,
            [
                ("agents.autoSend.review.title", "ask", "decide"),
                ("agents.change.reviewIn", "nowhere", "Checkout"),
                ("agents.autoSend.reply.title", "ask", "send"),
                ("agents.change.replyIn", "old", "general, Checkout"),
                ("agents.autoSend.fix.title", "ask", "send"),
                // The trigger that stays as it was is not on the card.
                (
                    "agents.triggers.reviewReceived.title",
                    "triggerOff",
                    "triggerOn"
                ),
            ]
        );
    }

    #[test]
    fn applying_writes_what_the_card_showed_and_no_more() {
        let dir = tempfile::tempdir().unwrap();
        let mut triggers = AgentTriggers::load(dir.path().join("agent-triggers.json"));
        // Picked by the person before: a room the card takes away, and one
        // it does not name.
        triggers.set_reply_send_in("rev", "old", true);
        triggers.set_reply_send_in("rev", "kept", true);
        triggers.set_deliver("dev", true);
        apply_grants(&grant(), &mut triggers);
        assert_eq!(triggers.review_send_choice("rev"), ReviewSend::Decide);
        assert!(triggers.review_send_in("rev", "app"));
        assert!(triggers.reply_send_on("rev"));
        assert!(triggers.reply_send_in("rev", "") && triggers.reply_send_in("rev", "app"));
        assert!(!triggers.reply_send_in("rev", "old"));
        assert!(triggers.reply_send_in("rev", "kept"));
        assert!(triggers.fix_in_task("rev"));
        assert!(triggers.is_on("rev", Trigger::ReviewReceived));
        // What it does not name is as it was, for it and for the others.
        assert!(!triggers.deliver_on("rev"));
        assert!(triggers.deliver_on("dev") && !triggers.reply_send_on("dev"));
        // Not a grant: nothing is written.
        let projects = Change::Projects {
            agent_id: "dev".into(),
            before: Vec::new(),
            after: vec!["app".into()],
        };
        apply_grants(&projects, &mut triggers);
        assert!(!triggers.deliver_in("dev", "app"));
    }
}
