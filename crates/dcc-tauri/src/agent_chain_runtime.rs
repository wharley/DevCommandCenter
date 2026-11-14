//! A pass without the person (`dcc_core::domain::agent_chain`), run where
//! the desktop sees every turn end, so it happens with the team room
//! closed too: when an agent's own conversation finishes a turn whose
//! answer ends with "@Teammate …" and the agent may call that teammate
//! itself, DCC sends the teammate the same message the card would, in its
//! own conversation, marked with the chain's step. A teammate with no
//! conversation yet waits for the person's click, which makes it.
//!
//! And the way back: when the turn that ended was itself a pass and passes
//! nothing on, the chain is over, and what its agent answered goes once to
//! the agent the chain started with, in that one's own conversation, to
//! close it with the person.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use dcc_core::application::SendTurnInput;
use dcc_core::domain::agent::ReviewerPresetText;
use dcc_core::domain::agent_chain::{
    auto_handoff_prompt, next_step, owes_return, passed_answer, passed_by, return_prompt,
    MAX_CHAIN_STEPS,
};
use dcc_core::domain::agent_routine::QUIET_ANSWER;
use dcc_core::domain::provider::ProviderApprovalPolicy;
use dcc_core::domain::session::{SessionEventKind, SessionId, SessionProjection};
use dcc_core::domain::team_room::{handoff, Teammate};
use dcc_core::domain::team_room_lines::lines_of;
use dcc_core::domain::timeline::project_session;
use dcc_core::ports::SessionEventRepo;

use crate::state::SessionCommandState;

/// The answers already passed in this run, so a turn's end seen twice
/// passes once.
fn passed() -> &'static Mutex<HashSet<String>> {
    static PASSED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    PASSED.get_or_init(|| Mutex::new(HashSet::new()))
}

/// After a turn of `session_id` ended: the pass, when one is due.
pub async fn after_agent_turn(state: &SessionCommandState, session_id: &SessionId) {
    if let Err(error) = pass_if_allowed(state, session_id).await {
        eprintln!("[DCC] agent chain: {error}");
    }
}

