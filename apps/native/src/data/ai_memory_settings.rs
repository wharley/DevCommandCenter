//! Settings → AI Memory (`ai-memory-api.ts`): the sidecar's status, its
//! settings (the token in the keychain; mode and folder take effect after
//! restarting DCC), the sessions waiting to be exported and what was
//! exported. Over `/api/v1/ai-memory/*` and `/api/v1/app/restart`.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::desktop_api::{ApiError, DesktopApi};

/// `AiMemorySidecarStatus`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SidecarStatus {
    pub mode: String,
    pub running: bool,
    pub url: Option<String>,
    pub data_dir: Option<String>,
    pub version: Option<String>,
    pub log_path: Option<String>,
    pub message: Option<String>,
}

impl SidecarStatus {
    /// The green check (`managed`, `remote`, `shared`), else the amber alert.
    pub fn healthy(&self) -> bool {
        matches!(self.mode.as_str(), "managed" | "remote" | "shared")
    }
}

/// `AiMemorySettingsOutput`
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub mode: String,
    pub base_url: Option<String>,
    pub workspace: String,
    pub project: String,
    pub data_dir: Option<String>,
    pub token_configured: bool,
    pub restart_required: bool,
}

/// `AiMemorySettingsInput`
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub mode: String,
    pub base_url: Option<String>,
    pub workspace: String,
    pub project: String,
    pub data_dir: Option<String>,
    /// Only when the person typed a new one.
    pub token: Option<String>,
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            mode: "managed".into(),
            base_url: None,
            workspace: "dcc-workspace".into(),
            project: "dcc-project".into(),
            data_dir: None,
            token: None,
        }
    }
}

impl From<&Settings> for Draft {
    fn from(settings: &Settings) -> Self {
        Self {
            mode: settings.mode.clone(),
            base_url: settings.base_url.clone(),
            workspace: settings.workspace.clone(),
            project: settings.project.clone(),
            data_dir: settings.data_dir.clone(),
            token: None,
        }
    }
}

/// The modes the person picks, in the desktop's order.
pub const MODES: [&str; 3] = ["managed", "remote", "disabled"];

/// `AiMemoryOutboxStatusOutput`
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboxEntry {
    pub session_id: String,
    pub attempts: u32,
    pub next_attempt_at: String,
    pub last_error: Option<String>,
    pub event_count: usize,
}

/// `AiMemoryExportHistoryOutput`
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportEntry {
    pub id: i64,
    pub session_id: String,
    pub status: String,
    pub attempts: u32,
    pub event_count: usize,
    pub accepted_count: usize,
    pub error: Option<String>,
    pub finished_at: String,
}

/// `abcd1234…wxyz` for ids longer than 12.
pub fn short_session(id: &str) -> String {
    let chars: Vec<char> = id.chars().collect();
    if chars.len() > 12 {
        let head: String = chars[..8].iter().collect();
        let tail: String = chars[chars.len() - 4..].iter().collect();
        format!("{head}…{tail}")
    } else {
        id.to_string()
    }
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
}

impl DesktopApi {
    /// `loadAiMemorySidecarStatus`
    pub fn ai_memory_sidecar(&self) -> Result<SidecarStatus, ApiError> {
        decode(self.get("/api/v1/ai-memory/sidecar")?)
    }

    /// `loadAiMemorySettings`
    pub fn ai_memory_settings(&self) -> Result<Settings, ApiError> {
        decode(self.get("/api/v1/ai-memory/settings")?)
    }

    /// `saveAiMemorySettings`
    pub fn save_ai_memory(&self, draft: &Draft) -> Result<Settings, ApiError> {
        let body = serde_json::to_value(draft).unwrap_or(Value::Null);
        decode(self.put("/api/v1/ai-memory/settings", &body)?)
    }

    /// `loadAiMemoryOutbox(50)`
    pub fn ai_memory_outbox(&self) -> Result<Vec<OutboxEntry>, ApiError> {
        decode(self.get("/api/v1/ai-memory/outbox?limit=50")?)
    }

    /// `retryAiMemoryOutbox`
    pub fn retry_ai_memory(&self, session_id: &str) -> Result<(), ApiError> {
        self.post(
            &format!(
                "/api/v1/ai-memory/outbox/{}/retry",
                super::desktop_api::encode(session_id)
            ),
            &json!({}),
        )
        .map(|_| ())
    }

    /// `loadAiMemoryExportHistory(100)`
    pub fn ai_memory_exports(&self) -> Result<Vec<ExportEntry>, ApiError> {
        decode(self.get("/api/v1/ai-memory/export-history?limit=100")?)
    }

    /// `restartDcc`
    pub fn restart_desktop(&self) -> Result<(), ApiError> {
        self.post("/api/v1/app/restart", &json!({})).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_settings_and_ids_read_like_the_desktop() {
        let status: SidecarStatus =
            serde_json::from_value(json!({ "mode": "shared", "running": true, "url": "http://x" }))
                .unwrap();
        assert!(status.healthy());
        assert!(!SidecarStatus::default().healthy());
        let settings: Settings = serde_json::from_value(json!({
            "mode": "remote", "baseUrl": "http://127.0.0.1:49374", "workspace": "w",
            "project": "p", "dataDir": null, "tokenConfigured": true, "restartRequired": false,
        }))
        .unwrap();
        let draft = Draft::from(&settings);
        let body = serde_json::to_value(&draft).unwrap();
        assert_eq!(body["baseUrl"], "http://127.0.0.1:49374");
        assert_eq!(body["token"], Value::Null);
        assert_eq!(short_session("0123456789abcdef"), "01234567…cdef");
        assert_eq!(short_session("short"), "short");
    }
}
