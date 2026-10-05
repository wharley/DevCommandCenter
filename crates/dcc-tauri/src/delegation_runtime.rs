//! Backend-owned delegation lifecycle.
//!
//! A delegation runs as a child thread on (usually) another provider. Everything
//! that must survive a webview reload lives here: launching the child, noticing
//! when its turn ends, recording the result, handing it back to the parent agent
//! that asked for it, enforcing the timeout budget, and telling the parent when
//! the person applies or discards delegated edits.

use std::{
    collections::HashSet,
    sync::{Mutex, OnceLock},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use dcc_core::{
    application::{
        queue_turn as run_queue_turn, start_thread as run_start_thread, QueueTurnInput,
        SendTurnInput, StartThreadInput,
    },
    domain::{
        delegation::{
            Delegation, DelegationBudget, DelegationContextPolicy, DelegationId, DelegationMode,
            DelegationOrigin, DelegationStatus,
        },
        session::{AssistantMessagePhase, SessionEventKind, SessionId, SessionProjection, TurnId},
    },
    ports::{
        DelegationRepo, DelegationWorktreeOperationRepo, ProviderRuntimeConfig, SessionEventRepo,
        SessionRepo, WorkspaceRepo,
    },
};

use crate::{
    commands::{
        delegation_commands::{
            complete_delegation_with_state, create_delegation_with_state,
            fail_delegation_with_state, start_delegation_with_state, CompleteDelegationInput,
            CreateDelegationInput, FailDelegationInput, StartDelegationInput,
        },
        session_commands::send_turn_with_state,
        workspace_commands::{
            workspace_git_branch_diff_with_state, workspace_git_status_inner,
            workspace_prepare_delegation_worktree_with_state,
            workspace_remove_delegation_worktree_with_state, WorkspaceGitBranchDiffInput,
            WorkspaceGitChangeEntry, WorkspacePrepareDelegationWorktreeInput,
            WorkspaceRemoveDelegationWorktreeInput,
        },
    },
    state::{SessionCommandState, WorkspaceCommandState},
};

/// Provider ids a delegation can target (validated against availability at run time).
pub use dcc_providers::PROVIDER_IDS as DELEGATION_PROVIDER_IDS;

/// Agent-initiated delegations a single parent may have in flight at once.
pub const MAX_ACTIVE_AGENT_DELEGATIONS_PER_PARENT: usize = 4;
/// A child that never started its turn within this window is treated as lost.
const CHILD_START_GRACE_SECONDS: i64 = 180;
/// Fallback for records without a budget timeout.
const DEFAULT_TIMEOUT_SECONDS: u64 = 600;
/// Budgets the watchdog enforces: a read-only pass is shorter than an implementation.
const READ_ONLY_TIMEOUT_SECONDS: u64 = 15 * 60;
const IMPLEMENT_TIMEOUT_SECONDS: u64 = 30 * 60;
const MAX_SUMMARY_CHARS: usize = 4_000;
const MAX_INSTRUCTION_CHARS: usize = 600;
const MAX_VALIDATION_COMMANDS: usize = 12;
const MAX_LISTED_FILES: usize = 20;
const MAX_CONTEXT_CHANGES: usize = 80;

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RunDelegationInput {
    pub parent_session_id: SessionId,
    /// Omitted: the first available provider that is not the parent's.
    #[serde(default)]
    pub target_provider_id: Option<String>,
    #[serde(default)]
    pub target_model_id: Option<String>,
    pub mode: DelegationMode,
    pub instruction: String,
    #[serde(default)]
    pub context_policy: DelegationContextPolicy,
    #[serde(default)]
    pub origin: DelegationOrigin,
    /// Replays an earlier delegation's prompt verbatim (reruns).
    #[serde(default)]
    pub prebuilt_prompt: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub fast_mode: Option<bool>,
    /// The desktop passes the person's runtime settings; backend callers
    /// inherit them from the newest session of the same provider.
    #[serde(default)]
    pub provider_runtime: Option<ProviderRuntimeConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RunDelegationOutput {
    pub delegation: Delegation,
}

pub fn provider_label(provider_id: &str) -> String {
    match provider_id {
        "claude_code" => "Claude Code",
        "codex" => "Codex",
        "cursor" => "Cursor",
        "grok" => "Grok",
        "antigravity" => "Antigravity",
        "gemini" => "Gemini",
        "droid" => "Droid",
        other => other,
    }
    .to_string()
}

fn mode_label(mode: &DelegationMode) -> &'static str {
    match mode {
        DelegationMode::Review => "review",
        DelegationMode::Implement => "implement",
        DelegationMode::Explain => "explain",
        DelegationMode::Test => "test",
        DelegationMode::Research => "research",
    }
}

fn is_edit_capable(delegation: &Delegation) -> bool {
    matches!(delegation.mode, DelegationMode::Implement) || delegation.budget.allow_file_edits
}

fn clip(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let clipped: String = trimmed.chars().take(max_chars).collect();
    format!("{}… [truncated]", clipped.trim_end())
}

// ---------------------------------------------------------------------------
// Hand-back message (deterministic template; never generated)
// ---------------------------------------------------------------------------

/// The instruction block of a prompt built by [`build_delegation_prompt`].
pub fn extract_delegation_instruction(prompt: &str) -> String {
    let marker = "\nInstruction:\n";
    let Some(start) = prompt.find(marker) else {
        return prompt.trim().to_string();
    };
    let rest = &prompt[start + marker.len()..];
    let end = ["\n\nGit context:\n", "\n\nMission spec:\n", "\n\nRecent parent session context:\n"]
        .iter()
        .filter_map(|section| rest.find(section))
        .min()
        .unwrap_or(rest.len());
    rest[..end].trim().to_string()
}

fn delegation_instruction(delegation: &Delegation) -> String {
    delegation
        .instruction
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| extract_delegation_instruction(&delegation.prompt))
}

fn agent_label(delegation: &Delegation) -> String {
    let provider = provider_label(&delegation.target_provider_id.0);
    match delegation.target_model_id.as_deref().filter(|model| !model.trim().is_empty()) {
        Some(model) => format!("{provider} ({model})"),
        None => provider,
    }
}

