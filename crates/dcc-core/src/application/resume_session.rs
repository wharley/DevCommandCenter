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
pub struct ResumeSessionInput {
    pub session_id: SessionId,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ResumeSessionOutput {
    pub session: Session,
    pub projection: SessionProjection,
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub async fn resume_session<S, E, B>(
    sessions: &S,
    session_events: &E,
    events: &B,
    input: ResumeSessionInput,
) -> Result<ResumeSessionOutput>
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

    if session.state == SessionState::Active {
        return Ok(ResumeSessionOutput {
            session,
            projection,
        });
    }

    let now = now_iso();
    let sequence = history.last().map(|event| event.sequence + 1).unwrap_or(1);
    let resumed = SessionEventRecord {
        event_id: Uuid::new_v4().to_string(),
        session_id: input.session_id.clone(),
        sequence,
        occurred_at: now.clone(),
        kind: SessionEventKind::SessionResumed,
    };

    session_events.append_event(&resumed).await?;
    session.state = SessionState::Active;
    session.updated_at = now.clone();
    sessions.save_session(&session).await?;
    events
        .publish(CoreEvent::SessionResumed {
            session_id: input.session_id.0.clone(),
        })
        .await?;

    let mut replay = history;
    replay.push(resumed);
    let projection = SessionProjection::fold(&replay).expect("session projection exists");

    Ok(ResumeSessionOutput {
        session,
        projection,
    })
}
