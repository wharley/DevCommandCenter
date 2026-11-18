//! The pull requests the person reviewed, searched by the server every 5
//! minutes (`docs/PLANO_MAQUINA_SEM_TELA`, slice 5): one `gh` search for the
//! GitHub account, whatever the number of projects or clients. It ran in the
//! native app's window; here it runs with or without one, and the app (and
//! what reacts to it in the server, next) reads the latest result instead of
//! searching again.
//!
//! Kuvo watches GitHub with a check that runs `gh` with no model, and
//! agentes.chat takes GitHub's webhook through its own GitHub App (ADR 0156).
//! A machine of one person has no public address for a webhook, so this
//! keeps the one search every 5 minutes (`background-cost-minimal`); a hosted
//! machine may take the webhook later.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::sync::RwLock;

use super::HttpBackend;

/// The first search, once the server settled.
const FIRST_SEARCH: Duration = Duration::from_secs(20);
/// Then every 5 minutes.
const EVERY: Duration = Duration::from_secs(5 * 60);

/// The latest search and when it came back.
#[derive(Default)]
pub(crate) struct PrWatch {
    latest: RwLock<Option<(Instant, Value)>>,
}

impl PrWatch {
    /// The latest result while it is fresh (younger than a search period).
    pub(super) async fn fresh(&self) -> Option<Value> {
        let latest = self.latest.read().await;
        latest
            .as_ref()
            .filter(|(at, _)| at.elapsed() < EVERY)
            .map(|(_, result)| result.clone())
    }

    async fn keep(&self, result: Value) {
        *self.latest.write().await = Some((Instant::now(), result));
    }
}

/// Searches once, keeps the result and lets what reacts to it react (each
/// reaction remembers what it already handled); the error otherwise.
pub(super) async fn search(backend: &Arc<HttpBackend>) -> Result<Value, String> {
    let output =
        dcc_tauri::commands::pr_hub_since::pull_request_hub_reviewed_with_state(&backend.workspace)
            .await?;
    let result = serde_json::to_value(&output).map_err(|error| error.to_string())?;
    if let Some(watch) = backend.pr_watch.get() {
        watch.keep(result.clone()).await;
    }
    // A task's pull request whose checks fail (`ci_failing`).
    super::ci_failing::react(backend, &output.failing).await;
    Ok(result)
}

/// Starts the search for the life of the process.
pub(crate) fn start(backend: Arc<HttpBackend>) {
    if backend.pr_watch.set(Arc::new(PrWatch::default())).is_err() {
        return;
    }
    crate::runtime::spawn(async move {
        tokio::time::sleep(FIRST_SEARCH).await;
        loop {
            if let Err(error) = search(&backend).await {
                eprintln!("[DCC][pr-watch] {error}");
            }
            tokio::time::sleep(EVERY).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_result_is_served_while_it_is_fresh() {
        let watch = PrWatch::default();
        assert!(watch.fresh().await.is_none());
        watch.keep(serde_json::json!({"items": []})).await;
        assert_eq!(watch.fresh().await, Some(serde_json::json!({"items": []})));
        // One older than a period is searched again.
        *watch.latest.write().await = Some((
            Instant::now() - EVERY - Duration::from_secs(1),
            serde_json::json!({}),
        ));
        assert!(watch.fresh().await.is_none());
    }
}