/// The turn that hands a finished delegation back to its parent agent.
pub fn delegation_result_turn(delegation: &Delegation, failure_reason: Option<&str>) -> String {
    let failed = matches!(delegation.status, DelegationStatus::Failed);
    let mut lines = vec![
        format!(
            "[DCC] Delegated {} task {} — {} (task {}).",
            mode_label(&delegation.mode),
            if failed { "failed" } else { "finished" },
            agent_label(delegation),
            delegation.id.0
        ),
        String::new(),
        "Task you delegated:".to_string(),
        clip(&delegation_instruction(delegation), MAX_INSTRUCTION_CHARS),
    ];
    if failed {
        lines.extend([
            String::new(),
            format!(
                "Failure: {}",
                clip(
                    failure_reason
                        .filter(|reason| !reason.trim().is_empty())
                        .unwrap_or("no reason recorded"),
                    MAX_SUMMARY_CHARS
                )
            ),
            String::new(),
            "Decide whether to continue without this result or ask the human how to proceed."
                .to_string(),
        ]);
        return lines.join("\n");
    }
    lines.extend([
        String::new(),
        "Result:".to_string(),
        clip(
            delegation
                .result_summary
                .as_deref()
                .filter(|summary| !summary.trim().is_empty())
                .unwrap_or("The delegated agent returned no text."),
            MAX_SUMMARY_CHARS,
        ),
    ]);
    if !delegation.touched_files.is_empty() {
        lines.push(String::new());
        lines.push(format!("Files touched ({}):", delegation.touched_files.len()));
        lines.extend(
            delegation
                .touched_files
                .iter()
                .take(MAX_LISTED_FILES)
                .map(|file| format!("- {file}")),
        );
        if delegation.touched_files.len() > MAX_LISTED_FILES {
            lines.push(format!(
                "- … {} more",
                delegation.touched_files.len() - MAX_LISTED_FILES
            ));
        }
    }
    if matches!(delegation.status, DelegationStatus::ReviewPending) {
        lines.extend([
            String::new(),
            format!(
                "These edits live in an isolated delegation worktree and are NOT applied to your \
                 workspace yet. The human reviews and applies or discards them in the DCC \
                 Inspector — do not re-implement them.{}",
                if delegation.origin == DelegationOrigin::Agent {
                    " DCC will tell you the outcome."
                } else {
                    ""
                }
            ),
        ]);
    }
    if let Some(validation) = delegation
        .validation_summary
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        lines.extend([
            String::new(),
            "Validation:".to_string(),
            clip(validation, 1_200),
        ]);
    }
    lines.extend([String::new(), "Continue your task using this result.".to_string()]);
    lines.join("\n")
}

/// The turn that tells the parent what the person did with delegated edits.
pub fn delegation_review_outcome_turn(delegation: &Delegation, applied: bool) -> String {
    let mut lines = vec![format!(
        "[DCC] The human {} the edits from your delegated {} task — {} (task {}).",
        if applied { "APPLIED" } else { "DISCARDED" },
        mode_label(&delegation.mode),
        agent_label(delegation),
        delegation.id.0
    )];
    if applied {
        if !delegation.touched_files.is_empty() {
            lines.push(String::new());
            lines.push("They are now in your workspace:".to_string());
            lines.extend(
                delegation
                    .touched_files
                    .iter()
                    .take(MAX_LISTED_FILES)
                    .map(|file| format!("- {file}")),
            );
        }
        lines.extend([
            String::new(),
            "Re-read the affected files before continuing.".to_string(),
        ]);
    } else {
        lines.extend([
            String::new(),
            "None of those edits reached your workspace. Continue without them, or do the work \
             yourself if it is still needed."
                .to_string(),
        ]);
    }
    lines.join("\n")
}

// ---------------------------------------------------------------------------
// Child session summary
// ---------------------------------------------------------------------------

#[derive(Debug, Default, PartialEq)]
pub struct ChildTurnSummary {
    pub assistant_text: Option<String>,
    pub commands: Vec<String>,
}

/// Final assistant text and observed commands of the child's latest turn.
pub fn summarize_child_turn(events: &[dcc_core::domain::session::SessionEventRecord]) -> ChildTurnSummary {
    let Some(turn_id) = events.iter().rev().find_map(|event| match &event.kind {
        SessionEventKind::TurnStarted { turn_id, .. } => Some(turn_id.clone()),
        _ => None,
    }) else {
        return ChildTurnSummary::default();
    };

    struct Message {
        id: String,
        phase: AssistantMessagePhase,
        content: String,
    }
    let mut messages: Vec<Message> = Vec::new();
    let mut commands: Vec<String> = Vec::new();
    let message_index = |messages: &mut Vec<Message>, id: &str| -> usize {
        messages
            .iter()
            .position(|message| message.id == id)
            .unwrap_or_else(|| {
                messages.push(Message {
                    id: id.to_string(),
                    phase: AssistantMessagePhase::Unknown,
                    content: String::new(),
                });
                messages.len() - 1
            })
    };
    for event in events {
        match &event.kind {
            SessionEventKind::TurnDelta { turn_id: id, content } if *id == turn_id => {
                let index = message_index(&mut messages, "legacy");
                messages[index].content.push_str(content);
            }
            SessionEventKind::TurnAssistantMessageStarted {
                turn_id: id,
                message_id,
                phase,
            } if *id == turn_id => {
                let index = message_index(&mut messages, message_id);
                messages[index].phase = phase.clone();
            }
            SessionEventKind::TurnAssistantMessageDelta {
                turn_id: id,
                message_id,
                content,
            } if *id == turn_id => {
                let index = message_index(&mut messages, message_id);
                messages[index].content.push_str(content);
            }
            SessionEventKind::TurnAssistantMessageCompleted {
                turn_id: id,
                message_id,
                phase,
                content,
            } if *id == turn_id => {
                let index = message_index(&mut messages, message_id);
                messages[index].phase = phase.clone();
                if let Some(content) = content {
                    messages[index].content = content.clone();
                }
            }
            SessionEventKind::TurnToolCallStarted {
                turn_id: id,
                action,
                command,
                ..
            } if *id == turn_id => {
                let value = command
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or(action.as_str())
                    .trim()
                    .to_string();
                if !value.is_empty() && !commands.contains(&value) {
                    commands.push(value);
                }
            }
            _ => {}
        }
    }
    let terminal = messages
        .iter()
        .rposition(|message| message.phase == AssistantMessagePhase::FinalAnswer)
        .or_else(|| {
            messages
                .iter()
                .rposition(|message| !message.content.trim().is_empty())
        });
    ChildTurnSummary {
        assistant_text: terminal
            .map(|index| messages[index].content.trim().to_string())
            .filter(|text| !text.is_empty()),
        commands,
    }
}

fn validation_summary(commands: &[String]) -> String {
    if commands.is_empty() {
        return "No tool commands observed in the delegated session.".to_string();
    }
    let mut lines = vec!["Observed child-session commands:".to_string()];
    lines.extend(
        commands
            .iter()
            .take(MAX_VALIDATION_COMMANDS)
            .map(|command| format!("- {command}")),
    );
    if commands.len() > MAX_VALIDATION_COMMANDS {
        lines.push(format!(
            "- … {} more command(s)",
            commands.len() - MAX_VALIDATION_COMMANDS
        ));
    }
    let validation_like = commands
        .iter()
        .filter(|command| {
            let lower = command.to_lowercase();
            [
                "test", "check", "lint", "typecheck", "vitest", "jest", "cargo", "pnpm", "npm",
                "yarn",
            ]
            .iter()
            .any(|needle| {
                lower
                    .split(|character: char| !character.is_alphanumeric())
                    .any(|word| word == *needle)
            })
        })
        .take(6)
        .cloned()
        .collect::<Vec<_>>();
    lines.push(String::new());
    lines.push(if validation_like.is_empty() {
        "Validation-like commands: none observed".to_string()
    } else {
        format!("Validation-like commands: {}", validation_like.join("; "))
    });
    lines.join("\n")
}

// ---------------------------------------------------------------------------
// Prompt
// ---------------------------------------------------------------------------

fn change_list(label: &str, changes: &[WorkspaceGitChangeEntry]) -> Vec<String> {
    if changes.is_empty() {
        return vec![format!("{label}: none")];
    }
    let mut lines = vec![format!("{label}:")];
    lines.extend(changes.iter().take(MAX_CONTEXT_CHANGES).map(|change| {
        format!(
            "- {} {} (+{}/-{})",
            change.status, change.path, change.insertions, change.deletions
        )
    }));
    if changes.len() > MAX_CONTEXT_CHANGES {
        lines.push(format!(
            "- ... {} more file(s)",
            changes.len() - MAX_CONTEXT_CHANGES
        ));
    }
    lines
}

