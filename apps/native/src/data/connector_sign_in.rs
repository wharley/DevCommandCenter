//! DCC's own sign-in for a remote connector (`startMcpConnect`,
//! `waitMcpConnect`, `cancelMcpConnect`): the browser's address, then the
//! wait until the person authorized (or gave up).

use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};

use super::connectors::Check;
use super::desktop_api::{ApiError, DesktopApi};

/// A browser sign-in may take this long.
const SIGN_IN_WAIT: Duration = Duration::from_secs(15 * 60);

/// `StartMcpConnectOutput`
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Start {
    NoAuthRequired,
    Authorize {
        login_id: String,
        authorization_url: String,
    },
    Failed {
        reason: String,
        message: String,
    },
}

/// `WaitMcpConnectOutput`
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Finish {
    Connected { check: Option<Check> },
    Failed { reason: String, message: String },
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
}

impl DesktopApi {
    /// `startMcpConnect`
    pub fn start_connect(
        &self,
        id: &str,
        client_id: Option<&str>,
        client_secret: Option<&str>,
    ) -> Result<Start, ApiError> {
        decode(self.post(
            "/api/v1/mcp/connect/start",
            &json!({ "definitionId": id, "clientId": client_id, "clientSecret": client_secret }),
        )?)
    }

    /// `waitMcpConnect`: once the person authorized in the browser.
    pub fn wait_connect(&self, login_id: &str) -> Result<Finish, ApiError> {
        decode(self.request_waiting(
            "POST",
            "/api/v1/mcp/connect/wait",
            Some(&json!({ "loginId": login_id })),
            SIGN_IN_WAIT,
        )?)
    }

    /// `cancelMcpConnect`
    pub fn cancel_connect(&self, login_id: &str) -> Result<(), ApiError> {
        self.post(
            "/api/v1/mcp/connect/cancel",
            &json!({ "loginId": login_id }),
        )
        .map(|_| ())
    }
}
