use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use dcc_core::domain::workspace::{
    WorkspaceSetupReport, WorkspaceSetupStatus, WorkspaceSetupStepReport,
};
use dcc_infra::git::WorkspaceSetupSuggestion;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceSetupFailurePolicy {
    ContinueOnFailure,
    RollbackOnFailure,
}

#[derive(Clone, Debug)]
pub struct WorkspaceSetupExecutionOutcome {
    pub report: WorkspaceSetupReport,
    pub should_rollback: bool,
}

pub async fn run_detected_workspace_setup(
    workspace_root: String,
    suggestions: Vec<WorkspaceSetupSuggestion>,
) -> Result<WorkspaceSetupReport, String> {
    Ok(run_workspace_setup_with_options(
        workspace_root,
        suggestions,
        WorkspaceSetupFailurePolicy::ContinueOnFailure,
    )
    .await?
    .report)
}

pub async fn run_workspace_setup_with_options(
    workspace_root: String,
    suggestions: Vec<WorkspaceSetupSuggestion>,
    failure_policy: WorkspaceSetupFailurePolicy,
) -> Result<WorkspaceSetupExecutionOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || {
        run_workspace_setup_with_options_blocking(&workspace_root, &suggestions, failure_policy)
    })
    .await
    .map_err(|error| error.to_string())?
}

pub fn run_workspace_setup_with_options_blocking(
    workspace_root: &str,
    suggestions: &[WorkspaceSetupSuggestion],
    failure_policy: WorkspaceSetupFailurePolicy,
) -> Result<WorkspaceSetupExecutionOutcome, String> {
    if suggestions.is_empty() {
        return Ok(WorkspaceSetupExecutionOutcome {
            report: WorkspaceSetupReport {
                status: WorkspaceSetupStatus::Skipped,
                steps: Vec::new(),
                message: None,
            },
            should_rollback: false,
        });
    }

    let mut steps = Vec::with_capacity(suggestions.len());
    let mut saw_warning = false;

    for suggestion in suggestions {
        match run_workspace_setup_command(workspace_root, &suggestion.command) {
            Ok(()) => steps.push(WorkspaceSetupStepReport {
                label: suggestion.label.clone(),
                command: suggestion.command.clone(),
                source_path: suggestion.source_path.clone(),
                status: WorkspaceSetupStatus::Completed,
                detail: None,
            }),
            Err(reason) if setup_failure_mentions_missing_node_runtime(&reason) => {
                saw_warning = true;
                steps.push(WorkspaceSetupStepReport {
                    label: suggestion.label.clone(),
                    command: suggestion.command.clone(),
                    source_path: suggestion.source_path.clone(),
                    status: WorkspaceSetupStatus::Warning,
                    detail: Some(reason),
                });
            }
            Err(reason) => {
                steps.push(WorkspaceSetupStepReport {
                    label: suggestion.label.clone(),
                    command: suggestion.command.clone(),
                    source_path: suggestion.source_path.clone(),
                    status: WorkspaceSetupStatus::Failed,
                    detail: Some(reason.clone()),
                });
                return Ok(WorkspaceSetupExecutionOutcome {
                    report: WorkspaceSetupReport {
                        status: WorkspaceSetupStatus::Failed,
                        steps,
                        message: Some(format!(
                            "Automatic workspace setup failed while running `{}`.",
                            suggestion.command
                        )),
                    },
                    should_rollback: matches!(
                        failure_policy,
                        WorkspaceSetupFailurePolicy::RollbackOnFailure
                    ),
                });
            }
        }
    }

    Ok(WorkspaceSetupExecutionOutcome {
        report: WorkspaceSetupReport {
            status: if saw_warning {
                WorkspaceSetupStatus::Warning
            } else {
                WorkspaceSetupStatus::Completed
            },
            steps,
            message: if saw_warning {
                Some(
                    "Workspace was created, but some setup steps need manual intervention."
                        .to_string(),
                )
            } else {
                Some("Workspace setup completed successfully.".to_string())
            },
        },
        should_rollback: false,
    })
}

fn setup_failure_mentions_missing_node_runtime(reason: &str) -> bool {
    let reason = reason.to_lowercase();
    let node_missing_patterns = [
        "command not found: node",
        "command not found: npm",
        "command not found: pnpm",
        "command not found: yarn",
        "node: command not found",
        "npm: command not found",
        "pnpm: command not found",
        "yarn: command not found",
        "/usr/bin/env: node: no such file or directory",
        "'node' is not recognized as an internal or external command",
        "'npm' is not recognized as an internal or external command",
        "'pnpm' is not recognized as an internal or external command",
        "'yarn' is not recognized as an internal or external command",
    ];

    node_missing_patterns
        .iter()
        .any(|pattern| reason.contains(pattern))
}

fn run_workspace_setup_command(workspace_root: &str, script: &str) -> Result<(), String> {
    let script = script.trim();
    if script.is_empty() {
        return Ok(());
    }

    #[cfg(windows)]
    let output = {
        let mut command = Command::new("cmd");
        command.args(["/C", script]).current_dir(workspace_root);
        command.creation_flags(0x08000000);
        command.output().map_err(|error| error.to_string())?
    };

    #[cfg(not(windows))]
    let output = run_workspace_setup_command_unix(workspace_root, script)?;

    if output.status.success() {
        return Ok(());
    }

    Err(format_output_failure(&output))
}

#[cfg(not(windows))]
fn run_workspace_setup_command_unix(
    workspace_root: &str,
    script: &str,
) -> Result<std::process::Output, String> {
    let shells = [
        std::env::var("SHELL").ok(),
        Some("/bin/zsh".to_string()),
        Some("/bin/bash".to_string()),
        Some("/bin/sh".to_string()),
    ];

    let mut last_error: Option<std::io::Error> = None;
    for shell in shells.into_iter().flatten() {
        match Command::new(&shell)
            .args(["-lc", script])
            .current_dir(workspace_root)
            .output()
        {
            Ok(output) => return Ok(output),
            Err(error) => {
                last_error = Some(error);
            }
        }
    }

    Err(last_error
        .map(|error| error.to_string())
        .unwrap_or_else(|| "no shell available to execute workspace setup".to_string()))
}

fn format_output_failure(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut detail = String::new();
    if !stderr.trim().is_empty() {
        detail.push_str(stderr.trim());
    }
    if !stdout.trim().is_empty() {
        if !detail.is_empty() {
            detail.push('\n');
        }
        detail.push_str(stdout.trim());
    }
    if detail.is_empty() {
        detail = format!(
            "exit code {}",
            output
                .status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "?".to_string())
        );
    }
    detail
}
