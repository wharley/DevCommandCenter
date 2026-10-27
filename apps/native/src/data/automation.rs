//! "Automação do projeto" (`WorkspaceProjectAutomationDialog`): the task's
//! `.dcc.toml` — the setup command, the delivery policy, the checks and
//! fixes with the hooks they run in — edited as a draft with the desktop's
//! rules, saved against the hash it was read with, and its tasks run in the
//! checkout. Over `/api/v1/workspaces/automation`.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::desktop_api::{ApiError, DesktopApi};

/// A run waits for its tasks (each up to an hour).
const RUN_WAIT: Duration = Duration::from_secs(4 * 60 * 60);

/// `WorkspaceProjectTaskKind`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Check,
    Fix,
}

impl Kind {
    pub fn key(self) -> &'static str {
        match self {
            Kind::Check => "check",
            Kind::Fix => "fix",
        }
    }
}

/// `WorkspaceProjectTask`
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub label: Option<String>,
    pub command: String,
    pub kind: Kind,
    pub cwd: Option<String>,
    pub timeout_seconds: u64,
}

impl Task {
    /// `taskLabel`
    pub fn title(&self) -> String {
        self.label
            .as_deref()
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .unwrap_or(&self.id)
            .to_string()
    }
}

/// `WorkspaceDeliveryPolicy`
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Policy {
    pub minimum_approvals: u32,
    pub require_pipeline: bool,
    pub require_resolved_discussions: bool,
    pub require_current_base: bool,
    pub require_before_merge_checks: bool,
}

/// The policy's switches, in the desktop's order (`automation.<key>`).
pub const REQUIREMENTS: [&str; 4] = [
    "requirePipeline",
    "requireResolvedDiscussions",
    "requireCurrentBase",
    "requireBeforeMergeChecks",
];

impl Policy {
    pub fn required(&self, key: &str) -> bool {
        match key {
            "requirePipeline" => self.require_pipeline,
            "requireResolvedDiscussions" => self.require_resolved_discussions,
            "requireCurrentBase" => self.require_current_base,
            "requireBeforeMergeChecks" => self.require_before_merge_checks,
            _ => false,
        }
    }

    pub fn set_required(&mut self, key: &str, on: bool) {
        match key {
            "requirePipeline" => self.require_pipeline = on,
            "requireResolvedDiscussions" => self.require_resolved_discussions = on,
            "requireCurrentBase" => self.require_current_base = on,
            "requireBeforeMergeChecks" => self.require_before_merge_checks = on,
            _ => {}
        }
    }
}

/// `WorkspaceProjectAutomationConfigOutput`
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub setup_command: Option<String>,
    pub tasks: Vec<Task>,
    pub before_merge: Vec<String>,
    pub before_push: Vec<String>,
    pub delivery_policy: Policy,
    pub source_path: String,
    pub config_hash: Option<String>,
    pub tracked_in_git: bool,
}

/// Which hook a check runs in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hook {
    BeforeMerge,
    BeforePush,
}

/// The dialog's `Draft`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Draft {
    pub setup_command: String,
    pub tasks: Vec<Task>,
    pub before_merge: Vec<String>,
    pub before_push: Vec<String>,
    pub policy: Policy,
}

impl From<&Config> for Draft {
    fn from(config: &Config) -> Self {
        Self {
            setup_command: config.setup_command.clone().unwrap_or_default(),
            tasks: config.tasks.clone(),
            before_merge: config.before_merge.clone(),
            before_push: config.before_push.clone(),
            policy: config.delivery_policy.clone(),
        }
    }
}

impl Draft {
    /// `isDirty`: unsaved changes (runs wait for a save).
    pub fn dirty(&self, config: &Config) -> bool {
        *self != Draft::from(config)
    }

    /// `checkTaskIds`
    pub fn check_ids(&self) -> Vec<String> {
        self.tasks
            .iter()
            .filter(|task| task.kind == Kind::Check)
            .map(|task| task.id.clone())
            .collect()
    }

    pub fn hooked(&self, hook: Hook, id: &str) -> bool {
        self.hook(hook).iter().any(|hooked| hooked == id)
    }

    fn hook(&self, hook: Hook) -> &Vec<String> {
        match hook {
            Hook::BeforeMerge => &self.before_merge,
            Hook::BeforePush => &self.before_push,
        }
    }

    /// `updateTask`: a renamed id follows into the hooks; a fix leaves them.
    pub fn update_task(&mut self, index: usize, change: impl FnOnce(&mut Task)) {
        let Some(task) = self.tasks.get_mut(index) else {
            return;
        };
        let previous = task.id.clone();
        change(task);
        let (id, fix) = (task.id.clone(), task.kind == Kind::Fix);
        for hook in [&mut self.before_merge, &mut self.before_push] {
            if id != previous {
                for hooked in hook.iter_mut() {
                    if *hooked == previous {
                        *hooked = id.clone();
                    }
                }
            }
            if fix {
                hook.retain(|hooked| *hooked != id);
            }
        }
    }

    /// `toggleHook`
    pub fn toggle_hook(&mut self, hook: Hook, id: &str) {
        let hook = match hook {
            Hook::BeforeMerge => &mut self.before_merge,
            Hook::BeforePush => &mut self.before_push,
        };
        if let Some(at) = hook.iter().position(|hooked| hooked == id) {
            hook.remove(at);
        } else {
            hook.push(id.to_string());
        }
    }

    /// `addTask`: `check_<n>` with the first free number, 600s.
    pub fn add_task(&mut self) {
        let mut suffix = self.tasks.len() + 1;
        while self
            .tasks
            .iter()
            .any(|task| task.id == format!("check_{suffix}"))
        {
            suffix += 1;
        }
        self.tasks.push(Task {
            id: format!("check_{suffix}"),
            label: Some(String::new()),
            command: String::new(),
            kind: Kind::Check,
            cwd: None,
            timeout_seconds: 600,
        });
    }

