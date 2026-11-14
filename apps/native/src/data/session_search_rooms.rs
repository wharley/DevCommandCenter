//! A result of "Sessões passadas" that is an agent's conversation in a
//! room: the search covers every session, the rooms' included, but a room
//! is not a task. Such a result reads "Sala · Dev" and, when it is the
//! conversation the room shows now, opens there with the query in the
//! room's find bar; an older one opens whole, as any session does.

use dcc_core::domain::agent_conversation::{home_agent_id, home_project_id};
use dcc_core::domain::session::SessionSearchResult;

use super::agents::AgentRecord;

/// Where a room's conversation is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomHit {
    pub agent_id: String,
    pub agent_name: String,
    /// The project whose room it is; none: Geral.
    pub room: Option<String>,
    /// The agent's latest conversation in that room: what the room shows.
    pub shown: bool,
}

/// `result` as a room's conversation, if it is one of `agents`'.
pub fn room_hit(result: &SessionSearchResult, agents: &[AgentRecord]) -> Option<RoomHit> {
    let agent_id = home_agent_id(&result.project_id.0)?;
    let agent = agents.iter().find(|agent| agent.id == agent_id)?;
    let home = home_project_id(&agent.id);
    let session = agent
        .sessions
        .iter()
        .find(|session| session.session_id == result.session_id.0 && session.project_id == home)?;
    let latest = agent
        .sessions
        .iter()
        .filter(|other| other.project_id == home && other.room == session.room)
        .max_by(|left, right| left.updated_at.cmp(&right.updated_at));
    Some(RoomHit {
        agent_id: agent.id.clone(),
        agent_name: agent.name.clone(),
        room: session.room.clone(),
        shown: latest.is_some_and(|latest| latest.session_id == session.session_id),
    })
}
