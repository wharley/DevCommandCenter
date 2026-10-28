//! The usage-limit scheduler: resumes a turn when its provider's limit
//! resets, for the sessions that asked.

use std::sync::Arc;

use dcc_core::ports::EventBus;
use dcc_tauri::state::SessionCommandState;
use dcc_tauri::usage_limit_resume::UsageLimitScheduler;

pub struct UsageLimitState {
    pub scheduler: Arc<UsageLimitScheduler>,
    _bus: Arc<dyn EventBus>,
}

pub fn start(sessions: &SessionCommandState) -> UsageLimitState {
    let scheduler = UsageLimitScheduler::new(sessions.clone());
    let bus = scheduler.event_bus();
    if let Err(error) = sessions.register_event_bus(&bus) {
        eprintln!("[DCC][usage-limit] session events unavailable: {error}");
    }
    crate::runtime::spawn(scheduler.clone().run());
    UsageLimitState {
        scheduler,
        _bus: bus,
    }
}
