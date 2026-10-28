//! The quick composer's durable, compare-and-swap launch journal
//! (`quick-composer/state.json`) and its saved global key. Ambiguous launches
//! stay reviewable and are never replayed automatically after a restart.

use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, sync::Mutex};

pub const DEFAULT_SHORTCUT: &str = "Super+Shift+KeyC";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Launch {
    pub id: String,
    pub revision: u32,
    pub phase: String,
    pub request: serde_json::Value,
    pub workspace_id: Option<String>,
    pub session_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Saved {
    pub shortcut: Option<String>,
    pub launch: Option<Launch>,
}
impl Default for Saved {
    fn default() -> Self {
        Self {
            shortcut: Some(DEFAULT_SHORTCUT.into()),
            launch: None,
        }
    }
}
pub struct QuickComposerState {
    root: PathBuf,
    pub saved: Mutex<Saved>,
    /// The saved key could not be registered.
    pub shortcut_error: Mutex<bool>,
}
impl QuickComposerState {
    pub fn persist(&self, saved: &Saved) -> Result<(), String> {
        let bytes = serde_json::to_vec(saved).map_err(|e| e.to_string())?;
        let temporary = self.root.join("state.tmp");
        fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
        // Sync before publication: no creation/send can precede this checkpoint.
        fs::File::open(&temporary)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        fs::rename(temporary, self.root.join("state.json")).map_err(|e| e.to_string())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    supported: bool,
    shortcut: Option<String>,
    shortcut_error: bool,
    launch: Option<Launch>,
}
/// `quick_composer_status` without the window check (the native app's
/// Settings, over the loopback API).
pub fn status(state: &QuickComposerState) -> Result<Status, String> {
    let saved = state.saved.lock().map_err(|e| e.to_string())?;
    Ok(Status {
        supported: cfg!(target_os = "macos"),
        shortcut: saved.shortcut.clone(),
        shortcut_error: *state.shortcut_error.lock().map_err(|e| e.to_string())?,
        launch: saved.launch.clone(),
    })
}

fn validate_launch(launch: &Launch) -> Result<(), String> {
    if uuid::Uuid::parse_str(&launch.id).is_err()
        || launch.revision != 0
        || launch.phase != "pending"
        || launch.workspace_id.is_some()
        || launch.session_id.is_some()
        || launch.error.is_some()
    {
        return Err("Invalid launch".into());
    }
    let prompt = launch
        .request
        .get("turn")
        .and_then(|v| v.get("rawPrompt"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if prompt.trim().is_empty() || prompt.len() > 256_000 {
        return Err("Invalid prompt".into());
    }
    if !matches!(
        launch.request.get("isolationMode").and_then(|v| v.as_str()),
        Some("localDirect" | "protectedWorktree")
    ) {
        return Err("Invalid environment".into());
    }
    if serde_json::to_vec(&launch.request)
        .map_err(|e| e.to_string())?
        .len()
        > 300_000
    {
        return Err("Launch too large".into());
    }
    Ok(())
}

pub fn begin(state: &QuickComposerState, launch: Launch) -> Result<Launch, String> {
    validate_launch(&launch)?;
    let mut saved = state.saved.lock().map_err(|e| e.to_string())?;
    if let Some(previous) = &saved.launch {
        if previous.id == launch.id {
            return Ok(previous.clone());
        }
        if !matches!(previous.phase.as_str(), "completed" | "failed") {
            return Err("A launch is already pending".into());
        }
        // Keep the original prompt and partial task links available for recovery.
        fs::write(
            state.root.join(format!("{}.json", previous.id)),
            serde_json::to_vec(previous).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    let mut next = saved.clone();
    next.launch = Some(launch.clone());
    state.persist(&next)?;
    *saved = next;
    Ok(launch)
}

fn transition(current: &Launch, next: &Launch) -> Result<(), String> {
    if current.id != next.id
        || next.revision != current.revision + 1
        || current.request != next.request
    {
        return Err("Stale launch".into());
    }
    let allowed = matches!(
        (current.phase.as_str(), next.phase.as_str()),
        ("pending", "creating")
            | ("creating", "workspaceReady")
            | ("workspaceReady", "starting")
            | ("starting", "sessionReady")
            | ("sessionReady", "sending")
            | ("sending", "completed")
    );
    if (!allowed && next.phase != "failed")
        || matches!(current.phase.as_str(), "completed" | "failed")
    {
        return Err("Invalid launch transition".into());
    }
    if current.workspace_id.is_some() && current.workspace_id != next.workspace_id
        || current.session_id.is_some() && current.session_id != next.session_id
    {
        return Err("Launch scope changed".into());
    }
    if matches!(
        next.phase.as_str(),
        "workspaceReady" | "starting" | "sessionReady" | "sending" | "completed"
    ) && next.workspace_id.is_none()
        || matches!(
            next.phase.as_str(),
            "sessionReady" | "sending" | "completed"
        ) && next.session_id.is_none()
    {
        return Err("Missing launch scope".into());
    }
    Ok(())
}
pub fn checkpoint(state: &QuickComposerState, launch: Launch) -> Result<Launch, String> {
    let mut saved = state.saved.lock().map_err(|e| e.to_string())?;
    let current = saved.launch.as_ref().ok_or("Missing launch")?;
    transition(current, &launch)?;
    let mut next = saved.clone();
    next.launch = Some(launch.clone());
    state.persist(&next)?;
    *saved = next;
    Ok(launch)
}

impl QuickComposerState {
    /// `quick-composer/state.json` in the data directory (private to the user).
    pub fn open(app_data: &std::path::Path) -> Result<Self, String> {
        let root = app_data.join("quick-composer");
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        let saved = match fs::read(root.join("state.json")) {
            Ok(bytes) => serde_json::from_slice::<Saved>(&bytes).map_err(|e| e.to_string())?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Saved::default(),
            Err(error) => return Err(error.to_string()),
        };
        Ok(Self {
            root,
            saved: Mutex::new(saved),
            shortcut_error: Mutex::new(false),
        })
    }

    /// The saved key, when it is the one registered.
    pub fn shortcut(&self) -> Option<String> {
        self.saved
            .lock()
            .ok()
            .and_then(|saved| saved.shortcut.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn launch() -> Launch {
        Launch {
            id: uuid::Uuid::new_v4().to_string(),
            revision: 0,
            phase: "pending".into(),
            request: serde_json::json!({ "isolationMode": "localDirect", "turn": { "rawPrompt": "Fix this" } }),
            workspace_id: None,
            session_id: None,
            error: None,
        }
    }
    #[test]
    fn cannot_replay_or_change_a_launch_scope() {
        let current = launch();
        let mut next = current.clone();
        next.phase = "creating".into();
        assert!(transition(&current, &next).is_err());
        next.revision = 1;
        assert!(transition(&current, &next).is_ok());
        assert!(transition(&next, &next).is_err());
        next.request["isolationMode"] = "protectedWorktree".into();
        assert!(transition(&current, &next).is_err());
    }
    #[test]
    fn checkpoints_require_task_and_session_identity() {
        let mut current = launch();
        current.phase = "creating".into();
        let mut next = current.clone();
        next.revision = 1;
        next.phase = "workspaceReady".into();
        assert!(transition(&current, &next).is_err());
        next.workspace_id = Some("workspace".into());
        assert!(transition(&current, &next).is_ok());
        current = next.clone();
        next.revision += 1;
        next.phase = "starting".into();
        next.workspace_id = Some("another".into());
        assert!(transition(&current, &next).is_err());
    }
    #[test]
    fn journal_survives_reopen_and_validates_requests() {
        let root = tempfile::tempdir().unwrap();
        let state = QuickComposerState {
            root: root.path().into(),
            saved: Mutex::new(Saved::default()),
            shortcut_error: Mutex::new(false),
        };
        let request = launch();
        validate_launch(&request).unwrap();
        let saved = Saved {
            shortcut: None,
            launch: Some(request.clone()),
        };
        state.persist(&saved).unwrap();
        let recovered: Saved =
            serde_json::from_slice(&fs::read(root.path().join("state.json")).unwrap()).unwrap();
        assert_eq!(recovered.launch.unwrap().id, request.id);
        let mut invalid = request;
        invalid.request["turn"]["rawPrompt"] = " ".into();
        assert!(validate_launch(&invalid).is_err());
    }
}
