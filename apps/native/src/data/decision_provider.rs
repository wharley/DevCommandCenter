//! Settings → Decision Provider (`decision-provider-api.ts`): TypeSafe Jev's
//! switch, mode, model routing, key, URL, model and the five decision
//! points with their thresholds, kept by the desktop (the key in the
//! keychain); and the decisions it recorded. Over `/api/v1/decision-provider`.

use dcc_core::domain::decision::DecisionEvaluation;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::desktop_api::{ApiError, DesktopApi};

/// `DecisionProviderSettingsOutput`
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub provider: String,
    pub mode: String,
    pub model_routing: String,
    pub memory_filter_enabled: bool,
    pub skill_router_enabled: bool,
    pub model_router_enabled: bool,
    pub completion_review_enabled: bool,
    #[serde(default = "yes")]
    pub tool_guard_enabled: bool,
    pub base_url: String,
    pub model: String,
    pub memory_threshold: f64,
    pub skill_confidence_threshold: f64,
    pub model_confidence_threshold: f64,
    pub completion_threshold: f64,
    pub tool_risk_threshold: f64,
    pub api_key_configured: bool,
}

fn yes() -> bool {
    true
}

/// `DecisionProviderSettingsInput`: the form as the person leaves it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub provider: String,
    pub mode: String,
    pub model_routing: String,
    pub memory_filter_enabled: bool,
    pub skill_router_enabled: bool,
    pub model_router_enabled: bool,
    pub completion_review_enabled: bool,
    pub tool_guard_enabled: bool,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub memory_threshold: Option<f64>,
    pub skill_confidence_threshold: Option<f64>,
    pub model_confidence_threshold: Option<f64>,
    pub completion_threshold: Option<f64>,
    pub tool_risk_threshold: Option<f64>,
    /// Only when the person typed a new one.
    pub api_key: Option<String>,
    pub clear_api_key: bool,
}

impl Default for Draft {
    /// The desktop's initial draft, before the settings arrive.
    fn default() -> Self {
        Self {
            provider: "disabled".into(),
            mode: "observe".into(),
            model_routing: "manual".into(),
            memory_filter_enabled: true,
            skill_router_enabled: true,
            model_router_enabled: true,
            completion_review_enabled: true,
            tool_guard_enabled: true,
            base_url: Some("https://api.typesafe.ai".into()),
            model: Some("jev-latest".into()),
            memory_threshold: Some(0.65),
            skill_confidence_threshold: Some(0.65),
            model_confidence_threshold: Some(0.80),
            completion_threshold: Some(0.80),
            tool_risk_threshold: Some(0.65),
            api_key: None,
            clear_api_key: false,
        }
    }
}

impl From<&Settings> for Draft {
    fn from(settings: &Settings) -> Self {
        Self {
            provider: settings.provider.clone(),
            mode: settings.mode.clone(),
            model_routing: settings.model_routing.clone(),
            memory_filter_enabled: settings.memory_filter_enabled,
            skill_router_enabled: settings.skill_router_enabled,
            model_router_enabled: settings.model_router_enabled,
            completion_review_enabled: settings.completion_review_enabled,
            tool_guard_enabled: settings.tool_guard_enabled,
            base_url: Some(settings.base_url.clone()),
            model: Some(settings.model.clone()),
            memory_threshold: Some(settings.memory_threshold),
            skill_confidence_threshold: Some(settings.skill_confidence_threshold),
            model_confidence_threshold: Some(settings.model_confidence_threshold),
            completion_threshold: Some(settings.completion_threshold),
            tool_risk_threshold: Some(settings.tool_risk_threshold),
            api_key: None,
            clear_api_key: false,
        }
    }
}

/// One of the five decision points, in the desktop's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Point {
    MemoryFilter,
    SkillRouter,
    ModelRouter,
    CompletionReview,
    ToolGuard,
}

impl Point {
    pub const ALL: [Point; 5] = [
        Point::MemoryFilter,
        Point::SkillRouter,
        Point::ModelRouter,
        Point::CompletionReview,
        Point::ToolGuard,
    ];

