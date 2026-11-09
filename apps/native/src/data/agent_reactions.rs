//! 👍 / 👎 on an agent's answer (`dcc_core::domain::agent_reaction`), sent
//! to the desktop, which keeps them for the room and the bulletin.

use dcc_core::domain::agent_reaction::Reaction;
use serde_json::json;

use super::desktop_api::{ApiError, DesktopApi, encode};

impl DesktopApi {
    /// `None` takes the reaction back.
    pub fn set_agent_reaction(
        &self,
        agent_id: &str,
        session_id: &str,
        line_key: &str,
        reaction: Option<Reaction>,
    ) -> Result<(), ApiError> {
        self.post(
            &format!("/api/v1/agents/{}/reactions", encode(agent_id)),
            &json!({ "sessionId": session_id, "lineKey": line_key, "reaction": reaction }),
        )
        .map(|_| ())
    }
}
