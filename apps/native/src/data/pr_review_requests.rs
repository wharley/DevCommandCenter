//! Pull requests an agent asked DCC to review (`dcc_review_pull_request`,
//! `dcc_core::domain::pr_review_request`): the sidebar's read brings the
//! waiting ones, and the app tells the desktop it took each one before it
//! runs the hub's review.

use serde_json::{Value, json};

use super::desktop_api::{ApiError, DesktopApi, encode};

impl DesktopApi {
    /// Marks the request taken; false when it already was.
    pub fn take_review_request(&self, agent_id: &str, id: &str) -> Result<bool, ApiError> {
        let value = self.post(
            &format!(
                "/api/v1/agents/{}/review-requests/{}/taken",
                encode(agent_id),
                encode(id)
            ),
            &json!({}),
        )?;
        Ok(value.get("taken").and_then(Value::as_bool).unwrap_or(false))
    }
}