    /// `settings.decisionProvider.points.<key>`
    pub fn key(self) -> &'static str {
        match self {
            Point::MemoryFilter => "memory_filter",
            Point::SkillRouter => "skill_router",
            Point::ModelRouter => "model_router",
            Point::CompletionReview => "completion_review",
            Point::ToolGuard => "tool_guard",
        }
    }
}

impl Draft {
    pub fn enabled(&self, point: Point) -> bool {
        match point {
            Point::MemoryFilter => self.memory_filter_enabled,
            Point::SkillRouter => self.skill_router_enabled,
            Point::ModelRouter => self.model_router_enabled,
            Point::CompletionReview => self.completion_review_enabled,
            Point::ToolGuard => self.tool_guard_enabled,
        }
    }

    pub fn set_enabled(&mut self, point: Point, on: bool) {
        *match point {
            Point::MemoryFilter => &mut self.memory_filter_enabled,
            Point::SkillRouter => &mut self.skill_router_enabled,
            Point::ModelRouter => &mut self.model_router_enabled,
            Point::CompletionReview => &mut self.completion_review_enabled,
            Point::ToolGuard => &mut self.tool_guard_enabled,
        } = on;
    }

    fn threshold_slot(&mut self, point: Point) -> &mut Option<f64> {
        match point {
            Point::MemoryFilter => &mut self.memory_threshold,
            Point::SkillRouter => &mut self.skill_confidence_threshold,
            Point::ModelRouter => &mut self.model_confidence_threshold,
            Point::CompletionReview => &mut self.completion_threshold,
            Point::ToolGuard => &mut self.tool_risk_threshold,
        }
    }

    /// `point.threshold ?? 0.65`
    pub fn threshold(&self, point: Point) -> f64 {
        let mut copy = self.clone();
        copy.threshold_slot(point).unwrap_or(0.65)
    }

    /// `Number(event.target.value)`: what is not a number reads 0, like
    /// the desktop's field (the desktop then refuses out-of-range values).
    pub fn set_threshold(&mut self, point: Point, typed: &str) {
        *self.threshold_slot(point) = Some(typed.trim().parse().unwrap_or(0.0));
    }
}

/// `DecisionProviderHistoryOutput`
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub evaluation: Option<DecisionEvaluation>,
    pub id: i64,
    pub decision_point: String,
    pub mode: String,
    pub status: String,
    pub model: String,
    pub candidate_count: usize,
    pub selected_count: usize,
    pub selected_indices: Vec<usize>,
    pub selected_labels: Vec<String>,
    pub threshold: f64,
    pub duration_ms: u64,
    pub error: Option<String>,
    pub created_at: String,
}

impl HistoryEntry {
    /// The completion review's own badge: kept, needs review or complete.
    pub fn completion_key(&self) -> Option<(&'static str, bool)> {
        if self.status != "completed" || self.decision_point != "completion_review" {
            return None;
        }
        let needs_review = self
            .selected_labels
            .iter()
            .any(|label| label.starts_with("needs_review:"));
        Some(
            if self.selected_labels.iter().any(|label| label == "kept") {
                (
                    "settings.decisionProvider.completionKeptLabel",
                    needs_review,
                )
            } else if needs_review {
                ("settings.decisionProvider.completionNeedsReview", true)
            } else {
                ("settings.decisionProvider.completionComplete", false)
            },
        )
    }
}

/// `Intl.DateTimeFormat(pt-BR | en-US, { dateStyle: "medium", timeStyle:
/// "short" })`: "27 de out. de 2026, 15:04" / "Oct 27, 2026, 3:04 PM".
pub fn medium_date_time(value: &str, english: bool) -> String {
    use chrono::{Datelike, Timelike};
    let Ok(at) = chrono::DateTime::parse_from_rfc3339(value) else {
        return value.to_string();
    };
    let at = at.with_timezone(&chrono::Local);
    const PT: [&str; 12] = [
        "jan.", "fev.", "mar.", "abr.", "mai.", "jun.", "jul.", "ago.", "set.", "out.", "nov.",
        "dez.",
    ];
    const EN: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month = at.month0() as usize;
    if english {
        let (pm, hour) = at.hour12();
        format!(
            "{} {}, {}, {hour}:{:02} {}",
            EN[month],
            at.day(),
            at.year(),
            at.minute(),
            if pm { "PM" } else { "AM" }
        )
    } else {
        format!(
            "{} de {} de {}, {:02}:{:02}",
            at.day(),
            PT[month],
            at.year(),
            at.hour(),
            at.minute()
        )
    }
}

