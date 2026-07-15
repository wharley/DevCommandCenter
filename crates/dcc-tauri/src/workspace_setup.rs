use std::{
    process::{Command, Output, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

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

pub const WORKSPACE_VALIDATION_TIMEOUT: Duration = Duration::from_secs(600);
const MAX_COMMAND_OUTPUT_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug)]
pub struct WorkspaceCommandExecution {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub output: String,
    pub timed_out: bool,
    pub duration_ms: u64,
    pub truncated: bool,
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

pub fn run_workspace_validation_command(
    workspace_root: &str,
    script: &str,
) -> Result<WorkspaceCommandExecution, String> {
    run_workspace_command_with_timeout(workspace_root, script, WORKSPACE_VALIDATION_TIMEOUT)
}

pub fn run_workspace_task_command(
    workspace_root: &str,
    script: &str,
    timeout_seconds: u64,
) -> Result<WorkspaceCommandExecution, String> {
    run_workspace_command_with_timeout(
        workspace_root,
        script,
        Duration::from_secs(timeout_seconds.clamp(1, 3600)),
    )
}

fn run_workspace_command_with_timeout(
    workspace_root: &str,
    script: &str,
    timeout: Duration,
) -> Result<WorkspaceCommandExecution, String> {
    let script = script.trim();
    if script.is_empty() {
        return Ok(WorkspaceCommandExecution {
            success: true,
            exit_code: Some(0),
            output: String::new(),
            timed_out: false,
            duration_ms: 0,
            truncated: false,
        });
    }

    let mut command = validation_shell_command(workspace_root, script)?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("CI", "1")
        .env("GIT_TERMINAL_PROMPT", "0");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let started = Instant::now();
    let child = command.spawn().map_err(|error| error.to_string())?;
    let child_pid = child.id();
    let (tx, rx) = mpsc::channel();
    let waiter = thread::spawn(move || {
        let result = child.wait_with_output();
        let _ = tx.send(result);
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) => {
            let _ = waiter.join();
            Ok(command_execution_from_output(
                output,
                started.elapsed(),
                false,
            ))
        }
        Ok(Err(error)) => {
            let _ = waiter.join();
            Err(error.to_string())
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            kill_process_tree(child_pid);
            let _ = waiter.join();
            let output = match rx.try_recv() {
                Ok(Ok(output)) => output,
                Ok(Err(error)) => return Err(error.to_string()),
                Err(_) => {
                    return Ok(WorkspaceCommandExecution {
                        success: false,
                        exit_code: None,
                        output: format!("command timed out after {}s", timeout.as_secs()),
                        timed_out: true,
                        duration_ms: started.elapsed().as_millis() as u64,
                        truncated: false,
                    });
                }
            };
            let mut execution = command_execution_from_output(output, started.elapsed(), true);
            if execution.output.is_empty() {
                execution.output = format!("command timed out after {}s", timeout.as_secs());
            }
            Ok(execution)
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let _ = waiter.join();
            Err("validation command waiter stopped unexpectedly".to_string())
        }
    }
}

fn validation_shell_command(workspace_root: &str, script: &str) -> Result<Command, String> {
    #[cfg(windows)]
    {
        let mut command = Command::new("cmd");
        command.args(["/C", script]).current_dir(workspace_root);
        command.creation_flags(0x08000000);
        return Ok(command);
    }

    #[cfg(not(windows))]
    {
        let shell = [
            std::env::var("SHELL").ok(),
            Some("/bin/zsh".to_string()),
            Some("/bin/bash".to_string()),
            Some("/bin/sh".to_string()),
        ]
        .into_iter()
        .flatten()
        .find(|shell| std::path::Path::new(shell).is_file())
        .ok_or_else(|| "no shell available to execute validations".to_string())?;
        let mut command = Command::new(shell);
        command.args(["-lc", script]).current_dir(workspace_root);
        Ok(command)
    }
}

fn kill_process_tree(child_pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(child_pid as libc::pid_t), libc::SIGKILL);
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .arg("/PID")
            .arg(child_pid.to_string())
            .arg("/T")
            .arg("/F")
            .output();
    }
}

fn command_execution_from_output(
    output: Output,
    duration: Duration,
    timed_out: bool,
) -> WorkspaceCommandExecution {
    let success = !timed_out && output.status.success();
    let exit_code = output.status.code();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = match (stdout.trim().is_empty(), stderr.trim().is_empty()) {
        (false, false) => format!("{}\n{}", stdout.trim_end(), stderr.trim_end()),
        (false, true) => stdout.trim_end().to_string(),
        (true, false) => stderr.trim_end().to_string(),
        (true, true) => String::new(),
    };
    let (output, truncated) = truncate_command_output(combined);
    WorkspaceCommandExecution {
        success,
        exit_code,
        output,
        timed_out,
        duration_ms: duration.as_millis() as u64,
        truncated,
    }
}

fn truncate_command_output(value: String) -> (String, bool) {
    if value.len() <= MAX_COMMAND_OUTPUT_BYTES {
        return (value, false);
    }
    let mut end = MAX_COMMAND_OUTPUT_BYTES;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    (
        format!("{}\n\n[output truncated by DCC]", &value[..end]),
        true,
    )
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