    /// `removeTask`: and out of the hooks.
    pub fn remove_task(&mut self, index: usize) {
        if index >= self.tasks.len() {
            return;
        }
        let id = self.tasks.remove(index).id;
        self.before_merge.retain(|hooked| *hooked != id);
        self.before_push.retain(|hooked| *hooked != id);
    }

    /// `save`'s body: trimmed, empty as none, hooks without repeats.
    pub fn save_body(&self, root: &str, expected_hash: Option<&str>) -> Value {
        let unique = |ids: &[String]| {
            let mut seen: Vec<String> = Vec::new();
            for id in ids {
                let id = id.trim().to_string();
                if !seen.contains(&id) {
                    seen.push(id);
                }
            }
            seen
        };
        let trimmed = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let tasks: Vec<Task> = self
            .tasks
            .iter()
            .map(|task| Task {
                id: task.id.trim().to_string(),
                label: trimmed(&task.label),
                command: task.command.trim().to_string(),
                cwd: trimmed(&task.cwd),
                ..task.clone()
            })
            .collect();
        json!({
            "workspaceRoot": root,
            "setupCommand": Some(self.setup_command.trim()).filter(|command| !command.is_empty()),
            "tasks": tasks,
            "beforeMerge": unique(&self.before_merge),
            "beforePush": unique(&self.before_push),
            "deliveryPolicy": self.policy,
            "expectedConfigHash": expected_hash,
        })
    }
}

/// One command of a run.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Step {
    pub command: String,
    pub success: bool,
    pub output: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Report {
    /// `passed` | `failed`
    pub status: String,
    pub steps: Vec<Step>,
}

/// `WorkspaceRunProjectTasksOutput`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RunOutput {
    pub report: Report,
    pub changed_files: bool,
}

impl RunOutput {
    pub fn passed(&self) -> bool {
        self.report.status == "passed"
    }
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
}

impl DesktopApi {
    /// `workspaceProjectAutomationConfig`
    pub fn automation(&self, root: &str) -> Result<Config, ApiError> {
        decode(self.post(
            "/api/v1/workspaces/automation",
            &json!({ "workspaceRoot": root }),
        )?)
    }

    /// `workspaceSaveProjectAutomation`
    pub fn save_automation(
        &self,
        root: &str,
        draft: &Draft,
        expected_hash: Option<&str>,
    ) -> Result<Config, ApiError> {
        decode(self.put(
            "/api/v1/workspaces/automation",
            &draft.save_body(root, expected_hash),
        )?)
    }

    /// `workspaceRunProjectTasks`
    pub fn run_automation(
        &self,
        root: &str,
        task_ids: &[String],
        expected_hash: Option<&str>,
    ) -> Result<RunOutput, ApiError> {
        decode(self.request_waiting(
            "POST",
            "/api/v1/workspaces/automation/run",
            Some(&json!({
                "workspaceRoot": root,
                "taskIds": task_ids,
                "expectedConfigHash": expected_hash,
            })),
            RUN_WAIT,
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_draft_follows_the_desktops_rules() {
        let mut draft = Draft::default();
        draft.add_task();
        draft.add_task();
        assert_eq!(draft.tasks[1].id, "check_2");
        draft.toggle_hook(Hook::BeforeMerge, "check_1");
        draft.toggle_hook(Hook::BeforePush, "check_1");
        // Renamed: the hooks follow.
        draft.update_task(0, |task| task.id = "lint".into());
        assert!(draft.hooked(Hook::BeforeMerge, "lint"));
        assert!(draft.hooked(Hook::BeforePush, "lint"));
        // A fix runs by hand only: out of the hooks.
        draft.update_task(0, |task| task.kind = Kind::Fix);
        assert!(!draft.hooked(Hook::BeforeMerge, "lint"));
        assert_eq!(draft.check_ids(), ["check_2"]);
        draft.toggle_hook(Hook::BeforeMerge, "check_2");
        draft.remove_task(1);
        assert!(draft.before_merge.is_empty());
        // Saved trimmed, empty as none.
        draft.tasks[0].label = Some("  ".into());
        draft.tasks[0].command = " yarn lint --fix ".into();
        draft.setup_command = "  ".into();
        let body = draft.save_body("/repo", Some("h1"));
        assert_eq!(body["setupCommand"], Value::Null);
        assert_eq!(body["tasks"][0]["label"], Value::Null);
        assert_eq!(body["tasks"][0]["command"], "yarn lint --fix");
        assert_eq!(body["tasks"][0]["kind"], "fix");
        assert_eq!(body["expectedConfigHash"], "h1");
        assert_eq!(draft.tasks[0].title(), "lint");
    }

    #[test]
    fn a_saved_config_is_not_dirty_and_the_policy_switches() {
        let config: Config = serde_json::from_value(json!({
            "setupCommand": null, "tasks": [], "beforeMerge": [], "beforePush": [],
            "deliveryPolicy": { "minimumApprovals": 0, "requirePipeline": false,
                "requireResolvedDiscussions": false, "requireCurrentBase": false,
                "requireBeforeMergeChecks": false },
            "sourcePath": "/repo/.dcc.toml", "configHash": null, "trackedInGit": false,
        }))
        .unwrap();
        let mut draft = Draft::from(&config);
        assert!(!draft.dirty(&config));
        draft.policy.set_required("requirePipeline", true);
        assert!(draft.policy.required("requirePipeline"));
        assert!(draft.dirty(&config));
    }
}