pub struct DelegationPromptContext<'a> {
    pub mode: &'a DelegationMode,
    pub instruction: &'a str,
    pub workspace_name: &'a str,
    pub workspace_branch: &'a str,
    pub workspace_path: &'a str,
    pub parent_session_id: &'a str,
    pub parent_title: &'a str,
    pub git_context: Option<Vec<String>>,
}

pub fn build_delegation_prompt(context: &DelegationPromptContext<'_>) -> String {
    let implementation = matches!(context.mode, DelegationMode::Implement);
    let mut lines = vec![
        format!(
            "Delegated {} task from Dev Command Center.",
            mode_label(context.mode)
        ),
        String::new(),
        "Scope:".to_string(),
    ];
    if implementation {
        lines.extend(
            [
                "- File edits are allowed for this delegated implementation.",
                "- You are in an isolated git worktree created from the parent's current HEAD. Treat it as the checkpoint baseline.",
                "- Inspect the current git status before editing and avoid overwriting unrelated work.",
                "- Do not commit, push, delete branches, reset history, or run destructive commands.",
                "- Run focused validation where practical and report the exact commands/results.",
                "- Stop after implementation; Dev Command Center will require human review before the edits reach the parent.",
            ]
            .map(str::to_string),
        );
    } else {
        lines.extend(
            [
                "- Work read-only. Do not edit files, run destructive commands, or apply patches.",
                "- Return concise findings, risks, and recommended next steps.",
            ]
            .map(str::to_string),
        );
    }
    lines.extend([
        "- Your final message is handed back to the requester as the result: make it self-contained."
            .to_string(),
        String::new(),
        "Workspace:".to_string(),
        format!("- Name: {}", context.workspace_name),
        format!(
            "- Branch: {}",
            if context.workspace_branch.is_empty() {
                "unknown"
            } else {
                context.workspace_branch
            }
        ),
        format!("- Path: {}", context.workspace_path),
        format!(
            "- Parent session: {} ({})",
            context.parent_title, context.parent_session_id
        ),
        String::new(),
        "Instruction:".to_string(),
        context.instruction.trim().to_string(),
    ]);
    if let Some(git) = context.git_context.as_ref() {
        lines.push(String::new());
        lines.push("Git context:".to_string());
        lines.extend(git.iter().cloned());
    }
    lines.join("\n")
}

async fn git_context_lines(workspace: &WorkspaceCommandState, path: &str) -> Vec<String> {
    let status = match workspace_git_status_inner(path) {
        Ok(status) => status,
        Err(error) => return vec![format!("- unavailable: {error}")],
    };
    let branch_diff = workspace_git_branch_diff_with_state(
        workspace,
        WorkspaceGitBranchDiffInput {
            workspace_root: path.to_string(),
        },
    )
    .await
    .ok();
    let mut lines = vec![
        format!(
            "- Current branch: {}",
            status.current_branch.as_deref().unwrap_or("unknown")
        ),
        format!(
            "- Base branch: {}",
            branch_diff
                .as_ref()
                .and_then(|diff| diff.base_branch.as_deref())
                .unwrap_or("unknown")
        ),
        format!("- Conflicts: {}", status.conflict_count),
    ];
    lines.extend(change_list("Staged changes", &status.staged));
    lines.extend(change_list("Unstaged changes", &status.unstaged));
    lines.extend(change_list(
        "Branch diff",
        branch_diff
            .as_ref()
            .map(|diff| diff.changes.as_slice())
            .unwrap_or(&[]),
    ));
    lines
}

// ---------------------------------------------------------------------------
// Launch
// ---------------------------------------------------------------------------

fn resolve_target_provider(
    state: &SessionCommandState,
    requested: Option<&str>,
    parent_provider: &str,
    mode: &DelegationMode,
    budget: &DelegationBudget,
) -> Result<String, String> {
    if let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) {
        state
            .validate_delegation_target(
                &dcc_core::domain::provider::ProviderId(requested.to_string()),
                mode,
                budget,
            )
            .map_err(|error| error.to_string())?;
        return Ok(requested.to_string());
    }
    let capable = |provider: &str| {
        state
            .validate_delegation_target(
                &dcc_core::domain::provider::ProviderId(provider.to_string()),
                mode,
                budget,
            )
            .is_ok()
    };
    dcc_providers::PROVIDER_IDS
        .iter()
        .find(|provider| **provider != parent_provider && capable(provider))
        .or_else(|| {
            dcc_providers::PROVIDER_IDS
                .iter()
                .find(|provider| capable(provider))
        })
        .map(|provider| provider.to_string())
        .ok_or_else(|| "no enabled provider can take this delegation".to_string())
}

async fn remove_delegation_worktree(
    state: &SessionCommandState,
    delegation_id: Option<&DelegationId>,
    operation_id: Option<&str>,
) {
    let operation = match (delegation_id, operation_id) {
        (Some(delegation_id), _) => {
            DelegationWorktreeOperationRepo::get_delegation_worktree_operation_by_delegation_id(
                state,
                delegation_id,
            )
            .await
        }
        (None, Some(operation_id)) => {
            DelegationWorktreeOperationRepo::get_delegation_worktree_operation(
                state,
                &dcc_core::domain::delegation_worktree::DelegationWorktreeOperationId(
                    operation_id.to_string(),
                ),
            )
            .await
        }
        (None, None) => return,
    };
    let Ok(Some(operation)) = operation else {
        return;
    };
    let input = WorkspaceRemoveDelegationWorktreeInput {
        workspace_root: operation.source_root.clone(),
        delegation_id: if operation_id.is_none() {
            delegation_id.cloned()
        } else {
            None
        },
        operation_id: operation_id.map(str::to_string),
        remove_branch: true,
    };
    if let Err(error) = workspace_remove_delegation_worktree_with_state(
        &WorkspaceCommandState::from_session(state),
        input,
    )
    .await
    {
        eprintln!("[DCC] delegation worktree cleanup failed: {error}");
    }
}

