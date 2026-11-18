//! What wakes a resident agent by itself (`dcc_tauri::agent_triggers`, the
//! store the server owns too), as a global for the screens.

use std::ops::{Deref, DerefMut};
use std::path::PathBuf;

use gpui::{App, Global};

pub use dcc_tauri::agent_triggers::{
    CEILING_CHOICES, DAILY_RUNS, Held, QUIET_CHOICES, QuietHours, Run, TARGET_RUNS, Trigger,
    UNATTENDED_LIMIT,
};

#[derive(Default)]
pub struct AgentTriggers(dcc_tauri::agent_triggers::AgentTriggers);

impl Global for AgentTriggers {}

impl AgentTriggers {
    pub fn init(cx: &mut App) {
        cx.set_global(Self::load(crate::prefs::native_file("agent-triggers.json")));
    }

    pub fn load(path: PathBuf) -> Self {
        Self(dcc_tauri::agent_triggers::AgentTriggers::load(path))
    }
}

impl Deref for AgentTriggers {
    type Target = dcc_tauri::agent_triggers::AgentTriggers;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for AgentTriggers {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
