//! An agent built by conversation (Tucano's Tuca): after at most three
//! questions an agent proposes a new teammate with `dcc_propose_agent` —
//! its name, its role, a mascot, the projects it works in, the least access
//! it needs and a first request to try it. The proposal waits as a card the
//! person creates, edits first or discards; nothing runs before that, and
//! the first request only goes when the person sends it.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::agent::{
    AgentAvatar, ResidentAgentDraft, AGENT_AVATAR_ACCESSORIES, AGENT_AVATAR_COLORS,
    AGENT_AVATAR_SHAPES,
};

pub const PROPOSE_AGENT_TOOL: &str = "dcc_propose_agent";
/// Proposals waiting for the person at once.
pub const MAX_WAITING_PROPOSALS: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentProposal {
    pub id: String,
    /// The agent that proposed it.
    pub agent_id: String,
    pub session_id: String,
    /// The new agent; its `kickoff_prompt` is the first request to try it.
    pub draft: ResidentAgentDraft,
    pub created_at: String,
}

/// A mascot of its own for a name, when the proposal names none: the same
/// name always gets the same one.
pub fn mascot_for(name: &str) -> AgentAvatar {
    let hash = name
        .to_lowercase()
        .bytes()
        .fold(2_166_136_261_u32, |hash, byte| {
            (hash ^ u32::from(byte)).wrapping_mul(16_777_619)
        }) as usize;
    let pick = |list: &[&str], shift: u32| list[(hash >> shift) % list.len()].to_string();
    AgentAvatar {
        color: pick(AGENT_AVATAR_COLORS, 0),
        arms: 4,
        eyes: "round".to_string(),
        shape: pick(AGENT_AVATAR_SHAPES, 8),
        // A built-in look for the rest; "none" is one of them.
        accessory: pick(AGENT_AVATAR_ACCESSORIES, 16),
    }
}

/// A draft from what the tool gives, checked like the editor's.
pub fn proposal_draft(
    name: &str,
    role: &str,
    first_request: &str,
    avatar: Option<AgentAvatar>,
    projects: Vec<String>,
    tool_access: super::agent_access::ToolAccess,
) -> Result<ResidentAgentDraft, String> {
    ResidentAgentDraft {
        name: name.to_string(),
        role: role.to_string(),
        kickoff_prompt: first_request.to_string(),
        offer_prompt: String::new(),
        extra_instructions: String::new(),
        provider_id: None,
        model: None,
        effort: None,
        avatar: avatar.unwrap_or_else(|| mascot_for(name.trim())),
        projects,
        monthly_token_limit: None,
        tool_access,
        auto_start_tasks: false,
        may_call: Vec::new(),
        account_id: None,
    }
    .normalized()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_proposal_is_a_checked_draft_with_a_mascot_of_its_own() {
        let draft = proposal_draft(
            " QA do Checkout ",
            "Teste o checkout de ponta a ponta.",
            "Rode o fluxo de compra no staging.",
            None,
            Vec::new(),
            Default::default(),
        )
        .unwrap();
        assert_eq!(draft.name, "QA do Checkout");
        assert_eq!(draft.kickoff_prompt, "Rode o fluxo de compra no staging.");
        assert_eq!(draft.avatar, mascot_for("QA do Checkout"));
        assert_eq!(mascot_for("qa do checkout"), mascot_for("QA do Checkout"));
        assert!(proposal_draft("", "x", "", None, Vec::new(), Default::default()).is_err());
    }
}
