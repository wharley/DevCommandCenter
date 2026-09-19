//! Decision providers used by the DCC orchestration layer.
//!
//! A decision provider may classify a bounded piece of workflow state, but it
//! never owns execution, permissions, or the final policy decision. The first
//! integration is TypeSafe Jev for judging retrieved ai-memory candidates.

use std::{collections::BTreeMap, env, time::Duration};

use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::ai_memory::AiMemoryHit;

const DEFAULT_TYPESAFE_BASE_URL: &str = "https://api.typesafe.ai";
const DEFAULT_MODEL: &str = "jev-latest";
const DEFAULT_TIMEOUT: Duration = Duration::from_millis(500);
const DEFAULT_MEMORY_RELEVANCE_THRESHOLD: f64 = 0.65;
const MAX_STATE_CHARS: usize = 12_000;
const MAX_MEMORY_SNIPPET_CHARS: usize = 1_500;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DecisionMode {
    #[default]
    Observe,
    Enforce,
}

impl DecisionMode {
    pub fn from_env() -> Self {
        match env::var("DCC_DECISION_MODE")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "enforce" => Self::Enforce,
            _ => Self::Observe,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecisionProviderConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub timeout: Duration,
    pub memory_relevance_threshold: f64,
    pub mode: DecisionMode,
}

impl DecisionProviderConfig {
    /// Reads the opt-in TypeSafe configuration without ever logging the key.
    pub fn from_env() -> Option<Self> {
        let provider = env::var("DCC_DECISION_PROVIDER").ok()?;
        if !provider.trim().eq_ignore_ascii_case("typesafe") {
            return None;
        }

        let api_key = env::var("TYPESAFE_API_KEY").ok()?;
        if api_key.trim().is_empty() {
            return None;
        }

        let base_url = env::var("TYPESAFE_BASE_URL")
            .unwrap_or_else(|_| DEFAULT_TYPESAFE_BASE_URL.to_string())
            .trim_end_matches('/')
            .to_string();
        let model = env::var("TYPESAFE_MODEL")
            .unwrap_or_else(|_| DEFAULT_MODEL.to_string())
            .trim()
            .to_string();
        let timeout = env::var("DCC_DECISION_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT_TIMEOUT);
        let threshold = env::var("DCC_DECISION_MEMORY_THRESHOLD")
            .ok()
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| (0.0..=1.0).contains(value))
            .unwrap_or(DEFAULT_MEMORY_RELEVANCE_THRESHOLD);

        Some(Self {
            base_url,
            api_key,
            model,
            timeout,
            memory_relevance_threshold: threshold,
            mode: DecisionMode::from_env(),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MemoryFilterInput<'a> {
    pub prompt: &'a str,
    pub hits: &'a [AiMemoryHit],
}

#[derive(Clone, Debug, PartialEq)]
pub struct MemoryFilterDecision {
    pub index: usize,
    pub relevance: f64,
    pub confidence: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MemoryFilterResult {
    pub decisions: Vec<MemoryFilterDecision>,
    pub model: String,
}

#[derive(Debug, thiserror::Error)]
pub enum DecisionProviderError {
    #[error("decision provider request failed: {0}")]
    Request(String),
    #[error("decision provider returned HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("decision provider returned an invalid response: {0}")]
    Response(String),
    #[error("decision provider does not support {0}")]
    Unsupported(&'static str),
}

#[async_trait]
pub trait DecisionProvider: Send + Sync {
    async fn filter_memory(
        &self,
        input: MemoryFilterInput<'_>,
    ) -> Result<MemoryFilterResult, DecisionProviderError>;
}

#[derive(Clone)]
pub struct TypeSafeDecisionProvider {
    config: DecisionProviderConfig,
    http: reqwest::Client,
}

impl TypeSafeDecisionProvider {
    pub fn from_env() -> Option<Self> {
        Self::new(DecisionProviderConfig::from_env()?).ok()
    }

    pub fn new(config: DecisionProviderConfig) -> Result<Self, DecisionProviderError> {
        let http = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|error| DecisionProviderError::Request(error.to_string()))?;
        Ok(Self { config, http })
    }

    pub fn config(&self) -> &DecisionProviderConfig {
        &self.config
    }

    pub fn filter_hits(
        &self,
        hits: Vec<AiMemoryHit>,
        result: &MemoryFilterResult,
    ) -> Vec<AiMemoryHit> {
        let selected: Vec<AiMemoryHit> = result
            .decisions
            .iter()
            .filter(|decision| decision.relevance >= self.config.memory_relevance_threshold)
            .filter_map(|decision| hits.get(decision.index).cloned())
            .collect();

        // A failed or over-aggressive classifier must not erase all historical
        // evidence. The caller can still use the original retrieval result.
        if selected.is_empty() {
            hits
        } else {
            selected
        }
    }

    async fn request(
        &self,
        state: String,
        questions: BTreeMap<String, Value>,
    ) -> Result<SystemOneResponse, DecisionProviderError> {
        let endpoint = format!("{}/v1/systemone", self.config.base_url);
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        let auth = HeaderValue::from_str(&format!("Bearer {}", self.config.api_key))
            .map_err(|error| DecisionProviderError::Request(error.to_string()))?;
        headers.insert(AUTHORIZATION, auth);

        let response = self
            .http
            .post(endpoint)
            .headers(headers)
            .json(&json!({
                "model": self.config.model,
                "state": state,
                "questions": questions,
            }))
            .send()
            .await
            .map_err(|error| DecisionProviderError::Request(error.to_string()))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| DecisionProviderError::Response(error.to_string()))?;
        if !status.is_success() {
            return Err(DecisionProviderError::Http {
                status: status.as_u16(),
                body: truncate(&body, 500),
            });
        }
        serde_json::from_str(&body)
            .map_err(|error| DecisionProviderError::Response(error.to_string()))
    }
}

#[async_trait]
impl DecisionProvider for TypeSafeDecisionProvider {
    async fn filter_memory(
        &self,
        input: MemoryFilterInput<'_>,
    ) -> Result<MemoryFilterResult, DecisionProviderError> {
        if input.hits.is_empty() {
            return Ok(MemoryFilterResult {
                decisions: Vec::new(),
                model: self.config.model.clone(),
            });
        }

        let mut state = format!(
            "Current task:\n{}\n\nRetrieved historical candidates:\n",
            input.prompt
        );
        let mut questions = BTreeMap::new();
        for (index, hit) in input.hits.iter().enumerate() {
            let title = hit
                .title
                .as_deref()
                .or(hit.path.as_deref())
                .unwrap_or("memory");
            let snippet = hit.snippet.as_deref().unwrap_or("");
            state.push_str(&format!(
                "\n[{}] {}\n{}\n",
                index,
                title,
                truncate(snippet, MAX_MEMORY_SNIPPET_CHARS)
            ));
            questions.insert(
                format!("memory_{index}"),
                json!({
                    "type": "noul",
                    "instructions": format!(
                        "Is historical memory candidate {index} directly relevant to the current task, rather than merely sharing a keyword?"
                    ),
                }),
            );
        }

        let response = self
            .request(truncate(&state, MAX_STATE_CHARS), questions)
            .await?;
        let decisions = input
            .hits
            .iter()
            .enumerate()
            .map(|(index, _)| {
                let answer = response.answers.get(&format!("memory_{index}"));
                MemoryFilterDecision {
                    index,
                    relevance: answer
                        .and_then(|answer| answer.noul)
                        .unwrap_or(0.0)
                        .clamp(0.0, 1.0),
                    confidence: answer.and_then(|answer| answer.confidence),
                }
            })
            .collect();

        Ok(MemoryFilterResult {
            decisions,
            model: response.model.unwrap_or_else(|| self.config.model.clone()),
        })
    }
}

#[derive(Debug, Deserialize)]
struct SystemOneResponse {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    answers: BTreeMap<String, SystemOneAnswer>,
}

#[derive(Debug, Deserialize)]
struct SystemOneAnswer {
    #[serde(default)]
    noul: Option<f64>,
    #[serde(default)]
    confidence: Option<f64>,
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut output: String = value.chars().take(max_chars).collect();
    if value.chars().count() > max_chars {
        output.push('…');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(title: &str, snippet: &str) -> AiMemoryHit {
        AiMemoryHit {
            path: None,
            title: Some(title.to_string()),
            snippet: Some(snippet.to_string()),
            rank: None,
            created_at: None,
            session_id: None,
            kind: Some("notification".to_string()),
        }
    }

    #[test]
    fn defaults_to_observe_mode() {
        assert_eq!(DecisionMode::default(), DecisionMode::Observe);
    }

    #[test]
    fn keeps_original_hits_when_filter_would_remove_everything() {
        let provider = TypeSafeDecisionProvider::new(DecisionProviderConfig {
            base_url: DEFAULT_TYPESAFE_BASE_URL.to_string(),
            api_key: "test-key".to_string(),
            model: DEFAULT_MODEL.to_string(),
            timeout: DEFAULT_TIMEOUT,
            memory_relevance_threshold: 0.65,
            mode: DecisionMode::Enforce,
        })
        .expect("provider");
        let hits = vec![hit("one", "first"), hit("two", "second")];
        let result = MemoryFilterResult {
            decisions: vec![
                MemoryFilterDecision {
                    index: 0,
                    relevance: 0.2,
                    confidence: None,
                },
                MemoryFilterDecision {
                    index: 1,
                    relevance: 0.4,
                    confidence: None,
                },
            ],
            model: DEFAULT_MODEL.to_string(),
        };
        assert_eq!(provider.filter_hits(hits.clone(), &result), hits);
    }

    #[test]
    fn keeps_only_candidates_above_the_configured_threshold() {
        let provider = TypeSafeDecisionProvider::new(DecisionProviderConfig {
            base_url: DEFAULT_TYPESAFE_BASE_URL.to_string(),
            api_key: "test-key".to_string(),
            model: DEFAULT_MODEL.to_string(),
            timeout: DEFAULT_TIMEOUT,
            memory_relevance_threshold: 0.65,
            mode: DecisionMode::Enforce,
        })
        .expect("provider");
        let hits = vec![hit("one", "first"), hit("two", "second")];
        let result = MemoryFilterResult {
            decisions: vec![
                MemoryFilterDecision {
                    index: 0,
                    relevance: 0.9,
                    confidence: Some(0.9),
                },
                MemoryFilterDecision {
                    index: 1,
                    relevance: 0.2,
                    confidence: Some(0.8),
                },
            ],
            model: DEFAULT_MODEL.to_string(),
        };
        assert_eq!(provider.filter_hits(hits, &result).len(), 1);
    }

    #[test]
    fn parses_noul_answers() {
        let response: SystemOneResponse = serde_json::from_value(json!({
            "model": "jev-latest",
            "answers": {
                "memory_0": {"type": "noul", "noul": 0.91},
                "memory_1": {"type": "noul", "noul": 0.12, "confidence": 0.8}
            }
        }))
        .expect("response");
        assert_eq!(response.answers["memory_0"].noul, Some(0.91));
        assert_eq!(response.answers["memory_1"].confidence, Some(0.8));
    }
}
