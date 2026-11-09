//! A resident agent's own conversation, opened from its page: one per agent,
//! in a folder of its own inside DCC's data (its "home"), under a project id
//! no person's project uses, so it stays out of the task list. There is no
//! code there: the agent operates DCC through its tools, and code work
//! happens in a task of the right project.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Every agent's home sits in `<app data>/agent-homes/<agent id>`.
pub const AGENT_HOMES_DIR: &str = "agent-homes";
pub const HOME_PROJECT_PREFIX: &str = "dcc-agent-";
pub const OVERVIEW_TOOL: &str = "dcc_overview";
pub const OPEN_TASK_TOOL: &str = "dcc_open_task";
pub const CONVERSATION_TAG: &str = "dcc_agent_conversation";

/// Where a task the agent opens from its conversation goes: a task of its
/// own in one of the person's projects, in their task list like one made
/// with "Nova tarefa", instead of a conversation in the caller's checkout.
/// Kept in the proposal's budget until the person starts it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NewTaskTarget {
    pub project_id: String,
    /// The project's name as the person sees it.
    pub project: String,
    pub root_path: String,
    pub base_branch: String,
    /// A protected worktree (the default) or the project's own checkout.
    pub worktree: bool,
    /// The first turn's effort; omitted: medium.
    #[serde(default)]
    pub effort: Option<String>,
    /// The task it became, once the person started it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
}

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
Reading or changing code happens in a task of the right project, not here. When the person asks for work in a project, call {OPEN_TASK_TOOL} with the project as {OVERVIEW_TOOL} names it and a self-contained brief: it shows a card they start, or starts at once with start=now when they said to start it now. Small questions about where things stand you answer yourself.\n\
To review a pull request (one the person names, links or asks you to find), call dcc_review_pull_request with its link: DCC runs the same review as Pull Requests › Gerar revisão, shows it working to the person and keeps its draft comments in Pull Requests for them; nothing is published. Do not open a task to review; a task is for changing code.\n\
When the person asks for something recurring (\"every Monday…\"), propose a routine with dcc_propose_routine: it waits as a card they create.\n\
When a [DCC] status says one of your tasks ended a turn, tell the person in a few lines what it did and stop. Send it more work with dcc_task_message only when the person asks; never review, fix or merge on your own.\n\
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
        assert!(block.contains(OPEN_TASK_TOOL));
    }
}