/// Starts a delegation: isolated worktree (implement), child thread, durable
/// record, then the child's turn in the background. Returns once the record is
/// `running`, so tool callers get a task id immediately.
pub async fn run_delegation(
    state: &SessionCommandState,
    input: RunDelegationInput,
) -> Result<RunDelegationOutput, String> {
    let instruction = input.instruction.trim().to_string();
    if instruction.is_empty() && input.prebuilt_prompt.is_none() {
        return Err("the delegated task cannot be empty".to_string());
    }
    let parent = SessionRepo::get_session(state, &input.parent_session_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("parent session not found: {}", input.parent_session_id.0))?;

    if input.origin == DelegationOrigin::Agent {
        // A delegated agent works on a bounded task; letting it fan out again
        // turns one request into an unbounded tree of paid runs.
        if DelegationRepo::get_delegation_by_child_session(state, &parent.id)
            .await
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Err("a delegated agent cannot delegate further; finish your task and report back".to_string());
        }
        let active = DelegationRepo::list_delegations(state, None, Some(&parent.id))
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|delegation| {
                delegation.origin == DelegationOrigin::Agent
                    && matches!(
                        delegation.status,
                        DelegationStatus::Draft | DelegationStatus::Queued | DelegationStatus::Running
                    )
            })
            .count();
        if active >= MAX_ACTIVE_AGENT_DELEGATIONS_PER_PARENT {
            return Err(format!(
                "{active} delegations are already running for this session; wait for their results before delegating more"
            ));
        }
    }

    let workspace = WorkspaceRepo::get_workspace(state, &parent.workspace_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("workspace not found: {}", parent.workspace_id.0))?;
    let workspace_path = parent
        .working_directory_override
        .clone()
        .or_else(|| workspace.worktree_path.clone())
        .unwrap_or_else(|| workspace.root_path.clone());

    let allow_file_edits = matches!(input.mode, DelegationMode::Implement);
    let budget = DelegationBudget {
        turn_limit: Some(1),
        timeout_seconds: Some(if allow_file_edits {
            IMPLEMENT_TIMEOUT_SECONDS
        } else {
            READ_ONLY_TIMEOUT_SECONDS
        }),
        allow_file_edits,
    };
    let target_provider_id = resolve_target_provider(
        state,
        input.target_provider_id.as_deref(),
        &parent.provider_id,
        &input.mode,
        &budget,
    )?;

    let sessions = state
        .list_workspace_sessions(&parent.workspace_id)
        .map_err(|error| error.to_string())?;
    let parent_title = sessions
        .iter()
        .find(|summary| summary.session.id == parent.id)
        .map(|summary| summary.thread.title.clone())
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| workspace.name.clone().unwrap_or_else(|| "Untitled".to_string()));
    let provider_runtime = input.provider_runtime.clone().or_else(|| {
        sessions
            .iter()
            .filter(|summary| summary.session.provider_id == target_provider_id)
            .max_by(|left, right| left.session.updated_at.cmp(&right.session.updated_at))
            .and_then(|summary| summary.session.provider_runtime.clone())
    });

    let workspace_state = WorkspaceCommandState::from_session(state);
    let prepared = if allow_file_edits {
        Some(
            workspace_prepare_delegation_worktree_with_state(
                &workspace_state,
                WorkspacePrepareDelegationWorktreeInput {
                    workspace_root: workspace_path.clone(),
                    workspace_id: parent.workspace_id.clone(),
                    parent_session_id: parent.id.clone(),
                    delegation_key: Some(Uuid::new_v4().simple().to_string()),
                },
            )
            .await?,
        )
    } else {
        None
    };
    let operation_id = prepared.as_ref().map(|prepared| prepared.operation_id.clone());

    let launched = async {
        let prompt = match input.prebuilt_prompt.clone() {
            Some(prompt) if !prompt.trim().is_empty() => prompt,
            _ => {
                let git_context = match input.context_policy {
                    DelegationContextPolicy::ReviewCurrentDiff
                    | DelegationContextPolicy::FullReanchor => {
                        Some(git_context_lines(&workspace_state, &workspace_path).await)
                    }
                    _ => None,
                };
                build_delegation_prompt(&DelegationPromptContext {
                    mode: &input.mode,
                    instruction: &instruction,
                    workspace_name: workspace.name.as_deref().unwrap_or("workspace"),
                    workspace_branch: &workspace.base_branch,
                    workspace_path: &workspace_path,
                    parent_session_id: &parent.id.0,
                    parent_title: &parent_title,
                    git_context,
                })
            }
        };

        let start_input = StartThreadInput {
            workspace_id: parent.workspace_id.clone(),
            additional_workspace_ids: Vec::new(),
            project_id: parent.project_id.clone(),
            provider_id: target_provider_id.clone(),
            model: input.target_model_id.clone(),
            provider_runtime: provider_runtime.clone(),
            working_directory_override: prepared
                .as_ref()
                .map(|prepared| prepared.worktree_path.clone()),
            title: Some(format!(
                "Delegated {}: {}",
                mode_label(&input.mode),
                parent_title
            )),
            forked_from: None,
        };
        state
            .validate_start_thread_scope(&start_input)
            .await
            .map_err(|error| error.to_string())?;
        let started = run_start_thread(state, state, state, state, start_input)
            .await
            .map_err(|error| error.to_string())?;
        let child_session_id = started.session.id.clone();

        let parent_turn_id = SessionEventRepo::list_events_by_session(state, &parent.id)
            .await
            .ok()
            .and_then(|events| SessionProjection::fold(&events))
            .and_then(|projection| projection.active_turn_id);
        let created = create_delegation_with_state(
            state,
            CreateDelegationInput {
                parent_session_id: parent.id.clone(),
                parent_turn_id,
                child_session_id: Some(child_session_id.clone()),
                delegation_worktree_operation_id: operation_id.clone(),
                workspace_id: parent.workspace_id.clone(),
                target_provider_id: dcc_core::domain::provider::ProviderId(
                    target_provider_id.clone(),
                ),
                target_model_id: input.target_model_id.clone(),
                mode: input.mode.clone(),
                prompt: prompt.clone(),
                context_policy: input.context_policy.clone(),
                budget: budget.clone(),
                origin: input.origin,
                instruction: Some(if instruction.is_empty() {
                    extract_delegation_instruction(&prompt)
                } else {
                    instruction.clone()
                }),
            },
        )
        .await?
        .delegation;
        let running = match start_delegation_with_state(
            state,
            StartDelegationInput {
                delegation_id: created.id.clone(),
            },
        )
        .await
        {
            Ok(output) => output.delegation,
            Err(error) => {
                let _ = fail_delegation_with_state(
                    state,
                    FailDelegationInput {
                        delegation_id: created.id.clone(),
                        reason: Some(error.clone()),
                    },
                )
                .await;
                return Err(error);
            }
        };
        Ok::<_, String>((running, child_session_id, prompt))
    }
    .await;

    let (running, child_session_id, prompt) = match launched {
        Ok(launched) => launched,
        Err(error) => {
            remove_delegation_worktree(state, None, operation_id.as_deref()).await;
            return Err(error);
        }
    };

    // Attaching a provider and starting its turn can take seconds (process
    // spawn, MCP handshakes). The record is already durable and `running`, so
    // finish in the background and let the watchdog own any loss.
    let background = state.clone();
    let delegation_id = running.id.clone();
    let effort = input.effort.clone().or_else(|| Some("medium".to_string()));
    let fast_mode = input.fast_mode.or(Some(false));
    tokio::spawn(async move {
        let turn = SendTurnInput {
            session_id: child_session_id,
            prompt,
            tool_instructions: None,
            provider_id: None,
            model: None,
            provider_runtime: None,
            plan_mode: Some(false),
            effort,
            fast_mode,
            approval_policy: Some(dcc_core::domain::provider::ProviderApprovalPolicy::Auto),
            evidence: None,
            retry_of_turn_id: None,
            decision_provider_model_route: None,
        };
        if let Err(error) = send_turn_with_state(&background, turn).await {
            eprintln!("[DCC] delegated turn failed to start: {error}");
            fail_and_hand_back(&background, &delegation_id, &error).await;
        }
    });

    Ok(RunDelegationOutput { delegation: running })
}

// ---------------------------------------------------------------------------
// Finalization
// ---------------------------------------------------------------------------

fn finalizing() -> &'static Mutex<HashSet<String>> {
    static FINALIZING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    FINALIZING.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Exactly one finalizer per delegation; the turn hook and the watchdog race.
