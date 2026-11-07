//! The open conversation's lineage (`delegation-lineage.ts`): the
//! delegations it started, newest first, and the one that created it when
//! it is a delegated child; which still run; what each is called.

use std::collections::HashMap;

use dcc_core::domain::delegation::DelegationStatus;

use super::conversation::DelegationRecord;
use super::session_list::SessionRow;

/// `isDelegationActive`: a draft, queued or running one can be stopped.
pub fn is_active(record: &DelegationRecord) -> bool {
    matches!(
        record.status,
        DelegationStatus::Draft | DelegationStatus::Queued | DelegationStatus::Running
    )
}

fn newest_first(left: &&DelegationRecord, right: &&DelegationRecord) -> std::cmp::Ordering {
    right.created_at.cmp(&left.created_at)
}

/// `childDelegationsOf`
pub fn children<'a>(
    delegations: &'a HashMap<String, DelegationRecord>,
    session: Option<&str>,
) -> Vec<&'a DelegationRecord> {
    let Some(session) = session else {
        return Vec::new();
    };
    let mut children: Vec<&DelegationRecord> = delegations
        .values()
        .filter(|record| record.parent_session_id == session)
        .collect();
    children.sort_by(newest_first);
    children
}

/// `parentDelegationOf`
pub fn parent<'a>(
    delegations: &'a HashMap<String, DelegationRecord>,
    session: Option<&str>,
) -> Option<&'a DelegationRecord> {
    let session = session?;
    let mut parents: Vec<&DelegationRecord> = delegations
        .values()
        .filter(|record| record.child_session_id.as_deref() == Some(session))
        .collect();
    parents.sort_by(newest_first);
    parents.first().copied()
}

/// The title of a session in the task, if it is one.
pub fn session_title<'a>(sessions: &'a [SessionRow], id: Option<&str>) -> Option<&'a str> {
    let id = id?;
    sessions
        .iter()
        .find(|row| row.id == id)
        .map(|row| row.title.as_str())
}

/// `delegationTitle`: the child's title (the agent's short name for the
/// task), else the instruction, written for another agent.
pub fn title(record: &DelegationRecord, sessions: &[SessionRow]) -> String {
    session_title(sessions, record.child_session_id.as_deref())
        .map(str::trim)
        .filter(|title| !title.is_empty() && !title.starts_with("Delegated "))
        .map(str::to_string)
        .unwrap_or_else(|| record.instruction.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: &str, parent: &str, child: Option<&str>, at: &str) -> DelegationRecord {
        DelegationRecord {
            id: id.into(),
            status: DelegationStatus::Running,
            mode: "implement".into(),
            created_at: at.into(),
            updated_at: String::new(),
            diff_summary: None,
            validation_summary: None,
            parent_turn_id: None,
            started_at: None,
            is_task: false,
            title: None,
            instruction: format!("faça {id}"),
            result_summary: None,
            prompt: String::new(),
            touched_files: Vec::new(),
            provider_id: "codex".into(),
            model_id: None,
            child_session_id: child.map(str::to_string),
            implement: true,
            parent_session_id: parent.into(),
            worktree: None,
            shared_checkout: false,
            new_task: None,
        }
    }

    fn row(id: &str, title: &str) -> SessionRow {
        SessionRow {
            id: id.into(),
            title: title.into(),
            provider_id: "codex".into(),
            model: None,
            account_id: None,
            state: "active".into(),
            turn_count: 1,
            running: false,
            archived: false,
            depth: 0,
            updated_at: String::new(),
            created_at: String::new(),
        }
    }

    #[test]
    fn children_come_newest_first_and_a_child_knows_its_parent() {
        let delegations: HashMap<String, DelegationRecord> = [
            record("d-1", "s-a", Some("s-1"), "2026-10-26T10:00:00Z"),
            record("d-2", "s-a", Some("s-2"), "2026-10-26T11:00:00Z"),
            record("d-3", "s-b", None, "2026-10-26T12:00:00Z"),
        ]
        .into_iter()
        .map(|record| (record.id.clone(), record))
        .collect();
        let ids: Vec<&str> = children(&delegations, Some("s-a"))
            .iter()
            .map(|record| record.id.as_str())
            .collect();
        assert_eq!(ids, ["d-2", "d-1"]);
        assert!(children(&delegations, None).is_empty());
        assert_eq!(parent(&delegations, Some("s-2")).unwrap().id, "d-2");
        assert!(parent(&delegations, Some("s-a")).is_none());
    }

    #[test]
    fn the_childs_title_unless_it_is_the_placeholder() {
        let sessions = [
            row("s-1", "Guia do checkout"),
            row("s-2", "Delegated implement"),
        ];
        let named = record("d-1", "s-a", Some("s-1"), "x");
        let placeholder = record("d-2", "s-a", Some("s-2"), "x");
        assert_eq!(title(&named, &sessions), "Guia do checkout");
        assert_eq!(title(&placeholder, &sessions), "faça d-2");
        let mut done = named.clone();
        done.status = DelegationStatus::Completed;
        assert!(is_active(&named) && !is_active(&done));
    }
}
