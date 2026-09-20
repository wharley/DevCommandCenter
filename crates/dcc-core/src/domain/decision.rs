use serde::{Deserialize, Serialize};
use specta::Type;

/// Numeric decision evidence only; never stores prompts, tool output or secrets.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DecisionEvaluation {
    pub version: String,
    pub scores: Vec<DecisionScore>,
    pub context_truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DecisionScore {
    pub key: String,
    pub probability: f64,
}
