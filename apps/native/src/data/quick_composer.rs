//! The quick composer's launch (`quick-composer/api.ts`, `launch.ts`): the
//! request (project, environment, provider, model, the prompt and its turn
//! options), and its journal in the desktop (`begin`, `checkpoint`). Intent
//! is saved before each step outside — create the task, start the
//! conversation, send — and an interrupted step is never replayed: the
//! prompt and what is known stay for the person. The project and its
//! environment last picked live in `quick-composer.json`.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::desktop_api::{ApiError, CreateTaskRequest, DesktopApi, TurnOptions};
use super::sse::{Body, SseFrames};

/// `isolationMode`
pub const LOCAL: &str = "localDirect";
pub const WORKTREE: &str = "protectedWorktree";

/// The turn's options as sent (`ComposerSubmittedTurn.envelope`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Envelope {
    pub effort: Option<String>,
    pub approval_policy: Option<String>,
    pub plan_mode: bool,
    pub fast_mode: bool,
    pub tool_instructions: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Turn {
    pub raw_prompt: String,
    pub envelope: Envelope,
}

/// `QuickRequest`
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Request {
    pub project_id: String,
    pub root_path: String,
    pub base_branch: String,
    pub isolation_mode: String,
    pub provider_id: String,
    pub model_id: String,
    pub provider_runtime: Option<dcc_core::ports::provider::ProviderRuntimeConfig>,
    pub turn: Turn,
}

impl Request {
    /// The task's name: the prompt's first line, 80 characters at most.
    pub fn task_name(&self) -> String {
        self.turn
            .raw_prompt
            .trim()
            .lines()
            .next()
            .unwrap_or_default()
            .chars()
            .take(80)
            .collect()
    }

    /// What `sendTurn` gets.
    pub fn turn_options(&self) -> TurnOptions {
        let envelope = &self.turn.envelope;
        TurnOptions {
            provider_id: Some(self.provider_id.clone()),
            model: Some(self.model_id.clone()),
            effort: envelope.effort.clone(),
            approval_policy: envelope.approval_policy.clone(),
            plan_mode: envelope.plan_mode,
            fast_mode: envelope.fast_mode,
            tool_instructions: envelope.tool_instructions.clone(),
            provider_runtime: self.provider_runtime.clone(),
        }
    }
}

/// `QuickLaunch`
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Launch {
    pub id: String,
    pub revision: u32,
    /// pending, creating, workspaceReady, starting, sessionReady, sending,
    /// completed or failed.
    pub phase: String,
    pub request: Request,
    pub workspace_id: Option<String>,
    pub session_id: Option<String>,
    pub error: Option<String>,
}

impl Launch {
    /// `newQuickLaunch`
    pub fn new(request: Request) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            revision: 0,
            phase: "pending".into(),
            request,
            ..Self::default()
        }
    }

    pub fn finished(&self) -> bool {
        matches!(self.phase.as_str(), "completed" | "failed")
    }
}

/// The steps outside, in order; each may fail.
pub trait Steps {
    fn begin(&self, launch: &Launch) -> Result<Launch, String>;
    fn checkpoint(&self, launch: &Launch) -> Result<Launch, String>;
    fn create_task(&self, launch: &Launch) -> Result<String, String>;
    fn start_session(&self, launch: &Launch) -> Result<String, String>;
    fn send(&self, launch: &Launch) -> Result<(), String>;
}

/// `executeQuickLaunch`: blocking; `progress` sees every saved state.
pub fn execute(initial: Launch, steps: &dyn Steps, progress: &mut dyn FnMut(&Launch)) -> Launch {
    let mut current = match steps.begin(&initial) {
        Ok(begun) => begun,
        Err(error) => {
            let failed = Launch {
                phase: "failed".into(),
                error: Some(error),
                ..initial
            };
            progress(&failed);
            return failed;
        }
    };
    progress(&current);
    if current.phase != "pending" {
        return current;
    }
    let mut checkpoint = |current: &mut Launch, phase: &str| -> Result<(), String> {
        let next = Launch {
            phase: phase.into(),
            revision: current.revision + 1,
            ..current.clone()
        };
        *current = steps.checkpoint(&next)?;
        progress(current);
        Ok(())
    };
    // Claimed outside the failure path: a losing run must not fail one
    // another run already claimed.
    if let Err(error) = checkpoint(&mut current, "creating") {
        return Launch {
            phase: "failed".into(),
            error: Some(error),
            ..current
        };
    }
    let run = |current: &mut Launch,
               checkpoint: &mut dyn FnMut(&mut Launch, &str) -> Result<(), String>|
     -> Result<(), String> {
        let workspace = steps.create_task(current)?;
        // The identities stay even when the checkpoint itself fails.
        current.workspace_id = Some(workspace);
        checkpoint(current, "workspaceReady")?;
        checkpoint(current, "starting")?;
        let session = steps.start_session(current)?;
        current.session_id = Some(session);
        checkpoint(current, "sessionReady")?;
        checkpoint(current, "sending")?;
        steps.send(current)?;
        checkpoint(current, "completed")
    };
    if let Err(error) = run(&mut current, &mut checkpoint) {
        let failed = Launch {
            phase: "failed".into(),
            revision: current.revision + 1,
            error: Some(error.clone()),
            ..current.clone()
        };
        match steps.checkpoint(&failed) {
            Ok(saved) => current = saved,
            // A lost answer may have moved the journal: never retry a
            // creation or a send to repair that.
            Err(_) => current = failed,
        }
        progress(&current);
    }
    current
}

