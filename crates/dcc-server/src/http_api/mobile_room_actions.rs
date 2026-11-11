//! What the phone does with a room's cards and answers, through the same
//! functions the desktop's room calls — the relay never forwards
//! `/api/v1/agents/`, so the phone has these under `/api/v1/mobile/room/`:
//! 👍 / 👎 on an answer (`reactions`), "Guardar como regra?" after a 👎
//! (`rules`, the person's note in the agent's memory), "Criar" /
//! "Descartar" on a proposed agent, routine or source (`proposals`) and
//! "Continuar mesmo assim" on an agent past its monthly limit (`limit`).

use dcc_core::domain::agent_reaction::Reaction;
use dcc_infra::db::SqliteSessionRepo;

use super::room_feed::{read_team, Team};
use super::*;

/// The longest rule kept, as the desktop's field.
pub(super) const MAX_RULE_CHARS: usize = 500;

fn bad_request(error: impl ToString) -> HttpApiError {
    HttpApiError::bad_request(error.to_string())
}

fn team(backend: &HttpBackend) -> Result<Team, HttpApiError> {
    let repo = SqliteSessionRepo::open_read_only(&backend.workspace.db_path)
        .map_err(|error| HttpApiError::internal(error.to_string()))?;
    read_team(&repo).map_err(HttpApiError::internal)
}

/// The agent whose own conversation `session_id` is.
pub(super) fn conversation_agent(team: &Team, session_id: &str) -> Option<String> {
    team.bindings
        .iter()
        .find(|binding| binding.session_id == session_id && Team::is_home(binding))
        .map(|binding| binding.agent_id.clone())
}

fn known_agent(team: &Team, agent_id: &str) -> Result<(), HttpApiError> {
    team.agents
        .iter()
        .any(|agent| agent.id == agent_id)
        .then_some(())
        .ok_or_else(|| HttpApiError::not_found("agent not found"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReactionInput {
    session_id: String,
    line_key: String,
    /// `None` takes it back.
    #[serde(default)]
    reaction: Option<Reaction>,
}

/// `POST /api/v1/mobile/room/reactions`
pub(super) async fn react(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<ReactionInput>,
) -> Result<Json<Value>, HttpApiError> {
    let session_id = input.session_id.trim();
    let agent = conversation_agent(&team(&backend)?, session_id)
        .ok_or_else(|| HttpApiError::not_found("conversation not found"))?;
    backend
        .sessions
        .set_agent_reaction(&agent, session_id, input.line_key.trim(), input.reaction)
        .map_err(bad_request)?;
    Ok(Json(json!({ "ok": true, "reaction": input.reaction })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RuleInput {
    agent_id: String,
    body: String,
}

/// The rule as kept: trimmed, and not empty nor longer than the field.
pub(super) fn rule_body(body: &str) -> Option<&str> {
    let body = body.trim();
    (!body.is_empty() && body.chars().count() <= MAX_RULE_CHARS).then_some(body)
}

/// `POST /api/v1/mobile/room/rules`
pub(super) async fn keep_rule(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<RuleInput>,
) -> Result<Json<Value>, HttpApiError> {
    let agent_id = input.agent_id.trim();
    known_agent(&team(&backend)?, agent_id)?;
    let body = rule_body(&input.body).ok_or_else(|| bad_request("empty or too long rule"))?;
    let note = backend
        .sessions
        .add_agent_memory_note(agent_id, body, None)
        .map_err(bad_request)?;
    Ok(Json(json!({ "ok": true, "noteId": note.id })))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum ProposalKind {
    Agent,
    Routine,
    Source,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum ProposalAction {
    Create,
    Discard,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProposalInput {
    kind: ProposalKind,
    id: String,
    /// The agent that proposed it (a routine's or a source's).
    #[serde(default)]
    agent_id: String,
    action: ProposalAction,
}

/// `POST /api/v1/mobile/room/proposals`
pub(super) async fn answer_proposal(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<ProposalInput>,
) -> Result<Json<Value>, HttpApiError> {
    let (id, agent) = (input.id.trim(), input.agent_id.trim());
    let sessions = &backend.sessions;
    let (found, created) = match (input.kind, input.action) {
        (ProposalKind::Agent, ProposalAction::Create) => {
            let made = sessions.accept_agent_proposal(id).map_err(bad_request)?;
            (made.is_some(), made.map(|agent| agent.id))
        }
        (ProposalKind::Agent, ProposalAction::Discard) => (
            sessions.dismiss_agent_proposal(id).map_err(bad_request)?,
            None,
        ),
        (ProposalKind::Routine, ProposalAction::Create) => (
            sessions
                .accept_agent_routine(agent, id)
                .map_err(bad_request)?,
            None,
        ),
        (ProposalKind::Routine, ProposalAction::Discard) => (
            sessions
                .delete_agent_routine(agent, id)
                .map_err(bad_request)?,
            None,
        ),
        (ProposalKind::Source, ProposalAction::Create) => (
            sessions
                .set_agent_source(agent, id, true)
                .map_err(bad_request)?,
            None,
        ),
        (ProposalKind::Source, ProposalAction::Discard) => (
            sessions
                .delete_agent_source(agent, id)
                .map_err(bad_request)?,
            None,
        ),
    };
    if !found {
        return Err(HttpApiError::not_found("proposal not found"));
    }
    Ok(Json(json!({ "ok": true, "agentId": created })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LimitInput {
    agent_id: String,
}

/// `POST /api/v1/mobile/room/limit`: "Continuar mesmo assim", until the
/// month ends.
pub(super) async fn let_past_limit(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<LimitInput>,
) -> Result<Json<Value>, HttpApiError> {
    let changed = backend
        .sessions
        .let_agent_past_limit(input.agent_id.trim())
        .map_err(bad_request)?;
    if !changed {
        return Err(HttpApiError::not_found("agent not found"));
    }
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::super::room_feed::tests::team;
    use super::*;

    #[test]
    fn a_reaction_goes_only_to_an_agents_own_conversation() {
        let team = team();
        assert_eq!(conversation_agent(&team, "s-new").as_deref(), Some("rev"));
        // A task's conversation, or none of the team's.
        assert_eq!(conversation_agent(&team, "s-task"), None);
        assert_eq!(conversation_agent(&team, "elsewhere"), None);
    }

    #[test]
    fn a_rule_is_trimmed_and_kept_short() {
        assert_eq!(rule_body("  sempre cite o PR "), Some("sempre cite o PR"));
        assert_eq!(rule_body("   "), None);
        assert_eq!(rule_body(&"x".repeat(MAX_RULE_CHARS + 1)), None);
        assert!(rule_body(&"é".repeat(MAX_RULE_CHARS)).is_some());
    }

    #[test]
    fn proposals_read_their_kind_and_action() {
        let input: ProposalInput = serde_json::from_value(json!({
            "kind": "routine", "id": "r-1", "agentId": "dev", "action": "create"
        }))
        .unwrap();
        assert_eq!(
            (input.kind, input.action),
            (ProposalKind::Routine, ProposalAction::Create)
        );
        assert!(serde_json::from_value::<ProposalInput>(json!({
            "kind": "prompt", "id": "x", "action": "create"
        }))
        .is_err());
    }
}
