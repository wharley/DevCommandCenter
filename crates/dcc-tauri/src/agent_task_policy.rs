//! What a task an agent opens may do without asking. A task takes its
//! opener's mode, and an agent's own conversation always runs in "ask" (so a
//! write there waits for the person); its tasks follow what the person chose
//! for the agent instead ("Tarefas que abre" in its editor), ask by default.

use dcc_core::domain::provider::ProviderApprovalPolicy;
use dcc_core::domain::session::SessionId;

use crate::state::SessionCommandState;

/// The mode for the tasks `parent` opens, when it is an agent's own
/// conversation.
pub(crate) fn for_agent_parent(
    state: &SessionCommandState,
    parent: &SessionId,
) -> Option<ProviderApprovalPolicy> {
    if !state.is_agent_conversation(parent).ok()? {
        return None;
    }
    let agent = state.session_agent(parent).ok()??;
    Some(
        agent
            .tool_access
            .tasks_approval
            .unwrap_or(ProviderApprovalPolicy::Ask),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::agent::{ResidentAgentDraft, ReviewerPresetText};
    use dcc_core::domain::agent_conversation::home_project_id;
    use dcc_core::domain::project::ProjectId;
    use dcc_core::domain::session::{Session, SessionState};
    use dcc_core::domain::workspace::WorkspaceId;
    use dcc_core::ports::SessionRepo;

    fn session(id: &str, project: &str) -> Session {
        Session {
            id: SessionId(id.to_string()),
            project_id: ProjectId(project.to_string()),
            workspace_id: WorkspaceId("workspace".to_string()),
            additional_workspace_ids: Vec::new(),
            provider_id: "codex".to_string(),
            model: None,
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: "2026-11-10T10:00:00Z".to_string(),
            updated_at: "2026-11-10T10:00:00Z".to_string(),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn an_agents_tasks_take_the_mode_set_for_it() {
        let root = tempfile::tempdir().unwrap();
        let physical = std::fs::canonicalize(root.path()).unwrap();
        let state = SessionCommandState::new_headless(
            physical.join("state.sqlite"),
            physical.join("app-data"),
        );
        let mut draft = ResidentAgentDraft::reviewer(&ReviewerPresetText::default());
        draft.name = "Dev".into();
        draft.tool_access.groups.clear();
        draft.tool_access.tasks_approval = Some(ProviderApprovalPolicy::FullAccess);
        let agent = state.save_resident_agent(None, draft).unwrap();
        let home = session("home", &home_project_id(&agent.id));
        let task = session("task", "checkout");
        for each in [&home, &task] {
            SessionRepo::save_session(&state, each).await.unwrap();
        }
        state.bind_session_agent(&home.id, &agent.id).unwrap();
        assert_eq!(
            for_agent_parent(&state, &home.id),
            Some(ProviderApprovalPolicy::FullAccess)
        );
        // A person's task keeps its own mode.
        assert_eq!(for_agent_parent(&state, &task.id), None);
    }
}