impl DesktopApi {
    /// `loadDecisionProviderSettings`
    pub fn decision_provider_settings(&self) -> Result<Settings, ApiError> {
        decode(self.get("/api/v1/decision-provider/settings")?)
    }

    /// `saveDecisionProviderSettings`
    pub fn save_decision_provider(&self, draft: &Draft) -> Result<Settings, ApiError> {
        let body = serde_json::to_value(draft).unwrap_or(Value::Null);
        decode(self.put("/api/v1/decision-provider/settings", &body)?)
    }

    /// `loadDecisionProviderHistory(100)`
    pub fn decision_provider_history(&self, limit: usize) -> Result<Vec<HistoryEntry>, ApiError> {
        decode(self.get(&format!("/api/v1/decision-provider/history?limit={limit}"))?)
    }
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_form_round_trips_like_the_desktop() {
        let settings: Settings = serde_json::from_value(json!({
            "provider": "typesafe", "mode": "enforce", "modelRouting": "automatic",
            "memoryFilterEnabled": true, "skillRouterEnabled": false, "modelRouterEnabled": true,
            "completionReviewEnabled": true, "baseUrl": "https://api.typesafe.ai",
            "model": "jev-latest", "memoryThreshold": 0.7, "skillConfidenceThreshold": 0.6,
            "modelConfidenceThreshold": 0.8, "completionThreshold": 0.85,
            "toolRiskThreshold": 0.5, "apiKeyConfigured": true, "restartRequired": false,
        }))
        .unwrap();
        assert!(settings.tool_guard_enabled);
        let mut draft = Draft::from(&settings);
        assert!(!draft.enabled(Point::SkillRouter));
        draft.set_enabled(Point::SkillRouter, true);
        draft.set_threshold(Point::ToolGuard, "0.4");
        assert_eq!(draft.threshold(Point::ToolGuard), 0.4);
        draft.set_threshold(Point::ToolGuard, "x");
        assert_eq!(draft.threshold(Point::ToolGuard), 0.0);
        let body = serde_json::to_value(&draft).unwrap();
        assert_eq!(body["skillRouterEnabled"], true);
        assert_eq!(body["apiKey"], Value::Null);
        assert_eq!(body["clearApiKey"], false);
        assert_eq!(Point::ALL.map(Point::key)[3], "completion_review");
    }

    #[test]
    fn history_badges_and_dates() {
        let entry = |labels: &[&str]| HistoryEntry {
            evaluation: None,
            id: 1,
            decision_point: "completion_review".into(),
            mode: "observe".into(),
            status: "completed".into(),
            model: "jev".into(),
            candidate_count: 1,
            selected_count: 0,
            selected_indices: vec![],
            selected_labels: labels.iter().map(|label| label.to_string()).collect(),
            threshold: 0.8,
            duration_ms: 10,
            error: None,
            created_at: "2026-10-27T12:00:00Z".into(),
        };
        assert_eq!(
            entry(&["needs_review:validation"]).completion_key(),
            Some(("settings.decisionProvider.completionNeedsReview", true))
        );
        assert_eq!(
            entry(&["kept", "needs_review:x"]).completion_key(),
            Some(("settings.decisionProvider.completionKeptLabel", true))
        );
        assert_eq!(
            entry(&[]).completion_key().unwrap().0,
            "settings.decisionProvider.completionComplete"
        );
        assert!(medium_date_time("2026-10-27T12:00:00Z", false).contains("de out. de 2026"));
        assert!(medium_date_time("2026-10-27T12:00:00Z", true).starts_with("Oct 27, 2026"));
        assert_eq!(medium_date_time("nope", true), "nope");
    }
}