/// The steps through the desktop.
pub struct DesktopSteps {
    pub api: DesktopApi,
    pub db_path: std::path::PathBuf,
}

fn message(error: ApiError) -> String {
    error.to_string()
}

impl Steps for DesktopSteps {
    fn begin(&self, launch: &Launch) -> Result<Launch, String> {
        self.api.quick_launch("begin", launch).map_err(message)
    }

    fn checkpoint(&self, launch: &Launch) -> Result<Launch, String> {
        self.api.quick_launch("checkpoint", launch).map_err(message)
    }

    fn create_task(&self, launch: &Launch) -> Result<String, String> {
        let request = &launch.request;
        let name = request.task_name();
        let created = self
            .api
            .create_task(&CreateTaskRequest {
                project_id: request.project_id.clone(),
                workspace_root: request.root_path.clone(),
                base_branch: request.base_branch.clone(),
                worktree: request.isolation_mode == WORKTREE,
                include_linked_folders: true,
                bundle_name: name.clone(),
                name: Some(name),
            })
            .map_err(message)?;
        Ok(created.workspace_id)
    }

    fn start_session(&self, launch: &Launch) -> Result<String, String> {
        let workspace = launch.workspace_id.as_deref().unwrap_or_default();
        // From the task as saved, so a resumed launch finds the same projects.
        let start = super::conversation::thread_start_for(&self.db_path, workspace)
            .map_err(|error| error.to_string())?;
        let request = &launch.request;
        self.api
            .start_thread(
                &start,
                &request.provider_id,
                Some(&request.model_id),
                Some(&request.task_name()),
            )
            .map_err(message)
    }

    fn send(&self, launch: &Launch) -> Result<(), String> {
        let session = launch.session_id.as_deref().unwrap_or_default();
        let request = &launch.request;
        self.api
            .send_turn(
                session,
                &request.turn.raw_prompt,
                None,
                &request.turn_options(),
            )
            .map_err(message)
    }
}

/// `quick_composer_status`'s launch.
pub fn launch_of(status: &Value) -> Option<Launch> {
    serde_json::from_value(status.get("launch")?.clone()).ok()
}

/// The project and the environment per project last picked.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Selection {
    pub project_id: String,
    pub modes: BTreeMap<String, String>,
}

impl Selection {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        std::fs::write(
            path,
            serde_json::to_vec(self).map_err(std::io::Error::other)?,
        )
    }

    /// `protectedWorktree` unless `localDirect` was picked for the project.
    pub fn mode(&self) -> &'static str {
        if self.modes.get(&self.project_id).map(String::as_str) == Some(LOCAL) {
            LOCAL
        } else {
            WORKTREE
        }
    }
}

/// The key's presses (`toggle` frames); `None` once the stream ends.
pub struct Toggles {
    body: Body,
    frames: SseFrames,
}

impl Iterator for Toggles {
    type Item = ();

    fn next(&mut self) -> Option<()> {
        use std::io::Read;
        let mut buffer = [0u8; 4096];
        loop {
            while let Some((name, _)) = self.frames.next_frame() {
                if name == "toggle" {
                    return Some(());
                }
            }
            match self.body.read(&mut buffer) {
                Ok(0) | Err(_) => return None,
                Ok(read) => self.frames.feed(&buffer[..read]),
            }
        }
    }
}

impl DesktopApi {
    fn quick_launch(&self, step: &str, launch: &Launch) -> Result<Launch, ApiError> {
        let value = self.post(&format!("/api/v1/quick-composer/{step}"), &json!(launch))?;
        serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
    }

    /// `quick_composer_status` as it comes (the launch is in it).
    pub fn quick_composer_raw(&self) -> Result<Value, ApiError> {
        self.get("/api/v1/quick-composer/status")
    }

    /// While this app listens, the key opens its quick composer.
    pub fn quick_composer_toggles(&self) -> Result<Toggles, ApiError> {
        let (reader, chunked) = self.open_stream(
            "/api/v1/quick-composer/stream",
            std::time::Duration::from_secs(45),
        )?;
        Ok(Toggles {
            body: Body::new(reader, chunked),
            frames: SseFrames::default(),
        })
    }
}

#[cfg(test)]
#[path = "quick_composer_tests.rs"]
mod tests;
