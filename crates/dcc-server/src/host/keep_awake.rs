//! The keep-awake controller wired to the sessions and the usage-limit
//! scheduler, and its hook for Settings (`http_api::keep_awake`).

use std::path::Path;
use std::sync::Arc;

use dcc_core::ports::EventBus;
use dcc_tauri::state::SessionCommandState;
use dcc_tauri::usage_limit_resume::UsageLimitScheduler;

use crate::http_api::DesktopKeepAwake;
use crate::keep_awake::{self, HoldReason, KeepAwake, KeepAwakeMode, KeepAwakeStatus, TurnSource};

/// Tells every window the hold changed; they read `keep_awake_status`.
pub const CHANGED_EVENT: &str = "keep-awake-changed";

/// Called with the hold's reason when it starts, changes or ends.
pub type HoldListener = Box<dyn Fn(Option<&HoldReason>) + Send + Sync>;

pub struct KeepAwakeState {
    pub controller: Arc<KeepAwake>,
    // The runtime keeps only a weak reference to its buses.
    _bus: Arc<dyn EventBus>,
}

/// The turns in flight (provider bindings) and the automatic resumes the
/// usage-limit scheduler waits for.
struct AgentWork {
    sessions: SessionCommandState,
    limits: Arc<UsageLimitScheduler>,
}

#[async_trait::async_trait]
impl TurnSource for AgentWork {
    async fn active_turns(&self) -> Result<Vec<(String, String)>, String> {
        TurnSource::active_turns(&self.sessions).await
    }

    async fn limit_waits(&self) -> Vec<chrono::DateTime<chrono::Utc>> {
        self.limits
            .waits()
            .into_iter()
            .map(|(_, due)| due)
            .collect()
    }
}

pub fn start(
    sessions: &SessionCommandState,
    limits: Arc<UsageLimitScheduler>,
    app_data_dir: &Path,
    listener: HoldListener,
) -> KeepAwakeState {
    let controller = KeepAwake::new(
        keep_awake::system_power(),
        Some(app_data_dir.to_path_buf()),
        keep_awake::load_mode(app_data_dir),
        keep_awake::HAND_OFF_GRACE,
    );
    controller.set_listener(listener);
    let bus = controller.event_bus();
    if let Err(error) = sessions.register_event_bus(&bus) {
        eprintln!("[DCC][keep-awake] session events unavailable: {error}");
    }
    let source: Arc<dyn TurnSource> = Arc::new(AgentWork {
        sessions: sessions.clone(),
        limits,
    });
    crate::runtime::spawn(controller.clone().run(source));
    KeepAwakeState {
        controller,
        _bus: bus,
    }
}

pub struct HostKeepAwake(pub Arc<KeepAwake>);

impl DesktopKeepAwake for HostKeepAwake {
    fn status(&self) -> Option<KeepAwakeStatus> {
        Some(self.0.status())
    }

    fn set_mode(&self, mode: KeepAwakeMode) -> Option<Result<KeepAwakeStatus, String>> {
        Some(self.0.set_mode(mode))
    }
}
