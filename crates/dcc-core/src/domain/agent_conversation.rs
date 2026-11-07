//! A resident agent's own conversation, opened from its page: one per agent,
//! in a folder of its own inside DCC's data (its "home"), under a project id
//! no person's project uses, so it stays out of the task list. There is no
//! code there: the agent operates DCC through its tools, and code work
//! happens in a task of the right project.

/// Every agent's home sits in `<app data>/agent-homes/<agent id>`.
pub const AGENT_HOMES_DIR: &str = "agent-homes";
pub const HOME_PROJECT_PREFIX: &str = "dcc-agent-";
pub const OVERVIEW_TOOL: &str = "dcc_overview";
pub const CONVERSATION_TAG: &str = "dcc_agent_conversation";

/// The project id of `agent_id`'s home.
pub fn home_project_id(agent_id: &str) -> String {
    format!("{HOME_PROJECT_PREFIX}{agent_id}")
}

/// The agent whose home `project_id` is, if it is one.
pub fn home_agent_id(project_id: &str) -> Option<&str> {
    project_id
        .strip_prefix(HOME_PROJECT_PREFIX)
        .filter(|id| !id.is_empty())
}

/// An agent id that is safe as a folder name (ids are uuids or preset names).
pub fn is_valid_agent_id(agent_id: &str) -> bool {
    !agent_id.is_empty()
        && agent_id.len() <= 128
        && agent_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

/// What a turn of the home conversation carries besides the role and the
/// memory: where it is and what it may do.
pub fn conversation_block() -> String {
    format!(
        "<{CONVERSATION_TAG}>\n\
This is the person's ongoing conversation with you on your page in DCC, not a task. The folder you run in is your empty home: there is no project code here, so do not create or edit files in it.\n\
You help the person operate DCC across their projects. Call {OVERVIEW_TOOL} for their projects and tasks (what needs them, what is running, what finished recently) before answering about them, and name tasks and projects as it shows them.\n\
Reading or changing code happens in a task of the right project, not here. You cannot open tasks from this conversation yet: say which project and what to ask, so the person starts it with \"Nova tarefa\".\n\
Answer in the person's language, short and direct.\n\
</{CONVERSATION_TAG}>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_home_is_known_by_its_project_id() {
        let project = home_project_id("a-1");
        assert_eq!(project, "dcc-agent-a-1");
        assert_eq!(home_agent_id(&project), Some("a-1"));
        assert_eq!(home_agent_id("dcc-agent-"), None);
        assert_eq!(home_agent_id("devcommandcenter"), None);
        assert!(is_valid_agent_id("8f2c-11aa_b"));
        assert!(!is_valid_agent_id("../x"));
        assert!(!is_valid_agent_id(""));
        let block = conversation_block();
        assert!(block.starts_with("<dcc_agent_conversation>"));
        assert!(block.contains(OVERVIEW_TOOL));
    }
}
