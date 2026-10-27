//! "Manter o Mac acordado" (`keep-awake-api.ts`): the desktop holds the Mac
//! awake (its controller and menu bar); the native app reads and sets the
//! mode over `/api/v1/keep-awake`.

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::desktop_api::{ApiError, DesktopApi};

/// `KeepAwakeMode`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Off,
    Always,
    DuringAgents,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Off, Mode::Always, Mode::DuringAgents];

    /// `settings.general.keepAwake.modes.<key>`
    pub fn key(self) -> &'static str {
        match self {
            Mode::Off => "off",
            Mode::Always => "always",
            Mode::DuringAgents => "duringAgents",
        }
    }
}

/// `KeepAwakeReason`
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Reason {
    AppOpen,
    AgentsWorking { count: usize },
    AgentJustFinished,
    WaitingForLimit { until: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Hold {
    pub reason: Reason,
}

/// `KeepAwakeStatus`
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Status {
    /// False off macOS: the setting is hidden.
    pub supported: bool,
    pub mode: Mode,
    pub active: Option<Hold>,
}

/// `keepAwakeReasonKey`: the key, its count and the time (pt-BR `HH:MM`,
/// as the desktop writes it in both languages).
pub fn reason_key(reason: &Reason) -> (&'static str, Option<i64>, Option<String>) {
    match reason {
        Reason::WaitingForLimit { until } => (
            "settings.general.keepAwake.reasons.waitingForLimit",
            None,
            Some(
                chrono::DateTime::parse_from_rfc3339(until)
                    .map(|at| at.with_timezone(&chrono::Local).format("%H:%M").to_string())
                    .unwrap_or_default(),
            ),
        ),
        Reason::AgentsWorking { count } => (
            "settings.general.keepAwake.reasons.agentsWorking",
            Some(*count as i64),
            None,
        ),
        Reason::AgentJustFinished => (
            "settings.general.keepAwake.reasons.agentJustFinished",
            None,
            None,
        ),
        Reason::AppOpen => ("settings.general.keepAwake.reasons.appOpen", None, None),
    }
}

impl DesktopApi {
    /// `keep_awake_status`; `None` when it cannot be read (the card hides).
    pub fn keep_awake_status(&self) -> Option<Status> {
        self.get("/api/v1/keep-awake")
            .ok()
            .and_then(|value| serde_json::from_value(value).ok())
    }

    /// `keep_awake_set_mode`
    pub fn set_keep_awake(&self, mode: Mode) -> Result<Status, ApiError> {
        let value = self.put("/api/v1/keep-awake", &json!({ "mode": mode }))?;
        serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_and_reasons_read_like_the_desktop() {
        let status: Status = serde_json::from_value(json!({
            "supported": true, "mode": "during_agents",
            "active": { "reason": { "kind": "agentsWorking", "count": 2 }, "label": "x" },
        }))
        .unwrap();
        assert_eq!(status.mode, Mode::DuringAgents);
        let (key, count, _) = reason_key(&status.active.unwrap().reason);
        assert_eq!(key, "settings.general.keepAwake.reasons.agentsWorking");
        assert_eq!(count, Some(2));
        let (_, _, time) = reason_key(&Reason::WaitingForLimit {
            until: "2026-10-27T15:04:00Z".into(),
        });
        assert_eq!(time.unwrap().len(), 5);
        assert_eq!(Mode::DuringAgents.key(), "duringAgents");
    }
}
