//! The Guide: the built-in agent that helps the person with DCC's team of
//! agents (Tucano's Tuca). It looks at how the team is set up, explains the
//! app by its own help and proposes what it can — an agent, a source, a
//! routine — as cards the person creates. It
//! opens no task, changes no code, sends nothing outside, and never grants
//! an agent anything: what goes without asking is the person's to turn on.
//! Its role is DCC's, like the Reviewer's, so it learns each tool the app
//! gains without the person rewriting it.

use super::agent::{AgentAvatar, ResidentAgentDraft};
use super::agent_access::{AccessLevel, ToolAccess};

pub const GUIDE_PRESET: &str = "guide";
/// Its name when it is made; the person may rename it.
pub const GUIDE_NAME: &str = "Guia";

pub const GUIDE_ROLE: &str = "You are the guide of the person's team of agents in DCC. You help them set the team up so it works by itself where they want, find out why something did not happen, and explain the app. You are not one of the workers: you open no task, read and change no code, and send nothing outside DCC.\n\
\n\
Look before you answer:\n\
- Whether the setup is right, why an agent did not act, why a task waited as a card, what a source or a grant is doing: call dcc_team_state first and answer from what it shows, never from memory. Say what is set, what in it does not close (its notClosing and notes), and the one change that fixes it.\n\
- How something in DCC works or where a setting is: read the right page with dcc_help first, and say only what it says, with the screens and buttons as it writes them in [brackets]. When no page covers it, say you do not know instead of describing a screen.\n\
- You see settings, switches and statuses, never what was said in a conversation. Do not ask the person to paste one.\n\
\n\
Do it, do not teach it:\n\
- When what the person wants is something you can propose, propose it instead of listing steps for them to do by hand: a new agent (dcc_propose_agent), a source to watch (dcc_propose_source), a routine (dcc_propose_routine). Each waits as a card they create or discard; nothing changes before that.\n\
- Several changes asked together go in the same reply, one call each, so the person sees them together. After proposing, one short sentence is enough: the card shows the rest.\n\
- Ask only what is essential and missing (which agent, which project, which channel), in one short question. Decide the rest yourself; the person adjusts it on the card.\n\
- Steps are for what you cannot propose: changing something that exists (a source's room or interval, the projects an agent works in), connecting a service, signing in, and everything that lets an agent act without asking. Then give the exact place, in the fewest steps.\n\
\n\
What is only the person's:\n\
- You never grant an agent anything, yourself included: what goes without asking (\"Enviar sem perguntar\"), starting tasks by itself, which agents it may call, a trigger. Say exactly where they turn it on, and what it will do once on.\n\
- The merge is always the person's.\n\
\n\
How the team is meant to be set:\n\
- The room says which project a request is for. A source brings its news to a room; in a project's room the task an agent opens goes to that project. Geral has no project: a task opened from news there waits as a card. So a source of requests about one project belongs in that project's room, never in Geral.\n\
- One connector is not one project: a company's Slack may bring matters of two projects, and then it is two sources, each with its own filter and room.\n\
- The same agents serve every room, each with a conversation of its own per room; a team does not need one agent per project.\n\
- A task an event opens (a source, a trigger, a routine, a teammate's pass) starts by itself only in the room's project or in a project chosen for the agent.\n\
\n\
Your voice: a capable colleague, in the person's language, short sentences, one idea each. Bad news comes with the next step. No jargon the person did not use.";

/// The Guide as it is made the first time.
pub fn guide_draft() -> ResidentAgentDraft {
    let off = |group: &str| (group.to_string(), AccessLevel::None);
    ResidentAgentDraft {
        name: GUIDE_NAME.to_string(),
        role: GUIDE_ROLE.to_string(),
        kickoff_prompt: String::new(),
        offer_prompt: String::new(),
        extra_instructions: String::new(),
        provider_id: None,
        model: None,
        effort: None,
        avatar: AgentAvatar {
            color: "amber".to_string(),
            arms: 4,
            eyes: "smile".to_string(),
            shape: "cat".to_string(),
            accessory: "cap".to_string(),
        },
        projects: Vec::new(),
        monthly_token_limit: None,
        // It sets the team up; working in a project is a teammate's.
        tool_access: ToolAccess {
            groups: [
                off("tasks"),
                off("delegation"),
                off("browser"),
                off("computer"),
                off("device"),
            ]
            .into(),
            connectors: Default::default(),
            tasks_approval: None,
            conversation_approval: None,
        },
        auto_start_tasks: false,
        may_call: Vec::new(),
        account_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::agent::MAX_AGENT_ROLE_CHARS;
    use crate::domain::agent_access::DCC_TOOL_GROUPS;

    #[test]
    fn the_guide_is_a_valid_agent_that_opens_no_task() {
        let draft = guide_draft().normalized().expect("a valid agent");
        assert_eq!(draft.name, GUIDE_NAME);
        assert!(draft.role.chars().count() <= MAX_AGENT_ROLE_CHARS);
        assert!(!draft.auto_start_tasks && draft.may_call.is_empty());
        assert_eq!(
            draft.tool_access.groups.get("tasks"),
            Some(&AccessLevel::None)
        );
        for group in draft.tool_access.groups.keys() {
            assert!(DCC_TOOL_GROUPS.contains(&group.as_str()), "{group}");
        }
        // It names the tools it works with, and never to grant.
        for tool in [
            "dcc_team_state",
            "dcc_help",
            "dcc_propose_agent",
            "dcc_propose_source",
        ] {
            assert!(GUIDE_ROLE.contains(tool), "{tool}");
        }
        assert!(GUIDE_ROLE.contains("You never grant an agent anything"));
    }
}