struct FinalizeGuard(String);

impl FinalizeGuard {
    fn acquire(delegation_id: &DelegationId) -> Option<Self> {
        let mut set = finalizing().lock().ok()?;
        set.insert(delegation_id.0.clone())
            .then(|| Self(delegation_id.0.clone()))
    }
}

impl Drop for FinalizeGuard {
    fn drop(&mut self) {
        if let Ok(mut set) = finalizing().lock() {
            set.remove(&self.0);
        }
    }
}

async fn fail_and_hand_back(state: &SessionCommandState, delegation_id: &DelegationId, reason: &str) {
    let Some(_guard) = FinalizeGuard::acquire(delegation_id) else {
        return;
    };
    let Ok(Some(delegation)) = DelegationRepo::get_delegation(state, delegation_id).await else {
        return;
    };
    if delegation.status != DelegationStatus::Running {
        return;
    }
    finish_failed(state, delegation, reason).await;
}

async fn finish_failed(state: &SessionCommandState, delegation: Delegation, reason: &str) {
    let failed = match fail_delegation_with_state(
        state,
        FailDelegationInput {
            delegation_id: delegation.id.clone(),
            reason: Some(reason.to_string()),
        },
    )
    .await
    {
        Ok(output) => output.delegation,
        Err(error) => {
            eprintln!("[DCC] delegation failure persistence failed: {error}");
            return;
        }
    };
    if is_edit_capable(&failed) {
        remove_delegation_worktree(state, Some(&failed.id), None).await;
    }
    if failed.origin == DelegationOrigin::Agent {
        hand_back_to_parent(
            state,
            &failed.parent_session_id,
            delegation_result_turn(&failed, Some(reason)),
        )
        .await;
    }
}

/// Schedules [`on_session_turn_terminal`] when a tokio runtime is available
/// (some synchronous test paths terminalize turns outside one).
pub fn spawn_on_session_turn_terminal(state: &SessionCommandState, session_id: &SessionId) {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(on_session_turn_terminal(state.clone(), session_id.clone()));
    }
}

/// Called after any durable terminal event of `session_id`'s turn.
pub async fn on_session_turn_terminal(state: SessionCommandState, session_id: SessionId) {
    let Ok(Some(delegation)) =
        DelegationRepo::get_delegation_by_child_session(&state, &session_id).await
    else {
        return;
    };
    if delegation.status == DelegationStatus::Running {
        finalize_running_delegation(&state, delegation).await;
    }
}

enum ChildOutcome {
    Running(TurnId),
    Completed,
    Aborted(Option<String>),
    NotStarted,
}

fn child_outcome(events: &[dcc_core::domain::session::SessionEventRecord]) -> ChildOutcome {
    if let Some(active) = SessionProjection::fold(events).and_then(|projection| projection.active_turn_id) {
        return ChildOutcome::Running(active);
    }
    for event in events.iter().rev() {
        match &event.kind {
            SessionEventKind::TurnCompleted { .. } => return ChildOutcome::Completed,
            SessionEventKind::TurnAborted { reason, .. } => {
                return ChildOutcome::Aborted(reason.clone())
            }
            SessionEventKind::TurnStarted { .. } => return ChildOutcome::NotStarted,
            _ => {}
        }
    }
    ChildOutcome::NotStarted
}

