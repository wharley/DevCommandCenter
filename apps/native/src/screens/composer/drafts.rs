//! Unsent prompts, one per conversation, in `composer-drafts.json` (the
//! desktop keeps its own in the webview; the two apps don't share drafts).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use gpui::App;

use crate::prefs;

const FILE: &str = "composer-drafts.json";
/// Writes wait for typing to pause this long.
const SAVE_DELAY: Duration = Duration::from_millis(400);

static GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn load() -> HashMap<String, String> {
    if cfg!(test) {
        return HashMap::new();
    }
    std::fs::read_to_string(prefs::native_file(FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn write(drafts: &HashMap<String, String>) {
    if cfg!(test) {
        return;
    }
    let path = prefs::native_file(FILE);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_string(drafts) {
        Ok(text) => {
            if let Err(error) = std::fs::write(&path, text) {
                eprintln!("[dcc-native] could not save the drafts: {error}");
            }
        }
        Err(error) => eprintln!("[dcc-native] could not save the drafts: {error}"),
    }
}

/// Saves once typing pauses; a newer save replaces a pending one.
pub fn save_later(drafts: HashMap<String, String>, cx: &mut App) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let executor = cx.background_executor().clone();
    executor
        .clone()
        .spawn(async move {
            executor.timer(SAVE_DELAY).await;
            if GENERATION.load(Ordering::SeqCst) == generation {
                write(&drafts);
            }
        })
        .detach();
}
