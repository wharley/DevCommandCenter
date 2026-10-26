//! The task's conversations as the header's history lists them
//! (`nestSessionsByLineage`, `visibleSessions`, `session-chrome-state.ts`):
//! open ones first, each delegated child under the conversation that
//! delegated it, the archived ones apart.

use std::collections::HashMap;

use dcc_core::domain::session::{SessionState, WorkspaceSessionSummary};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionRow {
    pub id: String,
    pub title: String,
    pub provider_id: String,
    pub model: Option<String>,
    /// `projection.state` as the desktop names it (`active`, `completed`…).
    pub state: String,
    pub turn_count: u32,
    /// A turn runs in it now (`activeTurnId` on an active session).
    pub running: bool,
    pub archived: bool,
    /// 1 for a delegated child under its parent.
    pub depth: u8,
}

fn state_name(state: &SessionState) -> &'static str {
    match state {
        SessionState::Draft => "draft",
        SessionState::Active => "active",
        SessionState::Completed => "completed",
        SessionState::Aborted => "aborted",
    }
}

fn row(summary: &WorkspaceSessionSummary, depth: u8) -> SessionRow {
    let active = summary.projection.state == SessionState::Active;
    SessionRow {
        id: summary.session.id.0.clone(),
        title: summary.thread.title.clone(),
        provider_id: summary.session.provider_id.clone(),
        model: summary.session.model.clone(),
        state: state_name(&summary.projection.state).to_string(),
        turn_count: summary.projection.turn_count,
        running: active && summary.projection.active_turn_id.is_some(),
        archived: summary.thread.archived_at.is_some(),
        depth,
    }
}

/// The open conversations nested by lineage, then the archived ones.
/// `lineage` maps a delegated child to the session that delegated it.
pub fn rows(
    summaries: &[WorkspaceSessionSummary],
    lineage: &HashMap<String, String>,
) -> Vec<SessionRow> {
    let open: Vec<&WorkspaceSessionSummary> = summaries
        .iter()
        .filter(|summary| summary.thread.archived_at.is_none())
        .collect();
    let ids: std::collections::HashSet<&str> = open
        .iter()
        .map(|summary| summary.session.id.0.as_str())
        .collect();
    let parent_of = |id: &str| {
        lineage
            .get(id)
            .filter(|parent| parent.as_str() != id && ids.contains(parent.as_str()))
    };
    let mut nested = Vec::new();
    for summary in &open {
        if parent_of(&summary.session.id.0).is_some() {
            continue;
        }
        nested.push(row(summary, 0));
        for child in &open {
            if parent_of(&child.session.id.0) == Some(&summary.session.id.0) {
                nested.push(row(child, 1));
            }
        }
    }
    nested.extend(
        summaries
            .iter()
            .filter(|summary| summary.thread.archived_at.is_some())
            .map(|summary| row(summary, 0)),
    );
    nested
}

/// `canResumeSession`: a session not already active.
pub fn can_resume(row: Option<&SessionRow>) -> bool {
    row.is_some_and(|row| row.state != "active")
}

/// `nextVisibleSessionIdAfterClose`: the first other open conversation.
pub fn next_after_close<'a>(rows: &'a [SessionRow], closing: &str) -> Option<&'a str> {
    rows.iter()
        .find(|row| !row.archived && row.id != closing)
        .map(|row| row.id.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::project::ProjectId;
    use dcc_core::domain::session::{Session, SessionId, SessionProjection, TurnId};
    use dcc_core::domain::thread::{Thread, ThreadId};
    use dcc_core::domain::workspace::WorkspaceId;

    fn summary(id: &str, archived: bool, running: bool) -> WorkspaceSessionSummary {
        let session = Session {
            id: SessionId(id.into()),
            project_id: ProjectId("p".into()),
            workspace_id: WorkspaceId("w".into()),
            additional_workspace_ids: Vec::new(),
            provider_id: "claude_code".into(),
            model: None,
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: String::new(),
            updated_at: String::new(),
        };
        WorkspaceSessionSummary {
            thread: Thread {
                id: ThreadId(format!("t-{id}")),
                project_id: ProjectId("p".into()),
                session_id: Some(SessionId(id.into())),
                title: format!("chat {id}"),
                archived_at: archived.then(|| "2026-10-25T00:00:00Z".to_string()),
            },
            projection: SessionProjection {
                session_id: SessionId(id.into()),
                project_id: ProjectId("p".into()),
                workspace_id: WorkspaceId("w".into()),
                provider_id: "claude_code".into(),
                model: None,
                state: SessionState::Active,
                active_turn_id: running.then(|| TurnId("turn".into())),
                turn_count: 1,
                checkpoint_count: 0,
                created_at: String::new(),
                updated_at: String::new(),
            },
            session,
            last_turn_prompt: None,
            last_turn_state: None,
            last_turn_started_at: None,
            last_turn_completed_at: None,
            last_turn_awaiting_user: None,
            last_turn_awaiting_since: None,
            usage_limit: None,
            goal: None,
        }
    }

    #[test]
    fn children_sit_under_their_parent_and_archived_go_last() {
        let summaries = [
            summary("child", false, true),
            summary("a", false, false),
            summary("old", true, false),
            summary("b", false, false),
            summary("orphan", false, false),
        ];
        let lineage = HashMap::from([
            ("child".to_string(), "a".to_string()),
            ("orphan".to_string(), "gone".to_string()),
        ]);
        let rows = rows(&summaries, &lineage);
        let order: Vec<(&str, u8, bool)> = rows
            .iter()
            .map(|row| (row.id.as_str(), row.depth, row.archived))
            .collect();
        assert_eq!(
            order,
            [
                ("a", 0, false),
                ("child", 1, false),
                ("b", 0, false),
                ("orphan", 0, false),
                ("old", 0, true),
            ]
        );
        assert!(rows[1].running);
        assert_eq!(next_after_close(&rows, "a"), Some("child"));
        assert!(!can_resume(rows.first()), "already active");
        assert!(!can_resume(None));
    }
}
