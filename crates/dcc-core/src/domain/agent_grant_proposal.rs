//! What the Guide asks for when it proposes that an agent act without
//! asking (`dcc_propose_grant`), turned into the change the card shows:
//! each field with what it is now, read from the desktop's own file, and
//! what it would be. Checked before it is a card, so the person is never
//! shown something that would not work: a grant with nowhere to hold, a
//! review grant for an agent that does not review, "fix in the task"
//! without the trigger that brings the comments.

use serde_json::Value;

use super::agent_change::{Change, TriggerSwitch, REVIEW_SENDS};
use super::agent_grants_check::{stored_grants, stored_review_send};
use super::agent_trigger::Trigger;

/// What is asked, with projects and rooms already by id ("" is Geral).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WantedGrants {
    pub review: Option<String>,
    pub review_projects: Option<Vec<String>>,
    pub reply: Option<bool>,
    pub reply_rooms: Option<Vec<String>>,
    pub deliver: Option<bool>,
    pub deliver_projects: Option<Vec<String>>,
    pub fix_in_task: Option<bool>,
    pub triggers: Vec<(Trigger, bool)>,
}

fn sorted(mut list: Vec<String>) -> Vec<String> {
    list.sort();
    list.dedup();
    list
}

/// The change for `agent_id`, or why it cannot be proposed. `reviewer`:
/// the agent is the Reviewer, the only one that reviews pull requests;
/// `file`: the desktop's `agent-triggers.json`.
pub fn grants_change(
    agent_id: &str,
    reviewer: bool,
    wanted: WantedGrants,
    file: &Value,
) -> Result<Change, String> {
    let now = stored_grants(file, agent_id);
    let review_now = stored_review_send(file, agent_id);
    if let Some(review) = &wanted.review {
        if !REVIEW_SENDS.contains(&review.as_str()) {
            return Err(format!("review is one of {}", REVIEW_SENDS.join(", ")));
        }
    }
    if !reviewer
        && (wanted.review.is_some()
            || wanted.review_projects.is_some()
            || wanted
                .triggers
                .iter()
                .any(|(trigger, _)| trigger.reviewer_only()))
    {
        return Err("only the Reviewer reviews pull requests: a review grant and the pull request triggers (prUpdated, reviewRequested) are its alone".to_string());
    }
    if let Some((trigger, _)) = wanted
        .triggers
        .iter()
        .find(|(trigger, _)| !Trigger::ALL.contains(trigger))
    {
        return Err(format!(
            "{} is not a switch; the triggers are: {}",
            trigger.id(),
            Trigger::ALL.map(Trigger::id).join(", ")
        ));
    }

    // What each would be once applied.
    let review_after = wanted.review.clone().unwrap_or_else(|| review_now.clone());
    let places = |wanted: &Option<Vec<String>>, now: &[String]| {
        sorted(wanted.clone().unwrap_or_else(|| now.to_vec()))
    };
    let review_projects = places(&wanted.review_projects, &now.review_projects);
    let reply_after = wanted.reply.unwrap_or(now.reply);
    let reply_rooms = places(&wanted.reply_rooms, &now.reply_rooms);
    let deliver_after = wanted.deliver.unwrap_or(now.deliver);
    let deliver_projects = places(&wanted.deliver_projects, &now.deliver_projects);
    let fix_after = wanted.fix_in_task.unwrap_or(now.fix_in_task);
    let on_after = |trigger: Trigger| {
        wanted
            .triggers
            .iter()
            .rev()
            .find(|(wanted, _)| *wanted == trigger)
            .map_or_else(|| now.triggers.contains(&trigger), |(_, on)| *on)
    };

    if review_after != "ask" && review_projects.is_empty() {
        return Err(
            "a review grant holds in the projects picked: say which (reviewProjects)".to_string(),
        );
    }
    if reply_after && reply_rooms.is_empty() {
        return Err(
            "replies go without asking in the rooms picked: say which (replyRooms)".to_string(),
        );
    }
    if deliver_after && deliver_projects.is_empty() {
        return Err(
            "delivery holds in the projects picked: say which (deliverProjects)".to_string(),
        );
    }
    if fix_after && !on_after(Trigger::ReviewReceived) {
        return Err("fixing in the task is started by the reviewReceived trigger: turn it on in the same proposal".to_string());
    }

    let pair = |asked: bool, before: bool, after: bool| asked.then_some((before, after));
    let listed = |asked: bool, before: &[String], after: Vec<String>| {
        asked.then(|| (sorted(before.to_vec()), after))
    };
    let mut triggers: Vec<TriggerSwitch> = Vec::new();
    for (trigger, after) in &wanted.triggers {
        triggers.retain(|switch| switch.trigger != *trigger);
        triggers.push(TriggerSwitch {
            trigger: *trigger,
            before: now.triggers.contains(trigger),
            after: *after,
        });
    }
    let change = Change::Grants {
        agent_id: agent_id.to_string(),
        review: wanted.review.is_some().then(|| (review_now, review_after)),
        review_projects: listed(
            wanted.review_projects.is_some(),
            &now.review_projects,
            review_projects,
        ),
        reply: pair(wanted.reply.is_some(), now.reply, reply_after),
        reply_rooms: listed(wanted.reply_rooms.is_some(), &now.reply_rooms, reply_rooms),
        deliver: pair(wanted.deliver.is_some(), now.deliver, deliver_after),
        deliver_projects: listed(
            wanted.deliver_projects.is_some(),
            &now.deliver_projects,
            deliver_projects,
        ),
        fix_in_task: pair(wanted.fix_in_task.is_some(), now.fix_in_task, fix_after),
        triggers,
    };
    if !change.changes_something() {
        return Err("it is already set like that; nothing to change".to_string());
    }
    Ok(change)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn file() -> Value {
        json!({
            "on": {"rev": ["reviewRequested"]},
            "autoSend": {"dev": {"reply": true, "replyRooms": [""]}}
        })
    }

    #[test]
    fn a_grant_is_proposed_only_when_it_would_hold() {
        let deliver = WantedGrants {
            deliver: Some(true),
            deliver_projects: Some(vec!["app".into()]),
            ..Default::default()
        };
        let Change::Grants {
            deliver: Some(switch),
            deliver_projects: Some((before, after)),
            reply: None,
            ..
        } = grants_change("dev", false, deliver.clone(), &file()).unwrap()
        else {
            panic!("a delivery grant");
        };
        assert_eq!(switch, (false, true));
        assert!(before.is_empty() && after == ["app"]);

        // Turned on with nowhere to hold: not a card.
        let nowhere = WantedGrants {
            deliver: Some(true),
            ..Default::default()
        };
        assert!(grants_change("dev", false, nowhere, &file())
            .unwrap_err()
            .contains("deliverProjects"));
        // Reviewing is the Reviewer's.
        let review = WantedGrants {
            review: Some("decide".into()),
            review_projects: Some(vec!["app".into()]),
            ..Default::default()
        };
        assert!(grants_change("dev", false, review.clone(), &file()).is_err());
        assert!(grants_change("rev", true, review, &file()).is_ok());
        let pr_trigger = WantedGrants {
            triggers: vec![(Trigger::PrUpdated, true)],
            ..Default::default()
        };
        assert!(grants_change("dev", false, pr_trigger, &file()).is_err());
        // "Fix in the task" needs what brings the comments, in the same card.
        let fix = |triggers| WantedGrants {
            fix_in_task: Some(true),
            triggers,
            ..Default::default()
        };
        assert!(grants_change("rev", true, fix(Vec::new()), &file())
            .unwrap_err()
            .contains("reviewReceived"));
        let Change::Grants { triggers, .. } = grants_change(
            "rev",
            true,
            fix(vec![(Trigger::ReviewReceived, true)]),
            &file(),
        )
        .unwrap() else {
            panic!("a grant");
        };
        assert_eq!(triggers[0].trigger, Trigger::ReviewReceived);
        assert!(!triggers[0].before && triggers[0].after);
        // Already as asked: nothing to propose; not a switch: refused.
        let same = WantedGrants {
            reply: Some(true),
            reply_rooms: Some(vec!["".into()]),
            ..Default::default()
        };
        assert!(grants_change("dev", false, same, &file())
            .unwrap_err()
            .contains("already"));
        let routine = WantedGrants {
            triggers: vec![(Trigger::Routine, true)],
            ..Default::default()
        };
        assert!(grants_change("dev", false, routine, &file()).is_err());
        let odd = WantedGrants {
            review: Some("always".into()),
            ..Default::default()
        };
        assert!(grants_change("rev", true, odd, &file()).is_err());
    }
}
