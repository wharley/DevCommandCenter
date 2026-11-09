//! Starting points for a new agent ("Novo agente"): a role and a look the
//! person can change before saving. A template is not a preset: the agent
//! it makes is entirely the person's, with no code of its own in DCC.

use crate::domain::agent::{AgentAvatar, ResidentAgentDraft};

/// The templates "Novo agente" offers, in order (`agents.templates.*`).
pub const AGENT_TEMPLATES: &[&str] = &["research", "qa"];

pub const RESEARCH_ROLE: &str = "You research what the person or a teammate asks: libraries, APIs, approaches, competitors, prices, documentation. Search the web and read the sources before answering; cite each factual claim with its link, and say so when you could not verify something.\n\
Compare at most a few options, each with what it is good and bad at for this case, and end with your recommendation and why. When the question is about one of the person's projects, read that project's code through a task before recommending.\n\
You do not write or change code. When the person decides, the work goes to a task of the right project.\n\
Reply in the language the person writes in, short and direct.";

pub const QA_ROLE: &str = "You check that a change works, with proof. In a task, read what changed, then run what proves it: the project's tests and checks, the steps to reproduce the bug, and the app itself in the browser or the iOS simulator when the change is visible there. Report what you ran, what passed and what failed, with the command output or a screenshot.\n\
You only check: never fix, commit or push. When something fails, say where and how to reproduce it, and leave the fix to the person or the task's agent.\n\
Reply in the language the person writes in, short and direct.";

pub const QA_KICKOFF: &str = "Check that the changes in this workspace work.";

/// The draft a template starts from, named in the person's language.
pub fn template_draft(template: &str, name: &str) -> Option<ResidentAgentDraft> {
    let (role, kickoff, color, shape, accessory) = match template {
        "research" => (RESEARCH_ROLE, "", "green", "drop", "magnifier"),
        "qa" => (QA_ROLE, QA_KICKOFF, "violet", "ghost", "cap"),
        _ => return None,
    };
    Some(ResidentAgentDraft {
        name: name.trim().to_string(),
        role: role.to_string(),
        kickoff_prompt: kickoff.to_string(),
        offer_prompt: String::new(),
        extra_instructions: String::new(),
        provider_id: None,
        model: None,
        effort: None,
        avatar: AgentAvatar {
            color: color.to_string(),
            arms: 4,
            eyes: "round".to_string(),
            shape: shape.to_string(),
            accessory: accessory.to_string(),
        },
        projects: Vec::new(),
        monthly_token_limit: None,
        tool_access: Default::default(),
        auto_start_tasks: false,
        may_call: Vec::new(),
        account_id: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::agent::MAX_AGENT_ROLE_CHARS;

    #[test]
    fn every_template_makes_a_valid_agent_of_the_person() {
        for template in AGENT_TEMPLATES {
            let draft = template_draft(template, " Nome ").unwrap();
            assert!(draft.role.chars().count() <= MAX_AGENT_ROLE_CHARS);
            let draft = draft.normalized().unwrap();
            assert_eq!(draft.name, "Nome");
            assert_eq!(draft.offer_prompt, "");
        }
        assert!(template_draft("nope", "x").is_none());
        assert!(QA_ROLE.contains("never fix"));
    }
}