async fn finalize_running_delegation(state: &SessionCommandState, delegation: Delegation) {
    let Some(_guard) = FinalizeGuard::acquire(&delegation.id) else {
        return;
    };
    // Re-read under the guard: another finalizer may have just finished.
    let delegation = match DelegationRepo::get_delegation(state, &delegation.id).await {
        Ok(Some(current)) if current.status == DelegationStatus::Running => current,
        _ => return,
    };
    let Some(child_session_id) = delegation.child_session_id.clone() else {
        finish_failed(state, delegation, "The delegated session no longer exists.").await;
        return;
    };
    let events = match SessionEventRepo::list_events_by_session(state, &child_session_id).await {
        Ok(events) => events,
        Err(error) => {
            eprintln!("[DCC] delegation child history unavailable: {error}");
            return;
        }
    };
    match child_outcome(&events) {
        ChildOutcome::Running(_) | ChildOutcome::NotStarted => {}
        ChildOutcome::Aborted(reason) => {
            let reason = reason
                .filter(|reason| !reason.trim().is_empty())
                .unwrap_or_else(|| "The delegated turn was interrupted.".to_string());
            finish_failed(state, delegation, &reason).await;
        }
        ChildOutcome::Completed => {
            let summary = summarize_child_turn(&events);
            let mut touched = Vec::<String>::new();
            let (mut additions, mut deletions) = (0u64, 0u64);
            if let Ok(change_sets) = state.list_turn_change_sets(&child_session_id) {
                for file in change_sets.iter().flat_map(|change_set| change_set.files.iter()) {
                    if !touched.contains(&file.path) {
                        touched.push(file.path.clone());
                    }
                    additions += u64::from(file.insertions);
                    deletions += u64::from(file.deletions);
                }
            }
            touched.sort();
            let diff_summary = if touched.is_empty() {
                "No changed files detected.".to_string()
            } else {
                format!("{} file(s), +{additions}/-{deletions}", touched.len())
            };
            let completed = match complete_delegation_with_state(
                state,
                CompleteDelegationInput {
                    delegation_id: delegation.id.clone(),
                    summary: Some(clip(
                        summary
                            .assistant_text
                            .as_deref()
                            .unwrap_or("Delegated session completed without assistant text."),
                        MAX_SUMMARY_CHARS,
                    )),
                    touched_files: touched,
                    diff_summary: Some(diff_summary),
                    validation_summary: Some(validation_summary(&summary.commands)),
                    review_required: is_edit_capable(&delegation),
                },
            )
            .await
            {
                Ok(output) => output.delegation,
                Err(error) => {
                    eprintln!("[DCC] delegation completion failed: {error}");
                    return;
                }
            };
            if completed.origin == DelegationOrigin::Agent {
                hand_back_to_parent(
                    state,
                    &completed.parent_session_id,
                    delegation_result_turn(&completed, None),
                )
                .await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Hand-back
// ---------------------------------------------------------------------------

fn hand_back_turn(parent_session_id: &SessionId, prompt: String) -> SendTurnInput {
    SendTurnInput {
        session_id: parent_session_id.clone(),
        prompt,
        tool_instructions: None,
        provider_id: None,
        model: None,
        provider_runtime: None,
        plan_mode: None,
        effort: None,
        fast_mode: None,
        approval_policy: None,
        evidence: None,
        retry_of_turn_id: None,
        decision_provider_model_route: None,
    }
}

/// Wakes the parent agent. With an open turn the message is queued and the
/// backend dispatches it right after that turn; an idle parent gets it as a
/// new turn. If starting fails (provider gone, app closing), the message stays
/// queued so the person can send it from the composer.
pub async fn hand_back_to_parent(
    state: &SessionCommandState,
    parent_session_id: &SessionId,
    prompt: String,
) {
    let parent_busy = SessionEventRepo::list_events_by_session(state, parent_session_id)
        .await
        .ok()
        .and_then(|events| SessionProjection::fold(&events))
        .and_then(|projection| projection.active_turn_id)
        .is_some();
    let queue = |prompt: String| async move {
        run_queue_turn(
            state,
            state,
            QueueTurnInput {
                turn: hand_back_turn(parent_session_id, prompt),
            },
        )
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
    };
    let result = if parent_busy {
        queue(prompt).await
    } else {
        match send_turn_with_state(state, hand_back_turn(parent_session_id, prompt.clone())).await
        {
            Ok(_) => Ok(()),
            Err(error) => {
                eprintln!("[DCC] delegation hand-back could not start a turn: {error}");
                queue(prompt).await
            }
        }
    };
    if let Err(error) = result {
        eprintln!("[DCC] delegation hand-back failed: {error}");
    }
}

/// After the person applies (`applied`) or discards reviewed edits.
pub fn notify_parent_of_review_outcome(
    state: &SessionCommandState,
    delegation: &Delegation,
    applied: bool,
) {
    if delegation.origin != DelegationOrigin::Agent {
        return;
    }
    let state = state.clone();
    let parent_session_id = delegation.parent_session_id.clone();
    let prompt = delegation_review_outcome_turn(delegation, applied);
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            hand_back_to_parent(&state, &parent_session_id, prompt).await;
        });
    }
}

// ---------------------------------------------------------------------------
// Watchdog
// ---------------------------------------------------------------------------

fn timed_out(delegation: &Delegation, now: DateTime<Utc>) -> bool {
    let timeout = delegation
        .budget
        .timeout_seconds
        .unwrap_or(DEFAULT_TIMEOUT_SECONDS);
    delegation
        .started_at
        .as_deref()
        .and_then(|started| DateTime::parse_from_rfc3339(started).ok())
        .is_some_and(|started| {
            (now - started.with_timezone(&Utc)).num_seconds() > i64::try_from(timeout).unwrap_or(i64::MAX)
        })
}

fn start_grace_elapsed(delegation: &Delegation, now: DateTime<Utc>) -> bool {
    delegation
        .started_at
        .as_deref()
        .or(Some(delegation.updated_at.as_str()))
        .and_then(|started| DateTime::parse_from_rfc3339(started).ok())
        .is_some_and(|started| {
            (now - started.with_timezone(&Utc)).num_seconds() > CHILD_START_GRACE_SECONDS
        })
}

/// Settles every `running` delegation against its child's real state:
/// finishes ones whose turn ended while nobody was listening (reload, app
/// restart), aborts children past their timeout budget, and fails children
/// that never started.
pub async fn reconcile_running_delegations(state: &SessionCommandState) {
    let Ok(running) = DelegationRepo::list_delegations_by_status(state, DelegationStatus::Running).await
    else {
        return;
    };
    let now = Utc::now();
    for delegation in running {
        let Some(child_session_id) = delegation.child_session_id.clone() else {
            fail_and_hand_back(state, &delegation.id, "The delegated session no longer exists.").await;
            continue;
        };
        let Ok(events) = SessionEventRepo::list_events_by_session(state, &child_session_id).await
        else {
            continue;
        };
        match child_outcome(&events) {
            ChildOutcome::Running(turn_id) => {
                if timed_out(&delegation, now) {
                    let reason = format!(
                        "Delegation timed out after {} seconds.",
                        delegation
                            .budget
                            .timeout_seconds
                            .unwrap_or(DEFAULT_TIMEOUT_SECONDS)
                    );
                    // The abort lands as a durable TurnAborted, whose hook
                    // fails the delegation and hands the reason back.
                    if let Err(error) = state
                        .quiesce_turn_for_abort(&child_session_id, &turn_id, Some(&reason))
                        .await
                    {
                        eprintln!("[DCC] delegation timeout abort failed: {error}");
                        fail_and_hand_back(state, &delegation.id, &reason).await;
                    }
                }
            }
            ChildOutcome::Completed | ChildOutcome::Aborted(_) => {
                finalize_running_delegation(state, delegation).await;
            }
            ChildOutcome::NotStarted => {
                if start_grace_elapsed(&delegation, now) {
                    fail_and_hand_back(
                        state,
                        &delegation.id,
                        "The delegated agent never started its turn.",
                    )
                    .await;
                }
            }
        }
    }
}

/// Runs [`reconcile_running_delegations`] now and then every 30 seconds.
/// Spawn it once at startup on the app's async runtime.
pub async fn run_delegation_watchdog(state: SessionCommandState) {
    loop {
        reconcile_running_delegations(&state).await;
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    }
}

/// Agent-scoped status for the `dcc_task_status` tool.
pub async fn delegation_for_parent(
    state: &SessionCommandState,
    parent_session_id: &SessionId,
    delegation_id: &str,
) -> Result<Delegation, String> {
    let delegation = DelegationRepo::get_delegation(state, &DelegationId(delegation_id.trim().to_string()))
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("task not found: {delegation_id}"))?;
    if delegation.parent_session_id != *parent_session_id {
        return Err(format!("task not found: {delegation_id}"));
    }
    Ok(delegation)
}

