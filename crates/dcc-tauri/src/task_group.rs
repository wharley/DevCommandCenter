//! One report for the tasks an agent opened together (Monocode's grouped
//! completion): the tasks opened in the same turn of the parent form a
//! group, and their first turns — the briefs — come back to the parent as
//! one [DCC] status once the last of them ends, instead of one wake-up per
//! task. Follow-up turns and tasks opened alone report as before.

use dcc_core::domain::delegation::{Delegation, DelegationKind, DelegationStatus};

/// How much of each other task's answer goes along.
const SIBLING_CHARS: usize = 400;

/// The other tasks opened in the same parent turn as `task`, started.
fn siblings<'a>(task: &Delegation, tasks: &'a [Delegation]) -> Vec<&'a Delegation> {
    let Some(turn) = task.parent_turn_id.as_ref() else {
        return Vec::new();
    };
    tasks
        .iter()
        .filter(|other| other.id != task.id && other.kind == DelegationKind::Task)
        .filter(|other| other.parent_turn_id.as_ref() == Some(turn))
        .filter(|other| other.child_session_id.is_some())
        .collect()
}

fn running(task: &Delegation) -> bool {
    matches!(
        task.status,
        DelegationStatus::Queued | DelegationStatus::Running
    )
}

/// Whether `task`'s first report waits for a sibling still on its brief.
pub fn waits_for_siblings(task: &Delegation, tasks: &[Delegation]) -> bool {
    siblings(task, tasks)
        .into_iter()
        .any(|other| running(other) && other.reported_turn_id.is_none())
}

/// `base` (the status of the task that ended last) with what the others
/// opened in the same turn said; `base` as it is when it was alone.
pub fn with_siblings(base: String, task: &Delegation, tasks: &[Delegation]) -> String {
    let done: Vec<&Delegation> = siblings(task, tasks)
        .into_iter()
        .filter(|other| other.reported_turn_id.is_some() || !running(other))
        .collect();
    if done.is_empty() {
        return base;
    }
    let mut block = vec![
        String::new(),
        "The other tasks you opened in the same turn ended their first turn too:".to_string(),
    ];
    for other in done {
        let title = other
            .title
            .as_deref()
            .filter(|title| !title.trim().is_empty())
            .unwrap_or("Untitled task");
        let summary: String = other
            .result_summary
            .as_deref()
            .filter(|summary| !summary.trim().is_empty())
            .unwrap_or("It returned no text.")
            .chars()
            .take(SIBLING_CHARS)
            .collect();
        block.push(format!(
            "- \"{title}\" (task {}): {}",
            other.id.0,
            summary.split_whitespace().collect::<Vec<_>>().join(" ")
        ));
    }
    block.push("Give the person one report for all of them.".to_string());
    // Before the list of every task and the closing note.
    let at = base
        .find("\n\nYour tasks:")
        .or_else(|| base.find("\n\nThe human can open the task"))
        .unwrap_or(base.len());
    let mut text = base;
    text.insert_str(at, &block.join("\n"));
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::session::TurnId;

    fn task(id: &str, turn: &str, reported: bool, summary: &str) -> Delegation {
        let mut value = serde_json::json!({
            "id": id,
            "parentSessionId": "home",
            "parentTurnId": turn,
            "childSessionId": format!("child-{id}"),
            "workspaceId": "w-home",
            "targetProviderId": "claude_code",
            "mode": "implement",
            "status": "running",
            "prompt": "",
            "contextPolicy": { "type": "minimal" },
            "budget": { "turnLimit": null, "timeoutSeconds": null, "allowFileEdits": true },
            "resultSummary": summary,
            "diffSummary": null,
            "validationSummary": null,
            "createdAt": "2026-11-09T10:00:00Z",
            "updatedAt": "2026-11-09T10:00:00Z",
            "kind": "task",
            "title": format!("Tarefa {id}"),
        });
        if reported {
            value["reportedTurnId"] = serde_json::json!(format!("t-{id}"));
        }
        serde_json::from_value(value).expect("delegation")
    }

    #[test]
    fn tasks_opened_together_report_once_the_last_ends() {
        let first = task("a", "turn-1", true, "Analisei: dá para enviar o campo.");
        let second = task("b", "turn-1", false, "");
        let alone = task("c", "turn-2", false, "");
        let tasks = vec![first.clone(), second.clone(), alone.clone()];
        // "a" ended its brief while "b" still works on its own.
        assert!(waits_for_siblings(&first, &tasks));
        assert!(!waits_for_siblings(&alone, &tasks));
        // "b" ends last: one report with what "a" said.
        let mut second_done = second.clone();
        second_done.reported_turn_id = Some(TurnId("t-b".into()));
        let tasks = vec![first.clone(), second_done.clone(), alone];
        assert!(!waits_for_siblings(&second_done, &tasks));
        let base = "[DCC] Task \"Tarefa b\" ended a turn — x (task b).\n\nFeito.\n\nYour tasks:\n- …\n\nThe human can open the task…".to_string();
        let report = with_siblings(base, &second_done, &tasks);
        assert!(report.starts_with("[DCC] Task \"Tarefa b\" ended a turn"));
        let block = report.find("The other tasks you opened").unwrap();
        assert!(block < report.find("Your tasks:").unwrap());
        assert!(report.contains("\"Tarefa a\" (task a): Analisei: dá para enviar o campo."));
        // Alone, nothing is added.
        let single = with_siblings("x".into(), &task("d", "turn-3", true, ""), &tasks);
        assert_eq!(single, "x");
    }
}
