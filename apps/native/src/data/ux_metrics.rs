//! "Sinais locais de UX" (`ux-metrics.ts`): how many times something was
//! first found and used, and how long into the run the first time came —
//! aggregate counts only, never prompts, paths or ids, kept on this device.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Instant;

use serde::{Deserialize, Serialize};

/// The ones Settings › Experimental shows, in its order.
pub const SHOWN: [&str; 6] = [
    "first_prompt",
    "diff_discovered",
    "terminal_discovered",
    "terminal_scope_switched",
    "advanced_composer_control_used",
    "command_palette_action",
];

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metric {
    pub count: u64,
    pub first_elapsed_ms: u64,
    pub last_elapsed_ms: u64,
}

/// `sessionStartedAt`: when this run began.
fn started() -> &'static Instant {
    static STARTED: OnceLock<Instant> = OnceLock::new();
    STARTED.get_or_init(Instant::now)
}

/// Marks the run's start (the app's boot).
pub fn start() {
    started();
}

fn path() -> PathBuf {
    crate::prefs::native_file("ux-metrics.json")
}

/// `readUxMetrics`
pub fn read() -> HashMap<String, Metric> {
    std::fs::read(path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// `recordUxMetric`: one more, keeping when it was first seen.
pub fn record(name: &str) {
    if cfg!(test) {
        return;
    }
    let elapsed = started().elapsed().as_millis() as u64;
    let mut metrics = read();
    let entry = metrics.entry(name.to_string()).or_insert(Metric {
        count: 0,
        first_elapsed_ms: elapsed,
        last_elapsed_ms: elapsed,
    });
    entry.count += 1;
    entry.last_elapsed_ms = elapsed;
    if let Ok(json) = serde_json::to_vec(&metrics) {
        let _ = std::fs::write(path(), json);
    }
}

/// "Limpar métricas"
pub fn reset() {
    let _ = std::fs::remove_file(path());
}
