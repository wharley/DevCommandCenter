//! `dcc_propose_agent`: a resident agent proposes a new teammate to the
//! person (`dcc_core::domain::agent_proposal`) after at most three
//! questions. It waits as a card in the team room — Criar, Editar antes,
//! Descartar — and nothing runs before the person answers. A session that
//! runs as no agent gets an error.

use std::collections::BTreeMap;

use dcc_core::domain::agent::{
    AgentAvatar, AGENT_AVATAR_ACCESSORIES, AGENT_AVATAR_COLORS, AGENT_AVATAR_SHAPES,
    MAX_AGENT_KICKOFF_CHARS, MAX_AGENT_NAME_CHARS,
};
use dcc_core::domain::agent_access::{AccessLevel, DCC_TOOL_GROUPS};
use dcc_core::domain::agent_proposal::PROPOSE_AGENT_TOOL;
use dcc_core::domain::session::SessionId;
use dcc_tauri::state::{ProposedAgent, SessionCommandState};
use serde::Deserialize;
use serde_json::{json, Value};

/// The role a proposal writes, shorter than the editor's.
const MAX_PROPOSED_ROLE_CHARS: usize = 4_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Mascot {
    shape: String,
    color: String,
    #[serde(default = "no_accessory")]
    accessory: String,
}

fn no_accessory() -> String {
    "none".to_string()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ProposeArgs {
    name: String,
    role: String,
    first_request: String,
    #[serde(default)]
    mascot: Option<Mascot>,
    #[serde(default)]
    projects: Vec<String>,
    #[serde(default)]
    dcc_tools: BTreeMap<String, AccessLevel>,
    #[serde(default)]
    connectors: BTreeMap<String, AccessLevel>,
}

impl ProposeArgs {
    fn checked(self) -> Option<ProposedAgent> {
        let filled = |text: &str, max: usize| {
            let count = text.trim().chars().count();
            count > 0 && count <= max
        };
        let fits = filled(&self.name, MAX_AGENT_NAME_CHARS)
            && filled(&self.role, MAX_PROPOSED_ROLE_CHARS)
            && filled(&self.first_request, MAX_AGENT_KICKOFF_CHARS)
            && self
                .dcc_tools
                .keys()
                .all(|group| DCC_TOOL_GROUPS.contains(&group.as_str()));
        let avatar = match self.mascot {
            Some(mascot) => {
                let known = AGENT_AVATAR_SHAPES.contains(&mascot.shape.as_str())
                    && AGENT_AVATAR_COLORS.contains(&mascot.color.as_str())
                    && AGENT_AVATAR_ACCESSORIES.contains(&mascot.accessory.as_str());
                if !known {
                    return None;
                }
                Some(AgentAvatar {
                    color: mascot.color,
                    arms: 4,
                    eyes: "round".to_string(),
                    shape: mascot.shape,
                    accessory: mascot.accessory,
                })
            }
            None => None,
        };
        fits.then_some(ProposedAgent {
            name: self.name,
            role: self.role,
            first_request: self.first_request,
            avatar,
            projects: self.projects,
            groups: self.dcc_tools,
            connectors: self.connectors,
        })
    }
}

pub fn is_agent_proposal_tool(name: &str) -> bool {
    name == PROPOSE_AGENT_TOOL
}

pub fn definition() -> Value {
    let level = json!({"type": "string", "enum": ["none", "read", "write"]});
    json!({
        "name": PROPOSE_AGENT_TOOL,
        "description": "Only when you run as a DCC resident agent and the person wants a new agent: after at most three short questions (what it does, with an example; in which projects or systems; in which tone — skip what they already said), propose it. It waits as a card the person creates, edits first or discards; nothing runs before that. Give it the least access its job needs. At most one per turn; tell the person the card is there and stop.",
        "inputSchema": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "name": {"type": "string", "minLength": 1, "maxLength": MAX_AGENT_NAME_CHARS, "description": "A short name in the person's language, e.g. \"QA do Checkout\"."},
                "role": {"type": "string", "minLength": 1, "maxLength": MAX_PROPOSED_ROLE_CHARS, "description": "Its instructions, in the second person: what it does, how, in which tone, and what it never does."},
                "firstRequest": {"type": "string", "minLength": 1, "maxLength": MAX_AGENT_KICKOFF_CHARS, "description": "A first, small request to try it, in the person's language; it goes only when the person sends it."},
                "mascot": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "shape": {"type": "string", "enum": AGENT_AVATAR_SHAPES},
                        "color": {"type": "string", "enum": AGENT_AVATAR_COLORS},
                        "accessory": {"type": "string", "enum": AGENT_AVATAR_ACCESSORIES}
                    },
                    "required": ["shape", "color"]
                },
                "projects": {"type": "array", "maxItems": 20, "items": {"type": "string", "maxLength": 128}, "description": "Project names as dcc_overview shows them; none: every project."},
                "dccTools": {"type": "object", "additionalProperties": false, "properties": {"browser": level, "computer": level, "device": level, "delegation": level}, "description": "DCC's own tool groups it may use; missing: read and write."},
                "connectors": {"type": "object", "additionalProperties": level, "description": "By connector name as the person installed it (\"Slack\"); missing: read and write."}
            },
            "required": ["name", "role", "firstRequest"]
        },
        "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false}
    })
}

pub fn call_is_well_formed(arguments: &Option<Value>) -> bool {
    arguments.as_ref().is_some_and(|arguments| {
        serde_json::from_value::<ProposeArgs>(arguments.clone())
            .ok()
            .and_then(ProposeArgs::checked)
            .is_some()
    })
}

fn text(message: String, is_error: bool) -> Value {
    json!({"content": [{"type": "text", "text": message}], "isError": is_error})
}

pub fn dispatch(
    sessions: &SessionCommandState,
    session_id: &str,
    arguments: Option<Value>,
) -> Value {
    let Some(proposed) = arguments
        .and_then(|arguments| serde_json::from_value::<ProposeArgs>(arguments).ok())
        .and_then(ProposeArgs::checked)
    else {
        return text(
            "Pass name, role and firstRequest; mascot, projects, dccTools and connectors are optional.".to_string(),
            true,
        );
    };
    match sessions.propose_agent_for_session(&SessionId(session_id.to_string()), proposed) {
        Ok((_, proposal)) => text(
            format!(
                "Proposed to the person: the agent {}. It waits as a card they create, edit or discard; tell them it is there and stop.",
                proposal.draft.name
            ),
            false,
        ),
        Err(error) => text(error.to_string(), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_proposal_needs_a_name_a_role_and_a_first_request() {
        let good = Some(json!({
            "name": "QA do Checkout",
            "role": "Teste o checkout.",
            "firstRequest": "Rode o fluxo de compra.",
            "mascot": {"shape": "ghost", "color": "violet"},
            "dccTools": {"browser": "read"},
            "connectors": {"Slack": "read"}
        }));
        assert!(call_is_well_formed(&good));
        for bad in [
            json!({"name": "QA", "role": "x"}),
            json!({"name": " ", "role": "x", "firstRequest": "y"}),
            json!({"name": "QA", "role": "x", "firstRequest": "y", "mascot": {"shape": "cat", "color": "blue"}}),
            json!({"name": "QA", "role": "x", "firstRequest": "y", "dccTools": {"shell": "read"}}),
            json!({"name": "QA", "role": "x", "firstRequest": "y", "extra": 1}),
        ] {
            assert!(!call_is_well_formed(&Some(bad.clone())), "{bad}");
        }
        assert_eq!(definition()["name"], PROPOSE_AGENT_TOOL);
    }
}
