use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use crate::{
    domain::session::{
        Session, SessionEventKind, SessionEventRecord, SessionId, SessionProjection, TurnId,
    },
    ports::{CoreEvent, EventBus, SessionEventRepo, SessionRepo},
    Result,
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SteerTurnInput {
    pub session_id: SessionId,
    pub prompt: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SteerTurnOutput {
    pub session: Session,
    pub turn_id: TurnId,
    pub projection: SessionProjection,
}

/// Resolve and validate the active turn before contacting the provider.
pub async fn active_turn_for_steer<S, E>(
    sessions: &S,
    session_events: &E,
    input: &SteerTurnInput,
) -> Result<(Session, TurnId)>
where
    S: SessionRepo + Sync,
    E: SessionEventRepo + Sync,
{
    let prompt = input.prompt.trim();
    if prompt.is_empty() {
        return Err(crate::CoreError::InvalidInput(
            "steering prompt must not be empty".to_string(),
        ));
    }

    let session = sessions
        .get_session(&input.session_id)
        .await?
        .ok_or_else(|| crate::CoreError::Repository("session not found".to_string()))?;
    let history = session_events
        .list_events_by_session(&input.session_id)
        .await?;
    let projection = SessionProjection::fold(&history)
        .ok_or_else(|| crate::CoreError::Repository("session history is empty".to_string()))?;
    let turn_id = projection.active_turn_id.ok_or_else(|| {
        crate::CoreError::InvalidInput("session has no active turn to steer".to_string())
    })?;

    Ok((session, turn_id))
}

/// Persist provider-accepted guidance in the durable timeline.
pub async fn record_turn_steer<S, E, B>(
    sessions: &S,
    session_events: &E,
    events: &B,
    input: SteerTurnInput,
    turn_id: TurnId,
) -> Result<SteerTurnOutput>
where
    S: SessionRepo + Sync,
    E: SessionEventRepo + Sync,
    B: EventBus + Sync,
{
    let session = sessions
        .get_session(&input.session_id)
        .await?
        .ok_or_else(|| crate::CoreError::Repository("session not found".to_string()))?;
    let mut history = session_events
        .list_events_by_session(&input.session_id)
        .await?;
    let occurred_at = chrono::Utc::now().to_rfc3339();
    let event = SessionEventRecord {
        event_id: Uuid::new_v4().to_string(),
        session_id: input.session_id.clone(),
        sequence: history.last().map(|event| event.sequence + 1).unwrap_or(1),
        occurred_at,
        kind: SessionEventKind::TurnSteered {
            turn_id: turn_id.clone(),
            prompt: input.prompt.clone(),
        },
    };
    session_events.append_event(&event).await?;
    events
        .publish(CoreEvent::SessionTurnSteered {
            session_id: input.session_id.0.clone(),
            turn_id: turn_id.0.clone(),
            prompt: input.prompt,
        })
        .await?;

    history.push(event);
    let projection = SessionProjection::fold(&history).expect("session projection exists");
    Ok(SteerTurnOutput {
        session,
        turn_id,
        projection,
    })
}
