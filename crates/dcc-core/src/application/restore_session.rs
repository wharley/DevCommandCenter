use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    domain::session::SessionId,
    ports::{SessionRepo, ThreadRepo},
    Result,
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RestoreSessionInput {
    pub session_id: SessionId,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RestoreSessionOutput {
    pub session_id: SessionId,
}

pub async fn restore_session<S, T>(
    sessions: &S,
    threads: &T,
    input: RestoreSessionInput,
) -> Result<RestoreSessionOutput>
where
    S: SessionRepo + Sync,
    T: ThreadRepo + Sync,
{
    sessions
        .get_session(&input.session_id)
        .await?
        .ok_or_else(|| crate::CoreError::Repository("session not found".to_string()))?;

    let mut thread = threads
        .find_thread_by_session_id(&input.session_id)
        .await?
        .ok_or_else(|| crate::CoreError::Repository("thread not found for session".to_string()))?;
    thread.archived_at = None;
    threads.save_thread(&thread).await?;

    Ok(RestoreSessionOutput {
        session_id: input.session_id,
    })
}
