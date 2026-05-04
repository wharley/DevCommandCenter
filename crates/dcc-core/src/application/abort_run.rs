use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use crate::{
    domain::session::{
        Session, SessionEventKind, SessionEventRecord, SessionId, SessionProjection, SessionState,
    },
    ports::{CoreEvent, EventBus, SessionEventRepo, SessionRepo},
    Result,
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AbortRunInput {
    pub session_id: SessionId,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AbortRunOutput {
    pub session: Session,
    pub projection: SessionProjection,
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub async fn abort_run<S, E, B>(
    sessions: &S,
    session_events: &E,
    events: &B,
    input: AbortRunInput,
) -> Result<AbortRunOutput>
where
    S: SessionRepo + Sync,
    E: SessionEventRepo + Sync,
    B: EventBus + Sync,
{
    let mut session = sessions
        .get_session(&input.session_id)
        .await?
        .ok_or_else(|| crate::CoreError::Repository("session not found".to_string()))?;

    let history = session_events
        .list_events_by_session(&input.session_id)
        .await?;
    let projection = SessionProjection::fold(&history)
        .ok_or_else(|| crate::CoreError::Repository("session history is empty".to_string()))?;

    if projection.state != SessionState::Active {
        return Err(crate::CoreError::InvalidInput(
            "session must be active to abort a run".to_string(),
        ));
    }

    let active_turn_id = projection.active_turn_id.clone().ok_or_else(|| {
        crate::CoreError::InvalidInput("session has no active turn to abort".to_string())
    })?;
    let now = now_iso();
    let sequence = history.last().map(|event| event.sequence + 1).unwrap_or(1);
    let turn_aborted = SessionEventRecord {
        event_id: Uuid::new_v4().to_string(),
        session_id: input.session_id.clone(),
        sequence,
        occurred_at: now.clone(),
        kind: SessionEventKind::TurnAborted {
            turn_id: active_turn_id.clone(),
            reason: input.reason.clone(),
        },
    };
    let session_aborted = SessionEventRecord {
        event_id: Uuid::new_v4().to_string(),
        session_id: input.session_id.clone(),
        sequence: sequence + 1,
        occurred_at: now.clone(),
        kind: SessionEventKind::SessionAborted {
            reason: input.reason.clone(),
        },
    };

    session_events.append_event(&turn_aborted).await?;
    session_events.append_event(&session_aborted).await?;
    session.state = SessionState::Aborted;
    session.updated_at = now.clone();
    sessions.save_session(&session).await?;
    events
        .publish(CoreEvent::SessionTurnAborted {
            session_id: input.session_id.0.clone(),
            turn_id: active_turn_id.0.clone(),
            reason: input.reason.clone(),
        })
        .await?;
    events
        .publish(CoreEvent::SessionAborted {
            session_id: input.session_id.0.clone(),
            reason: input.reason,
        })
        .await?;

    let mut replay = history;
    replay.push(turn_aborted);
    replay.push(session_aborted);
    let projection = SessionProjection::fold(&replay).expect("session projection exists");

    Ok(AbortRunOutput {
        session,
        projection,
    })
}