/// Cancels a running delegation on behalf of its parent agent.
pub async fn cancel_delegation_for_parent(
    state: &SessionCommandState,
    parent_session_id: &SessionId,
    delegation_id: &str,
    reason: Option<String>,
) -> Result<Delegation, String> {
    let delegation = delegation_for_parent(state, parent_session_id, delegation_id).await?;
    if !matches!(
        delegation.status,
        DelegationStatus::Draft | DelegationStatus::Queued | DelegationStatus::Running
    ) {
        return Ok(delegation);
    }
    let _guard = FinalizeGuard::acquire(&delegation.id)
        .ok_or_else(|| "the task is finishing right now; check its status".to_string())?;
    let reason = reason
        .map(|reason| clip(&reason, 2_000))
        .filter(|reason| !reason.is_empty())
        .unwrap_or_else(|| "Cancelled by the requesting agent.".to_string());
    let cancelled = crate::commands::delegation_commands::cancel_delegation_with_state(
        state,
        crate::commands::delegation_commands::CancelDelegationInput {
            delegation_id: delegation.id.clone(),
            reason: Some(reason.clone()),
        },
    )
    .await?
    .delegation;
    if let Some(child_session_id) = cancelled.child_session_id.as_ref() {
        if let Ok(events) = SessionEventRepo::list_events_by_session(state, child_session_id).await {
            if let ChildOutcome::Running(turn_id) = child_outcome(&events) {
                if let Err(error) = state
                    .quiesce_turn_for_abort(child_session_id, &turn_id, Some(&reason))
                    .await
                {
                    eprintln!("[DCC] cancelled delegation child abort failed: {error}");
                }
            }
        }
    }
    if is_edit_capable(&cancelled) {
        remove_delegation_worktree(state, Some(&cancelled.id), None).await;
    }
    Ok(cancelled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::session::SessionEventRecord;

    fn delegation(status: DelegationStatus) -> Delegation {
        Delegation {
            id: DelegationId("d-1".to_string()),
            parent_session_id: SessionId("parent".to_string()),
            parent_turn_id: None,
            child_session_id: Some(SessionId("child".to_string())),
            workspace_id: dcc_core::domain::workspace::WorkspaceId("w".to_string()),
            target_provider_id: dcc_core::domain::provider::ProviderId("codex".to_string()),
            target_model_id: Some("gpt-5.6".to_string()),
            mode: DelegationMode::Review,
            status,
            prompt: "Delegated review task\n\nInstruction:\nReview the diff.\n\nGit context:\n- x"
                .to_string(),
            context_policy: DelegationContextPolicy::Minimal,
            budget: DelegationBudget::default(),
            result_summary: Some("Looks good.".to_string()),
            touched_files: Vec::new(),
            diff_summary: None,
            validation_summary: Some("Validation-like commands: yarn test".to_string()),
            created_at: "2026-10-05T10:00:00Z".to_string(),
            updated_at: "2026-10-05T10:00:00Z".to_string(),
            origin: DelegationOrigin::Agent,
            instruction: None,
            started_at: Some("2026-10-05T10:00:00Z".to_string()),
        }
    }

    fn event(sequence: u64, kind: SessionEventKind) -> SessionEventRecord {
        SessionEventRecord {
            event_id: format!("e-{sequence}"),
            session_id: SessionId("child".to_string()),
            sequence,
            occurred_at: "2026-10-05T10:00:00Z".to_string(),
            kind,
        }
    }

    #[test]
    fn result_turn_carries_task_result_and_validation() {
        let text = delegation_result_turn(&delegation(DelegationStatus::Completed), None);
        assert!(text.starts_with("[DCC] Delegated review task finished — Codex (gpt-5.6) (task d-1)."));
        assert!(text.contains("Task you delegated:\nReview the diff."));
        assert!(text.contains("Result:\nLooks good."));
        assert!(text.contains("Validation:\nValidation-like commands: yarn test"));
        assert!(!text.contains("NOT applied"));
    }

    #[test]
    fn review_pending_result_says_edits_are_not_applied() {
        let mut pending = delegation(DelegationStatus::ReviewPending);
        pending.mode = DelegationMode::Implement;
        pending.touched_files = (0..22).map(|index| format!("src/f{index}.rs")).collect();
        let text = delegation_result_turn(&pending, None);
        assert!(text.contains("Files touched (22):"));
        assert!(text.contains("- … 2 more"));
        assert!(text.contains("NOT applied to your workspace yet"));
    }

    #[test]
    fn failed_result_reports_reason_without_result_block() {
        let text = delegation_result_turn(&delegation(DelegationStatus::Failed), Some("Timed out."));
        assert!(text.contains("task failed"));
        assert!(text.contains("Failure: Timed out."));
        assert!(!text.contains("Result:"));
    }

    #[test]
    fn review_outcome_turns_name_the_decision() {
        let mut applied = delegation(DelegationStatus::Completed);
        applied.touched_files = vec!["src/a.rs".to_string()];
        assert!(delegation_review_outcome_turn(&applied, true).contains("APPLIED"));
        assert!(delegation_review_outcome_turn(&applied, true).contains("- src/a.rs"));
        assert!(delegation_review_outcome_turn(&applied, false).contains("DISCARDED"));
    }

    #[test]
    fn instruction_is_read_from_the_prompt_when_not_stored() {
        assert_eq!(
            extract_delegation_instruction(&delegation(DelegationStatus::Running).prompt),
            "Review the diff."
        );
        assert_eq!(extract_delegation_instruction("  plain task "), "plain task");
    }

    #[test]
    fn child_summary_prefers_the_final_answer_and_dedupes_commands() {
        let turn = TurnId("t-1".to_string());
        let events = vec![
            event(
                1,
                SessionEventKind::TurnStarted {
                    turn_id: turn.clone(),
                    prompt: "do it".to_string(),
                    plan_mode: None,
                    model: None,
                    evidence: None,
                    retry_of_turn_id: None,
                },
            ),
            event(
                2,
                SessionEventKind::TurnToolCallStarted {
                    turn_id: turn.clone(),
                    tool_call_id: "c1".to_string(),
                    action: "bash".to_string(),
                    command: Some("yarn test".to_string()),
                    file: None,
                },
            ),
            event(
                3,
                SessionEventKind::TurnToolCallStarted {
                    turn_id: turn.clone(),
                    tool_call_id: "c2".to_string(),
                    action: "bash".to_string(),
                    command: Some("yarn test".to_string()),
                    file: None,
                },
            ),
            event(
                4,
                SessionEventKind::TurnAssistantMessageCompleted {
                    turn_id: turn.clone(),
                    message_id: "m1".to_string(),
                    phase: AssistantMessagePhase::Commentary,
                    content: Some("Looking…".to_string()),
                },
            ),
            event(
                5,
                SessionEventKind::TurnAssistantMessageCompleted {
                    turn_id: turn.clone(),
                    message_id: "m2".to_string(),
                    phase: AssistantMessagePhase::FinalAnswer,
                    content: Some("All good.".to_string()),
                },
            ),
        ];
        let summary = summarize_child_turn(&events);
        assert_eq!(summary.assistant_text.as_deref(), Some("All good."));
        assert_eq!(summary.commands, vec!["yarn test".to_string()]);
        assert!(validation_summary(&summary.commands).contains("Validation-like commands: yarn test"));
    }

    #[test]
    fn timeout_counts_from_started_at() {
        let running = delegation(DelegationStatus::Running);
        let started = DateTime::parse_from_rfc3339("2026-10-05T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert!(!timed_out(&running, started + chrono::Duration::seconds(599)));
        assert!(timed_out(&running, started + chrono::Duration::seconds(601)));
    }

    mod lifecycle {
        use super::*;
        use dcc_core::domain::{
            project::ProjectId,
            session::{Session, SessionEventRecord, SessionState},
            workspace::{Workspace, WorkspaceId, WorkspaceState},
        };
        use dcc_infra::db::SqliteWorkspaceRepo;

        struct Fixture {
            _root: tempfile::TempDir,
            state: SessionCommandState,
        }

        fn session(id: &str, provider_id: &str) -> Session {
            Session {
                id: SessionId(id.to_string()),
                project_id: ProjectId("project".to_string()),
                workspace_id: WorkspaceId("workspace".to_string()),
                additional_workspace_ids: Vec::new(),
                provider_id: provider_id.to_string(),
                model: None,
                provider_runtime: None,
                working_directory_override: None,
                state: SessionState::Active,
                created_at: "2026-10-05T10:00:00Z".to_string(),
                updated_at: "2026-10-05T10:00:00Z".to_string(),
            }
        }

        async fn fixture() -> Fixture {
            let root = tempfile::tempdir().expect("state root");
            let physical = std::fs::canonicalize(root.path()).expect("physical root");
            let state = SessionCommandState::new_headless(
                physical.join("state.sqlite"),
                physical.join("app-data"),
            );
            SqliteWorkspaceRepo::open(physical.join("state.sqlite"))
                .expect("workspace repo")
                .save_workspace(&Workspace {
                    id: WorkspaceId("workspace".to_string()),
                    project_id: ProjectId("project".to_string()),
                    name: None,
                    root_path: "/workspace".to_string(),
                    base_branch: "main".to_string(),
                    worktree_path: None,
                    source: None,
                    state: WorkspaceState::Ready,
                    setup_report: None,
                    pinned_at: None,
                    created_at: "2026-10-05T10:00:00Z".to_string(),
                    updated_at: "2026-10-05T10:00:00Z".to_string(),
                })
                .await
                .expect("save workspace");
            for (id, provider) in [("parent", "codex"), ("child", "claude_code")] {
                SessionRepo::save_session(&state, &session(id, provider))
                    .await
                    .expect("save session");
            }
            Fixture { _root: root, state }
        }

        async fn append(state: &SessionCommandState, session_id: &str, kind: SessionEventKind) {
            let session_id = SessionId(session_id.to_string());
            let sequence = SessionEventRepo::list_events_by_session(state, &session_id)
                .await
                .expect("history")
                .last()
                .map(|event| event.sequence + 1)
                .unwrap_or(1);
            SessionEventRepo::append_event(
                state,
                &SessionEventRecord {
                    event_id: Uuid::new_v4().to_string(),
                    session_id,
                    sequence,
                    occurred_at: Utc::now().to_rfc3339(),
                    kind,
                },
            )
            .await
            .expect("append");
        }

        fn started(turn: &str, prompt: &str) -> SessionEventKind {
            SessionEventKind::TurnStarted {
                turn_id: TurnId(turn.to_string()),
                prompt: prompt.to_string(),
                plan_mode: None,
                model: None,
                evidence: None,
                retry_of_turn_id: None,
            }
        }

        async fn running_delegation(state: &SessionCommandState, origin: DelegationOrigin) -> Delegation {
            let mut delegation = delegation(DelegationStatus::Running);
            // The finalize guard is process-wide; parallel tests need distinct ids.
            delegation.id = DelegationId(Uuid::new_v4().to_string());
            delegation.workspace_id = WorkspaceId("workspace".to_string());
            delegation.target_provider_id =
                dcc_core::domain::provider::ProviderId("claude_code".to_string());
            delegation.target_model_id = None;
            delegation.result_summary = None;
            delegation.validation_summary = None;
            delegation.origin = origin;
            delegation.instruction = Some("Review the sidebar diff.".to_string());
            delegation.started_at = Some(Utc::now().to_rfc3339());
            DelegationRepo::save_delegation(state, &delegation)
                .await
                .expect("save delegation");
            delegation
        }

        async fn queued_prompts(state: &SessionCommandState, session_id: &str) -> Vec<String> {
            SessionEventRepo::list_events_by_session(state, &SessionId(session_id.to_string()))
                .await
                .expect("history")
                .into_iter()
                .filter_map(|event| match event.kind {
                    SessionEventKind::TurnQueued { queued_turn } => Some(queued_turn.prompt),
                    _ => None,
                })
                .collect()
        }

        #[tokio::test(flavor = "current_thread")]
        async fn finished_child_completes_the_delegation_and_wakes_a_busy_parent() {
            let fixture = fixture().await;
            let state = &fixture.state;
            let delegation = running_delegation(state, DelegationOrigin::Agent).await;
            append(state, "parent", started("p-1", "work")).await;
            append(state, "child", started("c-1", "review")).await;
            append(
                state,
                "child",
                SessionEventKind::TurnAssistantMessageCompleted {
                    turn_id: TurnId("c-1".to_string()),
                    message_id: "m".to_string(),
                    phase: AssistantMessagePhase::FinalAnswer,
                    content: Some("Found 2 issues.".to_string()),
                },
            )
            .await;
            append(
                state,
                "child",
                SessionEventKind::TurnCompleted {
                    turn_id: TurnId("c-1".to_string()),
                },
            )
            .await;

            on_session_turn_terminal(state.clone(), SessionId("child".to_string())).await;

            let stored = DelegationRepo::get_delegation(state, &delegation.id)
                .await
                .expect("load")
                .expect("delegation");
            assert_eq!(stored.status, DelegationStatus::Completed);
            assert_eq!(stored.result_summary.as_deref(), Some("Found 2 issues."));
            let queued = queued_prompts(state, "parent").await;
            assert_eq!(queued.len(), 1);
            assert!(queued[0].starts_with("[DCC] Delegated review task finished — Claude Code"));
            assert!(queued[0].contains("Review the sidebar diff."));
            assert!(queued[0].contains("Found 2 issues."));

            // A second terminal signal (watchdog, duplicate hook) is a no-op.
            on_session_turn_terminal(state.clone(), SessionId("child".to_string())).await;
            reconcile_running_delegations(state).await;
            assert_eq!(queued_prompts(state, "parent").await.len(), 1);
        }

        #[tokio::test(flavor = "current_thread")]
        async fn watchdog_fails_an_interrupted_child_and_reports_the_reason() {
            let fixture = fixture().await;
            let state = &fixture.state;
            let delegation = running_delegation(state, DelegationOrigin::Agent).await;
            append(state, "parent", started("p-1", "work")).await;
            append(state, "child", started("c-1", "review")).await;
            append(
                state,
                "child",
                SessionEventKind::TurnAborted {
                    turn_id: TurnId("c-1".to_string()),
                    reason: Some("Provider crashed.".to_string()),
                },
            )
            .await;

            reconcile_running_delegations(state).await;

            let stored = DelegationRepo::get_delegation(state, &delegation.id)
                .await
                .expect("load")
                .expect("delegation");
            assert_eq!(stored.status, DelegationStatus::Failed);
            let queued = queued_prompts(state, "parent").await;
            assert_eq!(queued.len(), 1);
            assert!(queued[0].contains("Failure: Provider crashed."));
        }

        #[tokio::test(flavor = "current_thread")]
        async fn person_started_delegations_are_not_handed_back() {
            let fixture = fixture().await;
            let state = &fixture.state;
            let delegation = running_delegation(state, DelegationOrigin::Person).await;
            append(state, "parent", started("p-1", "work")).await;
            append(state, "child", started("c-1", "review")).await;
            append(
                state,
                "child",
                SessionEventKind::TurnCompleted {
                    turn_id: TurnId("c-1".to_string()),
                },
            )
            .await;

            on_session_turn_terminal(state.clone(), SessionId("child".to_string())).await;

            assert_eq!(
                DelegationRepo::get_delegation(state, &delegation.id)
                    .await
                    .expect("load")
                    .expect("delegation")
                    .status,
                DelegationStatus::Completed
            );
            assert!(queued_prompts(state, "parent").await.is_empty());
        }

        #[tokio::test(flavor = "current_thread")]
        async fn a_delegated_agent_cannot_delegate_again() {
            let fixture = fixture().await;
            let state = &fixture.state;
            running_delegation(state, DelegationOrigin::Agent).await;
            let error = run_delegation(
                state,
                RunDelegationInput {
                    parent_session_id: SessionId("child".to_string()),
                    target_provider_id: None,
                    target_model_id: None,
                    mode: DelegationMode::Review,
                    instruction: "fan out".to_string(),
                    context_policy: DelegationContextPolicy::Minimal,
                    origin: DelegationOrigin::Agent,
                    prebuilt_prompt: None,
                    effort: None,
                    fast_mode: None,
                    provider_runtime: None,
                },
            )
            .await
            .expect_err("nested delegation must be refused");
            assert!(error.contains("cannot delegate further"));
        }

        #[tokio::test(flavor = "current_thread")]
        async fn agent_task_tools_only_see_their_own_delegations() {
            let fixture = fixture().await;
            let state = &fixture.state;
            let delegation = running_delegation(state, DelegationOrigin::Agent).await;
            assert!(delegation_for_parent(state, &SessionId("parent".to_string()), &delegation.id.0)
                .await
                .is_ok());
            assert!(delegation_for_parent(state, &SessionId("child".to_string()), &delegation.id.0)
                .await
                .is_err());
        }
    }
}
