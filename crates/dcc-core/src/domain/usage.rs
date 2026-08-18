use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModelTokenUsage {
    #[serde(default)]
    pub model: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_write_input_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub total_tokens: u64,
    #[serde(default)]
    pub cost_usd: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UsageDashboardInput {
    /// Rolling period in days. `None` means all locally persisted DCC history.
    #[serde(default)]
    pub period_days: Option<u32>,
    /// Optional project filter. Missing means every project in this DCC database.
    #[serde(default)]
    pub project_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UsageTotals {
    pub sessions: u64,
    pub turns: u64,
    pub measured_turns: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_write_input_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub total_tokens: u64,
    #[serde(default)]
    pub cost_usd: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsageSummary {
    pub provider_id: String,
    pub sessions: u64,
    pub turns: u64,
    pub measured_turns: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_write_input_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub total_tokens: u64,
    #[serde(default)]
    pub cost_usd: Option<f64>,
    #[serde(default)]
    pub first_used_at: Option<String>,
    #[serde(default)]
    pub last_used_at: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModelUsageSummary {
    pub provider_id: String,
    pub model: String,
    pub measured_turns: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_write_input_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub total_tokens: u64,
    #[serde(default)]
    pub cost_usd: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DailyUsageSummary {
    /// UTC calendar date (`YYYY-MM-DD`).
    pub date: String,
    pub provider_id: String,
    pub turns: u64,
    pub measured_turns: u64,
    pub total_tokens: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UsageDashboard {
    pub generated_at: String,
    #[serde(default)]
    pub period_started_at: Option<String>,
    pub totals: UsageTotals,
    pub providers: Vec<ProviderUsageSummary>,
    pub models: Vec<ModelUsageSummary>,
    pub daily: Vec<DailyUsageSummary>,
}
