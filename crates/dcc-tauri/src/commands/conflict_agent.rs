//! The merge resolver's agent, as one read-only run: it reads the checkout
//! and the three versions of the conflicted file in the prompt and answers
//! a suggested resolution. No task and no conversation are made for it —
//! the run the pull request hub's Reviewer uses — and the person can stop
//! it while it works.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use dcc_core::ports::ProviderRuntimeConfig;
use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::sync::oneshot;

use crate::state::SessionCommandState;

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConflictAgentInput {
    /// The task's checkout: where the agent reads.
    pub workspace_root: String,
    pub provider_id: String,
    pub model: Option<String>,
    pub provider_runtime: Option<ProviderRuntimeConfig>,
    #[serde(default)]
    pub effort: Option<String>,
    pub prompt: String,
    /// The caller's name for this run, to stop it by.
    pub run_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConflictAgentOutput {
    pub response: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConflictAgentCancelInput {
    pub run_id: String,
}

fn runs() -> &'static Mutex<HashMap<String, oneshot::Sender<()>>> {
    static RUNS: OnceLock<Mutex<HashMap<String, oneshot::Sender<()>>>> = OnceLock::new();
    RUNS.get_or_init(Default::default)
}

fn lock() -> std::sync::MutexGuard<'static, HashMap<String, oneshot::Sender<()>>> {
    runs()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Largest prompt taken: three versions of a file and its result.
const MAX_PROMPT: usize = 400_000;

/// Runs the suggestion; the answer is the agent's final text.
pub async fn conflict_agent_with_state(
    state: &SessionCommandState,
    input: ConflictAgentInput,
) -> Result<ConflictAgentOutput, String> {
    let root = input.workspace_root.trim();
    let path = std::path::Path::new(root);
    if root.is_empty() || !path.is_absolute() || !path.is_dir() {
        return Err("The task's checkout is not a folder.".to_string());
    }
    let provider_id = input.provider_id.trim();
    if provider_id.is_empty() {
        return Err("Select a provider for the suggestion.".to_string());
    }
    if input.prompt.trim().is_empty() || input.prompt.len() > MAX_PROMPT {
        return Err("The conflict is empty or too large to send to the agent.".to_string());
    }
    let run_id = input.run_id.trim().to_string();
    if run_id.is_empty() {
        return Err("run_id is empty".to_string());
    }
    let (stop, stopped) = oneshot::channel();
    lock().insert(run_id.clone(), stop);
    let result = state
        .run_ephemeral_read_only_turn_until(
            root.to_string(),
            provider_id.to_string(),
            input.model,
            input.provider_runtime,
            input.effort,
            input.prompt,
            Some(stopped),
        )
        .await;
    lock().remove(&run_id);
    result
        .map(|response| ConflictAgentOutput { response })
        .map_err(|error| error.to_string())
}

/// Stops a run by its name; `false` when it already ended.
pub fn cancel_conflict_agent(input: ConflictAgentCancelInput) -> bool {
    match lock().remove(input.run_id.trim()) {
        Some(stop) => stop.send(()).is_ok(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_is_stopped_by_its_name_once() {
        let (stop, mut stopped) = oneshot::channel();
        lock().insert("run-a".into(), stop);
        assert!(cancel_conflict_agent(ConflictAgentCancelInput {
            run_id: " run-a ".into()
        }));
        assert!(stopped.try_recv().is_ok(), "the run heard it");
        // Already ended (or never there): nothing to stop.
        assert!(!cancel_conflict_agent(ConflictAgentCancelInput {
            run_id: "run-a".into()
        }));
    }
}
