use serde::{Deserialize, Serialize};
use specta::Type;

use super::{project::ProjectId, session::SessionId};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct ThreadId(pub String);

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct Thread {
    pub id: ThreadId,
    pub project_id: ProjectId,
    pub session_id: Option<SessionId>,
    pub title: String,
    pub archived_at: Option<String>,
}