async fn pass_if_allowed(
    state: &SessionCommandState,
    session_id: &SessionId,
) -> Result<(), String> {
    let text = |error: dcc_core::CoreError| error.to_string();
    let Some(agent) = state.session_agent(session_id).map_err(text)? else {
        return Ok(());
    };
    if !state.is_agent_conversation(session_id).map_err(text)? {
        return Ok(());
    }
    let events = SessionEventRepo::list_events_by_session(state, session_id)
        .await
        .map_err(text)?;
    if SessionProjection::fold(&events)
        .and_then(|projection| projection.active_turn_id)
        .is_some()
    {
        return Ok(());
    }
    // Only a turn that finished: a stopped one passes nothing.
    let Some(turn_id) = events
        .iter()
        .rev()
        .find_map(|event| match &event.kind {
            SessionEventKind::TurnCompleted { turn_id } => Some(Some(turn_id.0.clone())),
            SessionEventKind::TurnAborted { .. } => Some(None),
            _ => None,
        })
        .flatten()
    else {
        return Ok(());
    };
    let key = format!("{}:{turn_id}", session_id.0);
    let prompt = events
        .iter()
        .find_map(|event| match &event.kind {
            SessionEventKind::TurnStarted {
                turn_id: id,
                prompt,
                ..
            } if id.0 == turn_id => Some(prompt.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let messages = project_session(&events, &session_id.0);
    let Some(answer) = lines_of(&agent.id, &session_id.0, &messages, &HashMap::new())
        .into_iter()
        .find(|line| line.key == key)
        .map(|line| line.text)
    else {
        return Ok(());
    };
    let team = state
        .list_stored_agents(&ReviewerPresetText::default())
        .map_err(text)?;
    let mates: Vec<Teammate<'_>> = team
        .iter()
        .map(|mate| Teammate {
            id: &mate.id,
            name: &mate.name,
            preset: mate.preset.as_deref(),
        })
        .collect();
    // A pass the agent may make itself; none: the chain ends here, and
    // its result goes back to who started it.
    let handed = handoff(&answer, &mates, &agent.id).and_then(|(mate, request)| {
        next_step(&prompt, &agent.may_call, mate.id, mate.preset).map(|step| (mate, request, step))
    });
    let Some((mate, request, step)) = handed else {
        return return_if_owed(state, session_id, &agent, &prompt, &answer, &key, &team).await;
    };
    // The teammate's conversation in the same room; none there yet waits
    // for the person's click on the card, which makes it.
    let room = state.session_room(session_id).map_err(text)?;
    let Some(to_session) = state
        .agent_room_session(mate.id, room.as_deref())
        .map_err(text)?
    else {
        return Ok(());
    };
    if !passed()
        .lock()
        .map_err(|error| error.to_string())?
        .insert(key.clone())
    {
        return Ok(());
    }
    let input = SendTurnInput {
        approval_policy: Some(ProviderApprovalPolicy::Ask),
        ..serde_json::from_value(serde_json::json!({
            "sessionId": to_session,
            "prompt": auto_handoff_prompt(&agent.name, mate.name, &request, &answer, &key, step),
        }))
        .map_err(|error| error.to_string())?
    };
    crate::commands::session_commands::send_turn_with_state(state, input)
        .await
        .map(|_| ())
}

/// The turn `key` of `agent` ended a chain: its answer goes back, once, to
/// the agent the chain started with.
async fn return_if_owed(
    state: &SessionCommandState,
    session_id: &SessionId,
    agent: &dcc_core::domain::agent::ResidentAgent,
    prompt: &str,
    answer: &str,
    key: &str,
    team: &[dcc_core::domain::agent::ResidentAgent],
) -> Result<(), String> {
    let text = |error: dcc_core::CoreError| error.to_string();
    if !owes_return(prompt) || answer.trim() == QUIET_ANSWER {
        return Ok(());
    }
    // Back through the passes DCC made itself, each naming the answer it
    // came from, to the conversation the first one left.
    let mut root: Option<SessionId> = None;
    let mut current = prompt.to_string();
    for _ in 0..MAX_CHAIN_STEPS {
        let Some((from_session, from_turn)) =
            passed_answer(&current).and_then(|from| from.split_once(':'))
        else {
            break;
        };
        let from_session = SessionId(from_session.to_string());
        let events = SessionEventRepo::list_events_by_session(state, &from_session)
            .await
            .map_err(text)?;
        let started = events.iter().find_map(|event| match &event.kind {
            SessionEventKind::TurnStarted {
                turn_id, prompt, ..
            } if turn_id.0 == from_turn => Some(prompt.clone()),
            _ => None,
        });
        root = Some(from_session);
        match started {
            Some(started) => current = started,
            None => break,
        }
    }
    // A pass the person clicked names no answer: who passed it, by name,
    // in the same room.
    let root = match root {
        Some(root) => root,
        None => {
            let Some(from) = passed_by(prompt).and_then(|name| {
                team.iter()
                    .find(|mate| mate.name.trim().eq_ignore_ascii_case(name))
            }) else {
                return Ok(());
            };
            let room = state.session_room(session_id).map_err(text)?;
            match state
                .agent_room_session(&from.id, room.as_deref())
                .map_err(text)?
            {
                Some(session) => SessionId(session),
                None => return Ok(()),
            }
        }
    };
    let Some(first) = state.session_agent(&root).map_err(text)? else {
        return Ok(());
    };
    if first.id == agent.id || root == *session_id {
        return Ok(());
    }
    if !passed()
        .lock()
        .map_err(|error| error.to_string())?
        .insert(format!("return:{key}"))
    {
        return Ok(());
    }
    let input = SendTurnInput {
        approval_policy: Some(ProviderApprovalPolicy::Ask),
        ..serde_json::from_value(serde_json::json!({
            "sessionId": root.0,
            "prompt": return_prompt(&agent.name, &first.name, answer),
        }))
        .map_err(|error| error.to_string())?
    };
    crate::commands::session_commands::send_turn_with_state(state, input)
        .await
        .map(|_| ())
}
