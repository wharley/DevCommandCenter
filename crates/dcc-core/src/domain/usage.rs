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

/// Exact provider-reported usage of one turn, for the per-turn footer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionTurnUsage {
    pub turn_id: String,
    pub models: Vec<ModelTokenUsage>,
}
