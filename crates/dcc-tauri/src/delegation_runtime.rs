//! Backend-owned delegation lifecycle.
//!
//! A delegation runs as a child thread on (usually) another provider. Everything
//! that must survive a webview reload lives here: launching the child, noticing
//! when its turn ends, recording the result, handing it back to the parent agent
//! that asked for it, enforcing the timeout budget, and telling the parent when
//! the person applies or discards delegated edits.

use std::{
    collections::{HashMap, HashSet},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use dcc_core::{
    application::{
        active_turn_for_steer, queue_turn as run_queue_turn, record_turn_steer,
        start_thread as run_start_thread, QueueTurnInput, SendTurnInput, StartThreadInput,
        SteerTurnInput,
    },
    domain::{
        delegation::{
            Delegation, DelegationBudget, DelegationContextPolicy, DelegationId, DelegationKind,
            DelegationMode, DelegationOrigin, DelegationStatus,
        },
        provider::ProviderApprovalPolicy,
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
            cancel_delegation_with_state, complete_delegation_with_state,
            create_delegation_with_state, create_task_proposal_with_state,
            fail_delegation_with_state, start_delegation_with_state, CancelDelegationInput,
            CompleteDelegationInput, CreateDelegationInput, CreateTaskProposalInput,
            FailDelegationInput, StartDelegationInput,
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
use dcc_infra::db::SqliteWorkspaceRepo;

/// Provider ids a delegation can target (validated against availability at run time).
pub use dcc_providers::PROVIDER_IDS as DELEGATION_PROVIDER_IDS;

/// Agent-initiated delegations a single parent may have in flight at once.
pub const MAX_ACTIVE_AGENT_DELEGATIONS_PER_PARENT: usize = 4;
/// Agent-initiated delegations one parent turn may start, finished or not;
/// delegate → wait → delegate inside a turn never trips the in-flight cap.
pub const MAX_AGENT_DELEGATIONS_PER_TURN: usize = 6;
/// Every turn DCC itself sends to an agent (hand-backs, review outcomes)
/// starts with this, so a turn it started is never mistaken for the person's.
const DCC_TURN_PREFIX: &str = "[DCC] ";
/// Task proposals one parent turn may add; past that a card is noise.
pub const MAX_TASK_PROPOSALS_PER_TURN: usize = 3;
/// A proposal nobody started within this window is withdrawn.
const TASK_PROPOSAL_TTL_DAYS: i64 = 7;
/// How much of a task's last answer wakes the parent: enough to decide the
/// next step; the person reads the rest in the task itself.
const TASK_HAND_BACK_CHARS: usize = 400;
/// Opens every task's first message; its turn is the brief, which the parent
/// is waiting to hear back from.
const TASK_BRIEF_HEADER: &str = "Task from Dev Command Center";
/// Tasks listed in a status, newest first.
const MAX_LISTED_TASKS: usize = 8;
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
    /// A short name for the task, shown to the person as the child
    /// conversation's title. Omitted: derived from the instruction.
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub kind: DelegationKind,
    /// Starts this waiting task proposal instead of a new record.
    #[serde(default)]
    pub proposal_id: Option<DelegationId>,
    /// A child that edits works straight in the parent's checkout, so its
    /// edits are there when it reports back: no worktree, no review step.
    #[serde(default)]
    pub shared_checkout: bool,
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

/// The parent's child that is editing its checkout right now. Children that
/// share the checkout take turns: two writing the same files at once is how
/// edits get lost. An open task only counts while a turn of it runs.
async fn child_editing_the_checkout(
    state: &SessionCommandState,
    parent_session_id: &SessionId,
    except: Option<&DelegationId>,
) -> Result<Option<Delegation>, String> {
    let children = DelegationRepo::list_delegations(state, None, Some(parent_session_id))
        .await
        .map_err(|error| error.to_string())?;
    for child in children {
        if except == Some(&child.id)
            || !is_edit_capable(&child)
            || !child.budget.shared_checkout
            || !matches!(
                child.status,
                DelegationStatus::Queued | DelegationStatus::Running
            )
        {
            continue;
        }
        if child.kind == DelegationKind::Task {
            let Some(child_session_id) = child.child_session_id.as_ref() else {
                continue;
            };
            let events = SessionEventRepo::list_events_by_session(state, child_session_id)
                .await
                .map_err(|error| error.to_string())?;
            if SessionProjection::fold(&events)
                .and_then(|projection| projection.active_turn_id)
                .is_none()
            {
                continue;
            }
        }
        return Ok(Some(child));
    }
    Ok(None)
}

fn busy_checkout_error(child: &Delegation) -> String {
    format!(
        "\"{}\" is still editing this checkout; only one child edits at a time. Wait for it to end its turn, then try again",
        task_title(child)
    )
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

const MAX_CHILD_TITLE_CHARS: usize = 80;

/// The child conversation's title: the agent's own name for the task, else
/// the instruction's first line, else the parent's title. The lineage already
/// marks it as delegated, so no "Delegated …" prefix eats the space.
fn child_thread_title(title: Option<&str>, instruction: &str, parent_title: &str) -> String {
    let candidate = title
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            instruction
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
        })
        .unwrap_or(parent_title);
    let mut chars = candidate.chars();
    let clipped: String = chars.by_ref().take(MAX_CHILD_TITLE_CHARS).collect();
    if chars.next().is_some() {
        format!("{}…", clipped.trim_end())
    } else {
        clipped
    }
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
    if delegation.budget.shared_checkout && !delegation.touched_files.is_empty() {
        lines.extend([
            String::new(),
            "These edits are already in your workspace — do not redo them.".to_string(),
        ]);
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
                let command = command.as_deref().filter(|value| !value.trim().is_empty());
                // Claude streams Bash input after the start; the command
                // arrives in TurnToolCallUpdated instead of a bare "Bash".
                if command.is_none() && action == "Bash" {
                    continue;
                }
                let value = command.unwrap_or(action.as_str()).trim().to_string();
                if !value.is_empty() && !commands.contains(&value) {
                    commands.push(value);
                }
            }
            SessionEventKind::TurnToolCallUpdated {
                turn_id: id,
                detail,
                ..
            } if *id == turn_id => {
                if let Some(value) = detail
                    .command
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    if !commands.iter().any(|existing| existing == value) {
                        commands.push(value.to_string());
                    }
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
    /// The provider itself blocks or gates writes (see [`writes_guarded`]).
    pub writes_guarded: bool,
    /// Edits land straight in the parent's checkout instead of a worktree.
    pub shared_checkout: bool,
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
    if implementation && context.shared_checkout {
        lines.extend(
            [
                "- File edits are allowed for this delegated implementation.",
                "- You edit the parent's checkout directly: your changes are in its workspace as you make them.",
                "- The parent may work in the same files; check git status first and keep to your task's files.",
                "- Do not commit, push, delete branches, reset history, or run destructive commands.",
                "- Run focused validation where practical and report the exact commands/results.",
            ]
            .map(str::to_string),
        );
    } else if implementation {
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
        if !context.writes_guarded {
            lines.push(
                "- DCC cannot sandbox this provider, so read-only is on you: do not modify anything."
                    .to_string(),
            );
        }
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
// Permission ceiling
// ---------------------------------------------------------------------------

/// Levels the provider enforces natively; empty when it takes none or is unknown.
pub fn provider_approval_policies(provider_id: &str) -> Vec<ProviderApprovalPolicy> {
    dcc_providers::provider_registration(provider_id)
        .map(|registration| registration.capabilities.approval_policies)
        .unwrap_or_default()
}

/// The permission level of the parent's newest turn that recorded one.
pub fn latest_turn_approval_policy(
    events: &[dcc_core::domain::session::SessionEventRecord],
) -> Option<ProviderApprovalPolicy> {
    events.iter().rev().find_map(|event| match &event.kind {
        SessionEventKind::TurnStarted {
            approval_policy: Some(policy),
            ..
        } => Some(*policy),
        _ => None,
    })
}

/// What a task that edits may do: exactly what its parent may. The person
/// started it from the parent, so the delegation ceiling (auto at most) does
/// not apply — under it, every write outside the task's folder stopped to ask
/// while the parent ran with full access.
pub fn task_approval_policy(
    supported: &[ProviderApprovalPolicy],
    parent: Option<ProviderApprovalPolicy>,
) -> Option<ProviderApprovalPolicy> {
    match parent {
        Some(policy) if supported.contains(&policy) => Some(policy),
        _ => child_approval_policy(supported, &DelegationMode::Implement, parent),
    }
}

/// What a delegated child may do, mirroring T3 Code's rule that a child never
/// runs with broader permissions than its parent, plus a mode ceiling:
/// review/explain are read-only, implement is auto inside its isolated
/// worktree. When the target cannot honour the level, the next more
/// restrictive one it supports wins (read-only → ask, so a write needs a
/// human). `None` means the provider takes no policy at all.
pub fn child_approval_policy(
    supported: &[ProviderApprovalPolicy],
    mode: &DelegationMode,
    parent: Option<ProviderApprovalPolicy>,
) -> Option<ProviderApprovalPolicy> {
    let mode_ceiling = if matches!(mode, DelegationMode::Implement) {
        ProviderApprovalPolicy::Auto
    } else {
        ProviderApprovalPolicy::ReadOnly
    };
    let wanted = parent.map_or(mode_ceiling, |parent| parent.most_restrictive(mode_ceiling));
    if supported.contains(&wanted) {
        return Some(wanted);
    }
    // Prefer the closest more restrictive level; never widen past `wanted`
    // unless the provider offers nothing at or below it.
    let mut ordered = supported.to_vec();
    ordered.sort_by_key(|policy| policy.rank());
    ordered
        .iter()
        .copied()
        .filter(|policy| policy.rank() <= wanted.rank())
        .max_by_key(|policy| policy.rank())
        .or_else(|| {
            ordered
                .iter()
                .copied()
                .find(|policy| *policy == ProviderApprovalPolicy::Ask)
        })
}

/// Whether the provider (not just the prompt) keeps a read-only child from
/// writing: it either cannot write or must ask the person first.
pub fn writes_guarded(delegation: &Delegation) -> bool {
    matches!(
        delegation.budget.approval_policy,
        Some(ProviderApprovalPolicy::ReadOnly | ProviderApprovalPolicy::Ask)
    )
}

// ---------------------------------------------------------------------------
// Launch
// ---------------------------------------------------------------------------

/// The open turn and the prompt that started it. Steering adds guidance to a
/// turn without changing who started it.
fn open_turn_prompt(
    events: &[dcc_core::domain::session::SessionEventRecord],
) -> Option<(TurnId, String)> {
    let active = SessionProjection::fold(events)?.active_turn_id?;
    events.iter().rev().find_map(|event| match &event.kind {
        SessionEventKind::TurnStarted { turn_id, prompt, .. } if *turn_id == active => {
            Some((active.clone(), prompt.clone()))
        }
        _ => None,
    })
}

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
    // A read-only task prefers a provider that can guarantee it.
    let enforces_read_only = |provider: &str| {
        budget.allow_file_edits
            || provider_approval_policies(provider).contains(&ProviderApprovalPolicy::ReadOnly)
    };
    dcc_providers::PROVIDER_IDS
        .iter()
        .find(|provider| {
            **provider != parent_provider && capable(provider) && enforces_read_only(provider)
        })
        .or_else(|| {
            dcc_providers::PROVIDER_IDS
                .iter()
                .find(|provider| **provider != parent_provider && capable(provider))
        })
        .or_else(|| {
            dcc_providers::PROVIDER_IDS
                .iter()
                .find(|provider| capable(provider))
        })
        .map(|provider| provider.to_string())
        .ok_or_else(|| "no enabled provider can take this delegation".to_string())
}

/// Best effort: a failure is logged and the journal keeps the worktree for a
/// later cleanup. Without `discard_changes`, unapplied uncommitted edits stay.
pub(crate) async fn remove_delegation_worktree(
    state: &SessionCommandState,
    delegation_id: Option<&DelegationId>,
    operation_id: Option<&str>,
    discard_changes: bool,
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
        discard_changes,
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

    // A started proposal is the person's go-ahead, not the agent fanning out.
    if input.origin == DelegationOrigin::Agent && input.proposal_id.is_none() {
        // A delegated agent works on a bounded task; letting it fan out again
        // turns one request into an unbounded tree of paid runs.
        if DelegationRepo::get_delegation_by_child_session(state, &parent.id)
            .await
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Err("a delegated agent cannot delegate further; finish your task and report back".to_string());
        }
        let parent_events = SessionEventRepo::list_events_by_session(state, &parent.id)
            .await
            .map_err(|error| error.to_string())?;
        let open_turn = open_turn_prompt(&parent_events);
        // A turn DCC started (a result coming back) is not the person asking
        // for more work; delegating from it chains paid runs nobody requested.
        if open_turn
            .as_ref()
            .is_some_and(|(_, prompt)| prompt.starts_with(DCC_TURN_PREFIX))
        {
            return Err("this turn was started by DCC handing you a result, not by the human; do not delegate again on your own — report what you have and, if more work is needed, suggest it to the human so they can ask for it".to_string());
        }
        let agent_delegations: Vec<Delegation> =
            DelegationRepo::list_delegations(state, None, Some(&parent.id))
                .await
                .map_err(|error| error.to_string())?
                .into_iter()
                .filter(|delegation| {
                    delegation.origin == DelegationOrigin::Agent
                        && delegation.kind == DelegationKind::Delegation
                })
                .collect();
        let active = agent_delegations
            .iter()
            .filter(|delegation| {
                matches!(
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
        if let Some((turn_id, _)) = &open_turn {
            let this_turn = agent_delegations
                .iter()
                .filter(|delegation| delegation.parent_turn_id.as_ref() == Some(turn_id))
                .count();
            if this_turn >= MAX_AGENT_DELEGATIONS_PER_TURN {
                return Err(format!(
                    "you already delegated {this_turn} tasks in this turn; finish with what you have and let the human decide on more"
                ));
            }
        }
    }

    // SessionCommandState's WorkspaceRepo impl is a stub that never finds a
    // workspace; the durable workspace store lives in its own repository.
    let workspace_repo =
        SqliteWorkspaceRepo::open(state.db_path()).map_err(|error| error.to_string())?;
    let workspace = WorkspaceRepo::get_workspace(&workspace_repo, &parent.workspace_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("workspace not found: {}", parent.workspace_id.0))?;
    let workspace_path = parent
        .working_directory_override
        .clone()
        .or_else(|| workspace.worktree_path.clone())
        .unwrap_or_else(|| workspace.root_path.clone());

    let allow_file_edits = matches!(input.mode, DelegationMode::Implement);
    let is_task = input.kind == DelegationKind::Task;
    let mut budget = DelegationBudget {
        // A task is a conversation of its own: no turn count, no clock.
        turn_limit: (!is_task).then_some(1),
        timeout_seconds: (!is_task).then_some(if allow_file_edits {
            IMPLEMENT_TIMEOUT_SECONDS
        } else {
            READ_ONLY_TIMEOUT_SECONDS
        }),
        allow_file_edits,
        approval_policy: None,
        shared_checkout: allow_file_edits && input.shared_checkout,
    };
    if budget.shared_checkout {
        if let Some(busy) =
            child_editing_the_checkout(state, &parent.id, input.proposal_id.as_ref()).await?
        {
            return Err(busy_checkout_error(&busy));
        }
    }
    let target_provider_id = resolve_target_provider(
        state,
        input.target_provider_id.as_deref(),
        &parent.provider_id,
        &input.mode,
        &budget,
    )?;
    let parent_policy = SessionEventRepo::list_events_by_session(state, &parent.id)
        .await
        .ok()
        .and_then(|events| latest_turn_approval_policy(&events));
    budget.approval_policy = if is_task && allow_file_edits {
        task_approval_policy(&provider_approval_policies(&target_provider_id), parent_policy)
    } else {
        child_approval_policy(
            &provider_approval_policies(&target_provider_id),
            &input.mode,
            parent_policy,
        )
    };

    let sessions = state
        .list_workspace_sessions(&parent.workspace_id)
        .map_err(|error| error.to_string())?;
    let parent_title = sessions
        .iter()
        .find(|summary| summary.session.id == parent.id)
        .map(|summary| summary.thread.title.clone())
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| workspace.name.clone().unwrap_or_else(|| "Untitled".to_string()));
    // A child on the parent's provider runs as the parent's account.
    let parent_runtime = (parent.provider_id == target_provider_id)
        .then(|| parent.provider_runtime.clone())
        .flatten();
    let provider_runtime = input.provider_runtime.clone().or(parent_runtime).or_else(|| {
        sessions
            .iter()
            .filter(|summary| summary.session.provider_id == target_provider_id)
            .max_by(|left, right| left.session.updated_at.cmp(&right.session.updated_at))
            .and_then(|summary| summary.session.provider_runtime.clone())
    });

    let workspace_state = WorkspaceCommandState::from_session(state);
    let prepared = if allow_file_edits && !budget.shared_checkout {
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
            _ if is_task => build_task_prompt(&TaskPromptContext {
                instruction: &instruction,
                workspace_name: workspace.name.as_deref().unwrap_or("workspace"),
                workspace_branch: &workspace.base_branch,
                workspace_path: &workspace_path,
                parent_title: &parent_title,
                scope: match (allow_file_edits, budget.shared_checkout) {
                    (false, _) => TaskScope::ReadOnly,
                    (true, false) => TaskScope::OwnWorktree,
                    (true, true) => TaskScope::SharedCheckout,
                },
            }),
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
                    writes_guarded: matches!(
                        budget.approval_policy,
                        Some(ProviderApprovalPolicy::ReadOnly | ProviderApprovalPolicy::Ask)
                    ),
                    shared_checkout: budget.shared_checkout,
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
            title: Some(child_thread_title(
                input.title.as_deref(),
                &instruction,
                &parent_title,
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
                kind: input.kind,
                title: input.title.clone(),
                proposal_id: input.proposal_id.clone(),
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
            remove_delegation_worktree(state, None, operation_id.as_deref(), true).await;
            return Err(error);
        }
    };

    // Attaching a provider and starting its turn can take seconds (process
    // spawn, MCP handshakes). The record is already durable and `running`, so
    // finish in the background and let the watchdog own any loss.
    let background = state.clone();
    let delegation_id = running.id.clone();
    let effort = input.effort.clone().or_else(|| Some("medium".to_string()));
    let approval_policy = running.budget.approval_policy;
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
            approval_policy,
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
        remove_delegation_worktree(state, Some(&failed.id), None, true).await;
    }
    if failed.origin == DelegationOrigin::Agent {
        deliver_result(state, &failed, delegation_result_turn(&failed, Some(reason))).await;
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
        match delegation.kind {
            DelegationKind::Task => report_task_turn(&state, delegation).await,
            DelegationKind::Delegation => finalize_running_delegation(&state, delegation).await,
        }
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
                deliver_result(state, &completed, delegation_result_turn(&completed, None)).await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Task proposals (`dcc_propose_task`)
// ---------------------------------------------------------------------------

pub struct TaskPromptContext<'a> {
    pub instruction: &'a str,
    pub workspace_name: &'a str,
    pub workspace_branch: &'a str,
    pub workspace_path: &'a str,
    pub parent_title: &'a str,
    pub scope: TaskScope,
}

/// Where a task may write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskScope {
    ReadOnly,
    OwnWorktree,
    SharedCheckout,
}

/// The first message of a task: the brief, framed as a conversation the
/// person can keep talking to, not a one-shot report.
pub fn build_task_prompt(context: &TaskPromptContext<'_>) -> String {
    let mut lines = vec![
        format!(
            "{TASK_BRIEF_HEADER}, proposed in \"{}\" and started by the human.",
            context.parent_title
        ),
        String::new(),
        "Scope:".to_string(),
    ];
    match context.scope {
        TaskScope::OwnWorktree => lines.extend(
            [
                "- You are in an isolated git worktree created from the parent's current HEAD; edit freely there.",
                "- Do not commit, push, delete branches, reset history, or run destructive commands.",
                "- The human reviews your edits before they reach the parent workspace.",
            ]
            .map(str::to_string),
        ),
        TaskScope::SharedCheckout => lines.extend(
            [
                "- You edit the parent's checkout directly: your changes land in the human's workspace as you make them.",
                "- The parent session may work in the same files; check git status first and keep to your task's files.",
                "- Do not commit, push, delete branches, reset history, or run destructive commands.",
            ]
            .map(str::to_string),
        ),
        TaskScope::ReadOnly => {
            lines.push("- Work read-only. Do not edit files or run destructive commands.".to_string())
        }
    }
    lines.extend([
        "- This is your own conversation: the human may follow up here.".to_string(),
        "- When you end a turn, DCC hands its first lines back to the parent session, so open your final message with the outcome.".to_string(),
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
        String::new(),
        "Task:".to_string(),
        context.instruction.trim().to_string(),
    ]);
    lines.join("\n")
}

/// What wakes the parent when a task ends a turn: a short status, not the
/// whole report — the parent coordinates, the task holds the detail. `tasks`
/// is every task of the parent, so it sees where the whole plan stands.
pub fn task_result_turn(
    delegation: &Delegation,
    interrupted: Option<&str>,
    tasks: &[Delegation],
) -> String {
    let title = task_title(delegation);
    let mut lines = vec![
        format!(
            "[DCC] Task \"{}\" ended a turn — {} (task {}).",
            title,
            agent_label(delegation),
            delegation.id.0
        ),
        String::new(),
    ];
    match interrupted {
        Some(reason) => lines.push(format!(
            "The turn was interrupted: {}",
            clip(reason, TASK_HAND_BACK_CHARS)
        )),
        None => lines.push(clip(
            delegation
                .result_summary
                .as_deref()
                .filter(|summary| !summary.trim().is_empty())
                .unwrap_or("The task returned no text."),
            TASK_HAND_BACK_CHARS,
        )),
    }
    if !delegation.touched_files.is_empty() {
        lines.extend([
            String::new(),
            if delegation.budget.shared_checkout {
                format!(
                    "It has changed {} file(s) directly in your checkout; they are already in your workspace — do not redo them.",
                    delegation.touched_files.len()
                )
            } else {
                format!(
                    "It has changed {} file(s) in its own worktree so far; the human reviews them when the task is closed — do not re-implement them.",
                    delegation.touched_files.len()
                )
            },
        ]);
    }
    let listed: Vec<String> = tasks
        .iter()
        .filter(|task| task.kind == DelegationKind::Task && task.status != DelegationStatus::Cancelled)
        .take(MAX_LISTED_TASKS)
        .map(|task| {
            format!(
                "- {} — {} (task {})",
                task_title(task),
                task_status_label(&task.status),
                task.id.0
            )
        })
        .collect();
    if !listed.is_empty() {
        lines.push(String::new());
        lines.push("Your tasks:".to_string());
        lines.extend(listed);
    }
    lines.extend([
        String::new(),
        "The human can open the task to read the rest or keep talking to it. Tell them in a few lines where things stand, then stop: this message cannot propose or start tasks. If the task found problems, list them; reviewing, fixing and re-checking its work is yours to do in this conversation once the human replies.".to_string(),
    ]);
    lines.join("\n")
}

fn task_title(task: &Delegation) -> &str {
    task.title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or("Untitled task")
}

fn task_status_label(status: &DelegationStatus) -> &'static str {
    match status {
        DelegationStatus::Draft => "proposed, waiting for the human to start it",
        DelegationStatus::Queued | DelegationStatus::Running => "open",
        DelegationStatus::ReviewPending => "closed, edits awaiting the human's review",
        DelegationStatus::Completed => "closed",
        DelegationStatus::Failed => "failed",
        DelegationStatus::Cancelled => "dismissed",
    }
}

/// The turn that ended last in a child's history, and the abort reason when
/// it was interrupted.
fn last_ended_turn(
    events: &[dcc_core::domain::session::SessionEventRecord],
) -> Option<(TurnId, Option<String>)> {
    events.iter().rev().find_map(|event| match &event.kind {
        SessionEventKind::TurnCompleted { turn_id } => Some((turn_id.clone(), None)),
        SessionEventKind::TurnAborted { turn_id, reason } => Some((
            turn_id.clone(),
            Some(
                reason
                    .clone()
                    .filter(|reason| !reason.trim().is_empty())
                    .unwrap_or_else(|| "stopped".to_string()),
            ),
        )),
        _ => None,
    })
}

/// A task's child ended a turn. The task stays open; its record takes the
/// latest answer and changed files. The parent is woken only for turns it is
/// waiting on — the brief and its own follow-ups — never for turns the
/// person drove inside the task: they are already there.
async fn report_task_turn(state: &SessionCommandState, task: Delegation) {
    let Some(_guard) = FinalizeGuard::acquire(&task.id) else {
        return;
    };
    let mut task = match DelegationRepo::get_delegation(state, &task.id).await {
        Ok(Some(current)) if current.status == DelegationStatus::Running => current,
        _ => return,
    };
    let Some(child_session_id) = task.child_session_id.clone() else {
        return;
    };
    let Ok(events) = SessionEventRepo::list_events_by_session(state, &child_session_id).await
    else {
        return;
    };
    if SessionProjection::fold(&events).and_then(|projection| projection.active_turn_id).is_some() {
        return;
    }
    let Some((turn_id, interrupted)) = last_ended_turn(&events) else {
        return;
    };
    if task.reported_turn_id.as_ref() == Some(&turn_id) {
        return;
    }
    let turn_prompt = events.iter().find_map(|event| match &event.kind {
        SessionEventKind::TurnStarted { turn_id: id, prompt, .. } if *id == turn_id => {
            Some(prompt.as_str())
        }
        _ => None,
    });
    let wakes_parent = turn_prompt.is_some_and(|prompt| {
        prompt.starts_with(TASK_BRIEF_HEADER) || prompt.starts_with(DCC_TURN_PREFIX)
    });

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
    if interrupted.is_none() {
        if let Some(text) = summarize_child_turn(&events)
            .assistant_text
            .filter(|text| !text.trim().is_empty())
        {
            task.result_summary = Some(clip(&text, MAX_SUMMARY_CHARS));
        }
    }
    task.diff_summary = (!touched.is_empty())
        .then(|| format!("{} file(s), +{additions}/-{deletions}", touched.len()));
    task.touched_files = touched;
    task.reported_turn_id = Some(turn_id);
    task.updated_at = Utc::now().to_rfc3339();
    if let Err(error) = DelegationRepo::save_delegation(state, &task).await {
        eprintln!("[DCC] task turn report failed: {error}");
        return;
    }
    if !wakes_parent || task.origin != DelegationOrigin::Agent {
        return;
    }
    let tasks = DelegationRepo::list_delegations(state, None, Some(&task.parent_session_id))
        .await
        .unwrap_or_default();
    let prompt = task_result_turn(&task, interrupted.as_deref(), &tasks);
    deliver_result(state, &task, prompt).await;
}

/// The person's "Close task": the task stops being open. Its edits go to
/// review like a delegation's; nothing wakes the parent, which hears about
/// the review outcome as usual.
pub async fn complete_task(
    state: &SessionCommandState,
    task_id: &DelegationId,
) -> Result<Delegation, String> {
    let _guard = FinalizeGuard::acquire(task_id)
        .ok_or_else(|| "the task is reporting a turn right now; try again".to_string())?;
    let task = DelegationRepo::get_delegation(state, task_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("task not found: {}", task_id.0))?;
    if task.kind != DelegationKind::Task || task.status != DelegationStatus::Running {
        return Err("only an open task can be closed".to_string());
    }
    if let Some(child_session_id) = task.child_session_id.as_ref() {
        let events = SessionEventRepo::list_events_by_session(state, child_session_id)
            .await
            .map_err(|error| error.to_string())?;
        if SessionProjection::fold(&events).and_then(|projection| projection.active_turn_id).is_some() {
            return Err("the task is still working; wait for its turn to end or stop it first".to_string());
        }
    }
    complete_delegation_with_state(
        state,
        CompleteDelegationInput {
            delegation_id: task.id.clone(),
            summary: task.result_summary.clone(),
            touched_files: task.touched_files.clone(),
            diff_summary: task.diff_summary.clone(),
            validation_summary: None,
            review_required: is_edit_capable(&task) && !task.budget.shared_checkout,
        },
    )
    .await
    .map(|output| output.delegation)
}

/// `dcc_task_message`: the parent's follow-up to one of its open tasks. It
/// becomes a turn of its own in the task, so its end reports back.
pub async fn message_task(
    state: &SessionCommandState,
    parent_session_id: &SessionId,
    task_id: &str,
    message: &str,
) -> Result<Delegation, String> {
    let message = message.trim();
    if message.is_empty() {
        return Err("the message cannot be empty".to_string());
    }
    let task = delegation_for_parent(state, parent_session_id, task_id).await?;
    if task.kind != DelegationKind::Task || task.status != DelegationStatus::Running {
        return Err("only an open task takes messages; propose a new task instead".to_string());
    }
    if is_edit_capable(&task) && task.budget.shared_checkout {
        if let Some(busy) =
            child_editing_the_checkout(state, parent_session_id, Some(&task.id)).await?
        {
            return Err(busy_checkout_error(&busy));
        }
    }
    let parent_events = SessionEventRepo::list_events_by_session(state, parent_session_id)
        .await
        .map_err(|error| error.to_string())?;
    if !open_turn_prompt(&parent_events)
        .is_some_and(|(_, prompt)| !prompt.starts_with(DCC_TURN_PREFIX))
    {
        return Err("messages to a task go out from a turn the human started; tell them what you would send".to_string());
    }
    let child_session_id = task
        .child_session_id
        .clone()
        .ok_or_else(|| "the task has no conversation".to_string())?;
    let prompt = format!("{DCC_TURN_PREFIX}Message from the session that coordinates this task:\n\n{message}");
    let child_busy = SessionEventRepo::list_events_by_session(state, &child_session_id)
        .await
        .ok()
        .and_then(|events| SessionProjection::fold(&events))
        .and_then(|projection| projection.active_turn_id)
        .is_some();
    // Queued rather than steered: the message needs a turn of its own so
    // that turn's end is reported back.
    let mut turn = hand_back_turn(&child_session_id, prompt);
    turn.approval_policy = task.budget.approval_policy;
    if child_busy {
        run_queue_turn(state, state, QueueTurnInput { turn })
            .await
            .map_err(|error| error.to_string())?;
    } else {
        send_turn_with_state(state, turn).await?;
    }
    Ok(task)
}

#[derive(Clone, Debug)]
pub struct ProposeTaskInput {
    pub parent_session_id: SessionId,
    pub title: String,
    pub instruction: String,
    /// The task edits files, straight in the parent's checkout.
    pub edits: bool,
    /// Omitted: the parent's own provider.
    pub target_provider_id: Option<String>,
    pub target_model_id: Option<String>,
    /// Start without a card; only for a turn the person started.
    pub start_now: bool,
}

/// Records a task the parent agent suggests. It waits as a card in the
/// parent's thread until the person starts or dismisses it, unless the
/// person's own turn asked for it to start right away.
pub async fn propose_task(
    state: &SessionCommandState,
    input: ProposeTaskInput,
) -> Result<Delegation, String> {
    let title = input.title.trim().to_string();
    let instruction = input.instruction.trim().to_string();
    if title.is_empty() || instruction.is_empty() {
        return Err("a task needs a title and a brief".to_string());
    }
    let parent = SessionRepo::get_session(state, &input.parent_session_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("parent session not found: {}", input.parent_session_id.0))?;
    if DelegationRepo::get_delegation_by_child_session(state, &parent.id)
        .await
        .map_err(|error| error.to_string())?
        .is_some()
    {
        return Err("a task or delegated agent cannot propose tasks; report back instead".to_string());
    }
    let events = SessionEventRepo::list_events_by_session(state, &parent.id)
        .await
        .map_err(|error| error.to_string())?;
    let open_turn = open_turn_prompt(&events);
    let person_turn = open_turn
        .as_ref()
        .is_some_and(|(_, prompt)| !prompt.starts_with(DCC_TURN_PREFIX));
    // A [DCC] status turn proposing the next task is how review → fix →
    // review chains form with nobody asking; only the human moves the plan.
    if open_turn.is_some() && !person_turn {
        return Err("a turn started by a [DCC] status cannot propose tasks; tell the human where things stand and stop. Fixing or re-checking a task's work is yours to do in this conversation once they reply".to_string());
    }
    if input.start_now && !person_turn {
        return Err("start=now is only for a turn the human started; propose the task instead so they can start it".to_string());
    }
    let proposals: Vec<Delegation> = DelegationRepo::list_delegations(state, None, Some(&parent.id))
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|delegation| delegation.kind == DelegationKind::Task)
        .collect();
    if let Some((turn_id, _)) = &open_turn {
        let this_turn = proposals
            .iter()
            .filter(|proposal| proposal.parent_turn_id.as_ref() == Some(turn_id))
            .count();
        if this_turn >= MAX_TASK_PROPOSALS_PER_TURN {
            return Err(format!(
                "you already proposed {this_turn} tasks in this turn; let the human pick before proposing more"
            ));
        }
    }
    if proposals.iter().any(|proposal| {
        proposal.status == DelegationStatus::Draft
            && proposal
                .title
                .as_deref()
                .is_some_and(|existing| existing.trim().eq_ignore_ascii_case(&title))
    }) {
        return Err("a proposal with this title is already waiting for the human".to_string());
    }

    let mode = if input.edits {
        DelegationMode::Implement
    } else {
        DelegationMode::Research
    };
    let budget = DelegationBudget {
        turn_limit: None,
        timeout_seconds: None,
        allow_file_edits: input.edits,
        approval_policy: None,
        // Like a Claude Code subagent: the parent hears back with the edits
        // already in its checkout, never through a review of a copy.
        shared_checkout: input.edits,
    };
    let target_provider_id = input
        .target_provider_id
        .map(|provider| provider.trim().to_string())
        .filter(|provider| !provider.is_empty())
        .unwrap_or_else(|| parent.provider_id.clone());
    let target_provider_id = dcc_core::domain::provider::ProviderId(target_provider_id);
    state
        .validate_delegation_target(&target_provider_id, &mode, &budget)
        .map_err(|error| error.to_string())?;
    let proposal = create_task_proposal_with_state(
        state,
        CreateTaskProposalInput {
            parent_session_id: parent.id.clone(),
            parent_turn_id: open_turn.map(|(turn_id, _)| turn_id),
            workspace_id: parent.workspace_id.clone(),
            target_provider_id,
            target_model_id: input
                .target_model_id
                .map(|model| model.trim().to_string())
                .filter(|model| !model.is_empty()),
            mode,
            title,
            instruction,
            budget,
        },
    )
    .await?;
    if input.start_now {
        return start_task_proposal(state, &proposal.id, None).await;
    }
    Ok(proposal)
}

/// Launches a waiting proposal as its own conversation. `instruction` is the
/// brief as the person edited it on the card.
pub async fn start_task_proposal(
    state: &SessionCommandState,
    proposal_id: &DelegationId,
    instruction: Option<String>,
) -> Result<Delegation, String> {
    // Two clicks on Start must not launch two children.
    let _guard = FinalizeGuard::acquire(proposal_id)
        .ok_or_else(|| "this task is already starting".to_string())?;
    let proposal = DelegationRepo::get_delegation(state, proposal_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("task proposal not found: {}", proposal_id.0))?;
    if proposal.kind != DelegationKind::Task || proposal.status != DelegationStatus::Draft {
        return Err("this task proposal was already started or dismissed".to_string());
    }
    let instruction = instruction
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| proposal.instruction.clone())
        .unwrap_or_else(|| proposal.prompt.clone());
    run_delegation(
        state,
        RunDelegationInput {
            parent_session_id: proposal.parent_session_id.clone(),
            target_provider_id: Some(proposal.target_provider_id.0.clone()),
            target_model_id: proposal.target_model_id.clone(),
            mode: proposal.mode.clone(),
            instruction,
            context_policy: DelegationContextPolicy::Minimal,
            origin: DelegationOrigin::Agent,
            prebuilt_prompt: None,
            effort: None,
            fast_mode: None,
            provider_runtime: None,
            title: proposal.title.clone(),
            kind: DelegationKind::Task,
            proposal_id: Some(proposal.id.clone()),
            shared_checkout: proposal.budget.shared_checkout,
        },
    )
    .await
    .map(|output| output.delegation)
}

/// Withdraws proposals nobody started within [`TASK_PROPOSAL_TTL_DAYS`].
async fn expire_task_proposals(state: &SessionCommandState) {
    let Ok(drafts) = DelegationRepo::list_delegations_by_status(state, DelegationStatus::Draft).await
    else {
        return;
    };
    let cutoff = Utc::now() - chrono::Duration::days(TASK_PROPOSAL_TTL_DAYS);
    for proposal in drafts {
        let expired = proposal.kind == DelegationKind::Task
            && DateTime::parse_from_rfc3339(&proposal.created_at)
                .is_ok_and(|created| created.with_timezone(&Utc) < cutoff);
        if !expired {
            continue;
        }
        if let Err(error) = cancel_delegation_with_state(
            state,
            CancelDelegationInput {
                delegation_id: proposal.id.clone(),
                reason: Some("Nobody started this proposal within a week.".to_string()),
            },
        )
        .await
        {
            eprintln!("[DCC] task proposal expiry failed: {error}");
        }
    }
}

// ---------------------------------------------------------------------------
// Waiting (`wait=true`, `dcc_task_wait`)
// ---------------------------------------------------------------------------

/// Longest a tool call may block on a child.
pub const MAX_WAIT_SECONDS: u64 = 600;
pub const DEFAULT_WAIT_SECONDS: u64 = 300;
/// Concurrent blocking waits across the app; each holds an MCP request open.
const MAX_CONCURRENT_WAITS: usize = 4;
/// A finished child whose result was not handed to the waiter within this
/// window (no finalizer ran) is returned from the record instead.
const CLAIM_GRACE: Duration = Duration::from_secs(5);

#[derive(Default)]
struct WaitSlot {
    /// The hand-back the finalizer gave the waiter instead of sending it.
    claimed: Option<String>,
    returned: bool,
}

fn waits() -> &'static Mutex<HashMap<String, WaitSlot>> {
    static WAITS: OnceLock<Mutex<HashMap<String, WaitSlot>>> = OnceLock::new();
    WAITS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A result goes to a waiting tool call when there is one, otherwise back
/// to the parent as a turn. Exactly one of the two delivers it.
async fn deliver_result(state: &SessionCommandState, delegation: &Delegation, prompt: String) {
    let handed_to_waiter = waits()
        .lock()
        .map(|mut waits| match waits.get_mut(&delegation.id.0) {
            Some(slot) => {
                slot.claimed = Some(prompt.clone());
                true
            }
            None => false,
        })
        .unwrap_or(false);
    if !handed_to_waiter {
        hand_back_to_parent(state, &delegation.parent_session_id, prompt).await;
    }
}

/// Registered while a tool call blocks on a delegation. Dropping it (the
/// call returned, timed out, or its HTTP request was cancelled) releases the
/// slot; a result the waiter claimed but never returned is handed back.
struct WaitGuard {
    delegation_id: String,
    state: SessionCommandState,
    parent_session_id: SessionId,
}

impl WaitGuard {
    fn register(
        state: &SessionCommandState,
        delegation: &Delegation,
    ) -> Result<Self, String> {
        let mut waits = waits().lock().map_err(|_| "wait registry unavailable".to_string())?;
        if waits.contains_key(&delegation.id.0) {
            return Err("this task is already being waited on".to_string());
        }
        if waits.len() >= MAX_CONCURRENT_WAITS {
            return Err(
                "too many tasks are being waited on; continue without waiting and DCC will deliver the result"
                    .to_string(),
            );
        }
        waits.insert(delegation.id.0.clone(), WaitSlot::default());
        Ok(Self {
            delegation_id: delegation.id.0.clone(),
            state: state.clone(),
            parent_session_id: delegation.parent_session_id.clone(),
        })
    }

    fn claimed(&self) -> Option<String> {
        waits()
            .lock()
            .ok()
            .and_then(|waits| waits.get(&self.delegation_id).and_then(|slot| slot.claimed.clone()))
    }

    fn mark_returned(&self) {
        if let Ok(mut waits) = waits().lock() {
            if let Some(slot) = waits.get_mut(&self.delegation_id) {
                slot.returned = true;
            }
        }
    }
}

impl Drop for WaitGuard {
    fn drop(&mut self) {
        let orphaned = waits()
            .lock()
            .ok()
            .and_then(|mut waits| waits.remove(&self.delegation_id))
            .and_then(|slot| (!slot.returned).then_some(slot.claimed).flatten());
        if let Some(prompt) = orphaned {
            let state = self.state.clone();
            let parent = self.parent_session_id.clone();
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move { hand_back_to_parent(&state, &parent, prompt).await });
            }
        }
    }
}

#[derive(Debug)]
pub struct WaitOutcome {
    pub delegation: Delegation,
    /// The `[DCC] …` result, when the task finished within the wait.
    pub result: Option<String>,
    pub timed_out: bool,
}

/// Blocks until the parent's delegation finishes or `timeout` elapses. A
/// result returned here is not handed back again; on timeout the child keeps
/// running and its result arrives the usual way.
pub async fn wait_for_delegation(
    state: &SessionCommandState,
    parent_session_id: &SessionId,
    delegation_id: &str,
    timeout: Duration,
) -> Result<WaitOutcome, String> {
    let delegation = delegation_for_parent(state, parent_session_id, delegation_id).await?;
    if delegation.origin != DelegationOrigin::Agent {
        return Err("only delegations you requested can be waited on".to_string());
    }
    let guard = WaitGuard::register(state, &delegation)?;
    let deadline = Instant::now() + timeout.min(Duration::from_secs(MAX_WAIT_SECONDS));
    let mut finished_since: Option<Instant> = None;
    loop {
        let current = delegation_for_parent(state, parent_session_id, delegation_id).await?;
        if let Some(result) = guard.claimed() {
            guard.mark_returned();
            return Ok(WaitOutcome { delegation: current, result: Some(result), timed_out: false });
        }
        match current.status {
            // Cancelled tasks are never handed back; nothing to wait for.
            DelegationStatus::Cancelled => {
                return Ok(WaitOutcome { delegation: current, result: None, timed_out: false });
            }
            DelegationStatus::Completed | DelegationStatus::ReviewPending | DelegationStatus::Failed => {
                let since = *finished_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= CLAIM_GRACE {
                    // Finished before this wait began (already handed back),
                    // so report the record without delivering it twice.
                    guard.mark_returned();
                    return Ok(WaitOutcome { delegation: current, result: None, timed_out: false });
                }
            }
            _ => {}
        }
        if Instant::now() >= deadline {
            return Ok(WaitOutcome { delegation: current, result: None, timed_out: true });
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
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

/// Wakes the parent agent. With an open turn the result is steered into it
/// when the provider supports steering (the agent reacts mid-turn, as in T3
/// Code), else queued and dispatched right after that turn; an idle parent
/// gets it as a new turn. If starting fails (provider gone, app closing), the
/// message stays queued so the person can send it from the composer.
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
        match steer_into_open_turn(state, parent_session_id, &prompt).await {
            Ok(()) => Ok(()),
            Err(reason) => {
                eprintln!("[DCC] delegation hand-back not steered ({reason}); queueing");
                queue(prompt).await
            }
        }
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

/// Injects the hand-back into the parent's running turn and records it as
/// steering guidance, exactly like a person steering from the composer.
async fn steer_into_open_turn(
    state: &SessionCommandState,
    parent_session_id: &SessionId,
    prompt: &str,
) -> Result<(), String> {
    let input = SteerTurnInput {
        session_id: parent_session_id.clone(),
        prompt: prompt.to_string(),
    };
    let (_, turn_id) = active_turn_for_steer(state, state, &input)
        .await
        .map_err(|error| error.to_string())?;
    state
        .steer_provider_turn(parent_session_id, prompt)
        .await
        .map_err(|error| error.to_string())?;
    record_turn_steer(state, state, state, input, turn_id)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
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
    if delegation.kind == DelegationKind::Task {
        return false;
    }
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
    expire_task_proposals(state).await;
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
            ChildOutcome::Completed | ChildOutcome::Aborted(_) => match delegation.kind {
                DelegationKind::Task => report_task_turn(state, delegation).await,
                DelegationKind::Delegation => finalize_running_delegation(state, delegation).await,
            },
            ChildOutcome::NotStarted => {
                // A task that already reported turns is open, not lost.
                if delegation.reported_turn_id.is_none() && start_grace_elapsed(&delegation, now) {
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
        remove_delegation_worktree(state, Some(&cancelled.id), None, true).await;
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
            kind: Default::default(),
            title: None,
            reported_turn_id: None,
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
    fn child_title_prefers_the_agents_name_then_the_instruction() {
        assert_eq!(
            child_thread_title(Some("  Corrigir respostas antigas "), "long task", "Parent"),
            "Corrigir respostas antigas"
        );
        assert_eq!(
            child_thread_title(Some("  "), "\nFix stale replies\nmore detail", "Parent"),
            "Fix stale replies"
        );
        assert_eq!(child_thread_title(None, "", "Parent"), "Parent");
        let long = "x".repeat(MAX_CHILD_TITLE_CHARS + 5);
        assert_eq!(
            child_thread_title(None, &long, "Parent").chars().count(),
            MAX_CHILD_TITLE_CHARS + 1
        );
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
                    approval_policy: None,
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

    #[test]
    fn a_child_never_runs_above_its_parent_or_its_mode() {
        use ProviderApprovalPolicy::*;
        let full = [ReadOnly, Ask, Auto, FullAccess];
        // Read-only modes stay read-only even under a full-access parent.
        assert_eq!(child_approval_policy(&full, &DelegationMode::Review, Some(FullAccess)), Some(ReadOnly));
        assert_eq!(child_approval_policy(&full, &DelegationMode::Explain, None), Some(ReadOnly));
        // Implement tops out at auto in its worktree, and follows a stricter parent.
        assert_eq!(child_approval_policy(&full, &DelegationMode::Implement, Some(FullAccess)), Some(Auto));
        assert_eq!(child_approval_policy(&full, &DelegationMode::Implement, Some(Ask)), Some(Ask));
        assert_eq!(child_approval_policy(&full, &DelegationMode::Implement, None), Some(Auto));
        // Without a read-only sandbox the next stricter level gates writes.
        let no_read_only = [Ask, Auto, FullAccess];
        assert_eq!(child_approval_policy(&no_read_only, &DelegationMode::Review, Some(Auto)), Some(Ask));
        // A provider that takes no policy gets none; the prompt carries the rule.
        assert_eq!(child_approval_policy(&[], &DelegationMode::Review, Some(Auto)), None);
    }

    #[test]
    fn a_task_that_edits_runs_with_its_parents_access() {
        use ProviderApprovalPolicy::*;
        let codex = [ReadOnly, Ask, Auto, FullAccess];
        assert_eq!(task_approval_policy(&codex, Some(FullAccess)), Some(FullAccess));
        assert_eq!(task_approval_policy(&codex, Some(Ask)), Some(Ask));
        // A delegation from the same parent stays under the auto ceiling.
        assert_eq!(
            child_approval_policy(&codex, &DelegationMode::Implement, Some(FullAccess)),
            Some(Auto)
        );
        // A level the provider lacks falls back to the delegation rule.
        assert_eq!(task_approval_policy(&[ReadOnly, Ask], Some(FullAccess)), Some(Ask));
        assert_eq!(
            task_approval_policy(&codex, None),
            child_approval_policy(&codex, &DelegationMode::Implement, None)
        );
    }

    #[test]
    fn parent_policy_is_the_newest_recorded_one() {
        let started = |sequence: u64, policy: Option<ProviderApprovalPolicy>| {
            event(
                sequence,
                SessionEventKind::TurnStarted {
                    turn_id: TurnId(format!("t-{sequence}")),
                    prompt: "p".to_string(),
                    plan_mode: None,
                    model: None,
                    evidence: None,
                    retry_of_turn_id: None,
                    approval_policy: policy,
                },
            )
        };
        let events = vec![
            started(1, Some(ProviderApprovalPolicy::FullAccess)),
            started(2, Some(ProviderApprovalPolicy::Ask)),
            started(3, None),
        ];
        assert_eq!(latest_turn_approval_policy(&events), Some(ProviderApprovalPolicy::Ask));
        assert_eq!(latest_turn_approval_policy(&[]), None);
    }

    #[test]
    fn unguarded_read_only_prompts_say_so() {
        let context = |writes_guarded| DelegationPromptContext {
            mode: &DelegationMode::Review,
            instruction: "Review",
            workspace_name: "w",
            workspace_branch: "main",
            workspace_path: "/w",
            parent_session_id: "p",
            parent_title: "t",
            git_context: None,
            writes_guarded,
            shared_checkout: false,
        };
        assert!(build_delegation_prompt(&context(false)).contains("DCC cannot sandbox this provider"));
        assert!(!build_delegation_prompt(&context(true)).contains("DCC cannot sandbox this provider"));
    }

    #[test]
    fn an_agent_delegated_implementation_edits_the_parents_checkout() {
        let prompt = build_delegation_prompt(&DelegationPromptContext {
            mode: &DelegationMode::Implement,
            instruction: "Fix the domain check",
            workspace_name: "w",
            workspace_branch: "main",
            workspace_path: "/w",
            parent_session_id: "p",
            parent_title: "t",
            git_context: None,
            writes_guarded: false,
            shared_checkout: true,
        });
        assert!(prompt.contains("You edit the parent's checkout directly"));
        assert!(!prompt.contains("isolated git worktree"));
        assert!(!prompt.contains("human review"));

        let mut done = delegation(DelegationStatus::Completed);
        done.mode = DelegationMode::Implement;
        done.budget.allow_file_edits = true;
        done.budget.shared_checkout = true;
        done.touched_files = vec!["src/domain.ts".to_string()];
        let text = delegation_result_turn(&done, None);
        assert!(text.contains("already in your workspace"), "{text}");
        assert!(!text.contains("NOT applied"));
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
                    snoozed_until: None,
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
                approval_policy: None,
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
                    title: None,
                    kind: Default::default(),
                    proposal_id: None,
                    shared_checkout: false,
                },
            )
            .await
            .expect_err("nested delegation must be refused");
            assert!(error.contains("cannot delegate further"));
        }

        fn agent_review_from_parent() -> RunDelegationInput {
            RunDelegationInput {
                parent_session_id: SessionId("parent".to_string()),
                // Unknown on purpose: a request that passes the guards stops
                // at the provider check instead of launching anything.
                target_provider_id: Some("not-a-provider".to_string()),
                target_model_id: None,
                mode: DelegationMode::Review,
                instruction: "review".to_string(),
                context_policy: DelegationContextPolicy::Minimal,
                origin: DelegationOrigin::Agent,
                prebuilt_prompt: None,
                effort: None,
                fast_mode: None,
                provider_runtime: None,
                title: None,
                kind: Default::default(),
                proposal_id: None,
                shared_checkout: false,
            }
        }

        #[tokio::test(flavor = "current_thread")]
        async fn a_turn_dcc_started_cannot_delegate() {
            let fixture = fixture().await;
            let state = &fixture.state;
            append(
                state,
                "parent",
                started("p-2", "[DCC] Delegated review task finished — Codex (task d-1)."),
            )
            .await;

            let error = run_delegation(state, agent_review_from_parent())
                .await
                .expect_err("a hand-back turn must not delegate");
            assert!(error.contains("started by DCC"), "{error}");
        }

        #[tokio::test(flavor = "current_thread")]
        async fn a_hand_back_steered_into_the_persons_turn_still_lets_it_delegate() {
            let fixture = fixture().await;
            let state = &fixture.state;
            append(state, "parent", started("p-1", "work")).await;
            append(
                state,
                "parent",
                SessionEventKind::TurnSteered {
                    turn_id: TurnId("p-1".to_string()),
                    prompt: "[DCC] Delegated review task finished — Codex (task d-1).".to_string(),
                },
            )
            .await;

            let error = run_delegation(state, agent_review_from_parent())
                .await
                .expect_err("stops at the unknown provider");
            assert!(!error.contains("started by DCC"), "{error}");
        }

        #[tokio::test(flavor = "current_thread")]
        async fn one_turn_cannot_start_unlimited_delegations() {
            let fixture = fixture().await;
            let state = &fixture.state;
            append(state, "parent", started("p-1", "work")).await;
            for _ in 0..MAX_AGENT_DELEGATIONS_PER_TURN {
                let mut finished = delegation(DelegationStatus::Completed);
                finished.id = DelegationId(Uuid::new_v4().to_string());
                finished.workspace_id = WorkspaceId("workspace".to_string());
                finished.origin = DelegationOrigin::Agent;
                finished.parent_turn_id = Some(TurnId("p-1".to_string()));
                DelegationRepo::save_delegation(state, &finished)
                    .await
                    .expect("save delegation");
            }

            let error = run_delegation(state, agent_review_from_parent())
                .await
                .expect_err("the per-turn cap must refuse");
            assert!(error.contains("already delegated 6 tasks in this turn"), "{error}");

            // The next turn the person starts gets a fresh allowance.
            append(
                state,
                "parent",
                SessionEventKind::TurnCompleted {
                    turn_id: TurnId("p-1".to_string()),
                },
            )
            .await;
            append(state, "parent", started("p-2", "keep going")).await;
            let error = run_delegation(state, agent_review_from_parent())
                .await
                .expect_err("stops at the unknown provider");
            assert!(!error.contains("in this turn"), "{error}");
        }

        fn proposal_input(title: &str) -> ProposeTaskInput {
            ProposeTaskInput {
                parent_session_id: SessionId("parent".to_string()),
                title: title.to_string(),
                instruction: "Implement phase 2.".to_string(),
                edits: true,
                // Unknown on purpose: a proposal that passes the guards
                // stops at the provider check.
                target_provider_id: Some("not-a-provider".to_string()),
                target_model_id: None,
                start_now: false,
            }
        }

        async fn saved_proposal(
            state: &SessionCommandState,
            title: &str,
            turn: Option<&str>,
        ) -> Delegation {
            create_task_proposal_with_state(
                state,
                CreateTaskProposalInput {
                    parent_session_id: SessionId("parent".to_string()),
                    parent_turn_id: turn.map(|turn| TurnId(turn.to_string())),
                    workspace_id: WorkspaceId("workspace".to_string()),
                    target_provider_id: dcc_core::domain::provider::ProviderId(
                        "codex".to_string(),
                    ),
                    target_model_id: None,
                    mode: DelegationMode::Implement,
                    title: title.to_string(),
                    instruction: "Implement it.".to_string(),
                    budget: DelegationBudget::default(),
                },
            )
            .await
            .expect("save proposal")
        }

        #[tokio::test(flavor = "current_thread")]
        async fn start_now_needs_a_turn_the_person_started() {
            let fixture = fixture().await;
            let state = &fixture.state;
            let now = |title: &str| ProposeTaskInput {
                start_now: true,
                ..proposal_input(title)
            };
            let error = propose_task(state, now("Fase 2")).await.expect_err("no open turn");
            assert!(error.contains("start=now"), "{error}");

            append(state, "parent", started("p-2", "plan the mobile app")).await;
            let error = propose_task(state, proposal_input("Fase 2"))
                .await
                .expect_err("stops at the unknown provider");
            assert!(!error.contains("start=now"), "{error}");
        }

        #[tokio::test(flavor = "current_thread")]
        async fn a_dcc_status_turn_cannot_propose_the_next_task() {
            // Review → fix → review chains came from status turns proposing
            // the next step with nobody asking.
            let fixture = fixture().await;
            let state = &fixture.state;
            append(
                state,
                "parent",
                started("p-2", "[DCC] Task \"Revisar correção\" ended a turn"),
            )
            .await;
            let error = propose_task(state, proposal_input("Corrigir os P1"))
                .await
                .expect_err("status turn");
            assert!(error.contains("[DCC] status cannot propose"), "{error}");
        }

        #[tokio::test(flavor = "current_thread")]
        async fn proposals_are_capped_per_turn_and_not_repeated() {
            let fixture = fixture().await;
            let state = &fixture.state;
            append(state, "parent", started("p-1", "plan the mobile app")).await;
            saved_proposal(state, "Fase 1", Some("p-1")).await;

            let error = propose_task(state, proposal_input("fase 1"))
                .await
                .expect_err("same title waits already");
            assert!(error.contains("already waiting"), "{error}");

            saved_proposal(state, "Fase 2", Some("p-1")).await;
            saved_proposal(state, "Fase 3", Some("p-1")).await;
            let error = propose_task(state, proposal_input("Fase 4"))
                .await
                .expect_err("fourth proposal in one turn");
            assert!(error.contains("already proposed 3 tasks"), "{error}");
        }

        #[tokio::test(flavor = "current_thread")]
        async fn a_task_cannot_propose_tasks() {
            let fixture = fixture().await;
            let state = &fixture.state;
            running_delegation(state, DelegationOrigin::Agent).await;
            let error = propose_task(
                state,
                ProposeTaskInput {
                    parent_session_id: SessionId("child".to_string()),
                    ..proposal_input("Fase 2")
                },
            )
            .await
            .expect_err("depth stays one");
            assert!(error.contains("cannot propose"), "{error}");
        }

        #[tokio::test(flavor = "current_thread")]
        async fn a_proposal_shows_a_card_and_starts_only_once() {
            let fixture = fixture().await;
            let state = &fixture.state;
            let proposal = saved_proposal(state, "Fase 2", None).await;
            assert_eq!(proposal.status, DelegationStatus::Draft);
            assert_eq!(proposal.kind, DelegationKind::Task);
            let parent_events =
                SessionEventRepo::list_events_by_session(state, &SessionId("parent".to_string()))
                    .await
                    .expect("history");
            assert!(parent_events.iter().any(|event| matches!(
                &event.kind,
                SessionEventKind::DelegationRequested { delegation_id } if *delegation_id == proposal.id
            )));

            cancel_delegation_with_state(
                state,
                CancelDelegationInput {
                    delegation_id: proposal.id.clone(),
                    reason: None,
                },
            )
            .await
            .expect("dismiss");
            let error = start_task_proposal(state, &proposal.id, None)
                .await
                .expect_err("a dismissed proposal cannot start");
            assert!(error.contains("already started or dismissed"), "{error}");
        }

        #[tokio::test(flavor = "current_thread")]
        async fn proposals_nobody_started_expire_after_a_week() {
            let fixture = fixture().await;
            let state = &fixture.state;
            let mut old = saved_proposal(state, "Fase antiga", None).await;
            old.created_at = (Utc::now() - chrono::Duration::days(8)).to_rfc3339();
            DelegationRepo::save_delegation(state, &old).await.expect("age it");
            let fresh = saved_proposal(state, "Fase nova", None).await;

            reconcile_running_delegations(state).await;

            let status = |id: DelegationId| async move {
                DelegationRepo::get_delegation(state, &id)
                    .await
                    .expect("load")
                    .expect("record")
                    .status
            };
            assert_eq!(status(old.id).await, DelegationStatus::Cancelled);
            assert_eq!(status(fresh.id).await, DelegationStatus::Draft);
        }

        #[tokio::test(flavor = "current_thread")]
        async fn a_task_ending_a_turn_wakes_the_parent_with_a_short_status() {
            let fixture = fixture().await;
            let state = &fixture.state;
            let mut task = running_delegation(state, DelegationOrigin::Agent).await;
            task.kind = DelegationKind::Task;
            task.title = Some("Fase 2".to_string());
            task.budget.timeout_seconds = None;
            // Long past any delegation timeout: a task has no clock.
            task.started_at = Some((Utc::now() - chrono::Duration::hours(3)).to_rfc3339());
            DelegationRepo::save_delegation(state, &task).await.expect("save task");
            append(state, "parent", started("p-1", "work")).await;
            append(state, "child", started("c-1", "Task from Dev Command Center, proposed in \"x\".")).await;
            reconcile_running_delegations(state).await;
            assert_eq!(
                DelegationRepo::get_delegation(state, &task.id)
                    .await
                    .expect("load")
                    .expect("task")
                    .status,
                DelegationStatus::Running
            );

            append(
                state,
                "child",
                SessionEventKind::TurnAssistantMessageCompleted {
                    turn_id: TurnId("c-1".to_string()),
                    message_id: "m".to_string(),
                    phase: AssistantMessagePhase::FinalAnswer,
                    content: Some(format!("Push notifications work. {}", "detail ".repeat(200))),
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

            let queued = queued_prompts(state, "parent").await;
            assert_eq!(queued.len(), 1);
            assert!(queued[0].starts_with("[DCC] Task \"Fase 2\" ended a turn"), "{}", queued[0]);
            assert!(queued[0].contains("Push notifications work."));
            assert!(queued[0].chars().count() < 900, "the status stays short");
        }

        async fn open_task(state: &SessionCommandState) -> Delegation {
            let mut task = running_delegation(state, DelegationOrigin::Agent).await;
            task.kind = DelegationKind::Task;
            task.title = Some("Fase 2".to_string());
            task.mode = DelegationMode::Research;
            task.budget = DelegationBudget {
                turn_limit: None,
                timeout_seconds: None,
                allow_file_edits: false,
                approval_policy: None,
                shared_checkout: false,
            };
            DelegationRepo::save_delegation(state, &task).await.expect("save task");
            task
        }

        async fn child_turn(state: &SessionCommandState, turn: &str, prompt: &str, answer: &str) {
            append(state, "child", started(turn, prompt)).await;
            append(
                state,
                "child",
                SessionEventKind::TurnAssistantMessageCompleted {
                    turn_id: TurnId(turn.to_string()),
                    message_id: format!("m-{turn}"),
                    phase: AssistantMessagePhase::FinalAnswer,
                    content: Some(answer.to_string()),
                },
            )
            .await;
            append(
                state,
                "child",
                SessionEventKind::TurnCompleted {
                    turn_id: TurnId(turn.to_string()),
                },
            )
            .await;
            on_session_turn_terminal(state.clone(), SessionId("child".to_string())).await;
        }

        async fn load(state: &SessionCommandState, id: &DelegationId) -> Delegation {
            DelegationRepo::get_delegation(state, id)
                .await
                .expect("load")
                .expect("record")
        }

        #[tokio::test(flavor = "current_thread")]
        async fn a_task_stays_open_and_wakes_the_parent_only_for_turns_it_waits_on() {
            let fixture = fixture().await;
            let state = &fixture.state;
            let task = open_task(state).await;
            append(state, "parent", started("p-1", "plan")).await;

            child_turn(state, "c-1", "Task from Dev Command Center, proposed in \"plan\".", "Phase 2 done.").await;
            // The watchdog announcing the same turn again reports nothing new.
            reconcile_running_delegations(state).await;
            let queued = queued_prompts(state, "parent").await;
            assert_eq!(queued.len(), 1);
            assert!(queued[0].contains("Phase 2 done."));
            assert!(queued[0].contains("Your tasks:\n- Fase 2 — open"), "{}", queued[0]);
            let stored = load(state, &task.id).await;
            assert_eq!(stored.status, DelegationStatus::Running);
            assert_eq!(stored.reported_turn_id, Some(TurnId("c-1".to_string())));

            // The person talking to the task: recorded, the parent left alone.
            child_turn(state, "c-2", "also rename the button", "Renamed.").await;
            assert_eq!(queued_prompts(state, "parent").await.len(), 1);
            assert_eq!(load(state, &task.id).await.result_summary.as_deref(), Some("Renamed."));

            // A follow-up the parent sent comes back to it.
            child_turn(state, "c-3", "[DCC] Message from the session that coordinates this task:\n\nadd tests", "Tests added.").await;
            let queued = queued_prompts(state, "parent").await;
            assert_eq!(queued.len(), 2);
            assert!(queued[1].contains("Tests added."));
        }

        #[tokio::test(flavor = "current_thread")]
        async fn an_interrupted_task_turn_keeps_the_task_open() {
            let fixture = fixture().await;
            let state = &fixture.state;
            let task = open_task(state).await;
            append(state, "parent", started("p-1", "plan")).await;
            append(state, "child", started("c-1", "Task from Dev Command Center, proposed in \"plan\".")).await;
            append(
                state,
                "child",
                SessionEventKind::TurnAborted {
                    turn_id: TurnId("c-1".to_string()),
                    reason: Some("Stopped by the person.".to_string()),
                },
            )
            .await;
            on_session_turn_terminal(state.clone(), SessionId("child".to_string())).await;

            assert_eq!(load(state, &task.id).await.status, DelegationStatus::Running);
            let queued = queued_prompts(state, "parent").await;
            assert_eq!(queued.len(), 1);
            assert!(queued[0].contains("The turn was interrupted: Stopped by the person."));
        }

        #[tokio::test(flavor = "current_thread")]
        async fn closing_a_task_waits_for_its_turn_to_end() {
            let fixture = fixture().await;
            let state = &fixture.state;
            let task = open_task(state).await;
            append(state, "child", started("c-1", "Task from Dev Command Center.")).await;
            let error = complete_task(state, &task.id).await.expect_err("still working");
            assert!(error.contains("still working"), "{error}");

            append(
                state,
                "child",
                SessionEventKind::TurnCompleted {
                    turn_id: TurnId("c-1".to_string()),
                },
            )
            .await;
            let closed = complete_task(state, &task.id).await.expect("close");
            assert_eq!(closed.status, DelegationStatus::Completed);
            let error = complete_task(state, &task.id).await.expect_err("closed already");
            assert!(error.contains("only an open task"), "{error}");
        }

        #[tokio::test(flavor = "current_thread")]
        async fn the_parent_messages_a_task_only_from_a_turn_the_person_started() {
            let fixture = fixture().await;
            let state = &fixture.state;
            let task = open_task(state).await;
            append(state, "parent", started("p-1", "[DCC] Task \"Fase 2\" ended a turn")).await;
            let error = message_task(state, &SessionId("parent".to_string()), &task.id.0, "add tests")
                .await
                .expect_err("a DCC turn cannot message");
            assert!(error.contains("a turn the human started"), "{error}");

            append(state, "parent", SessionEventKind::TurnCompleted { turn_id: TurnId("p-1".to_string()) }).await;
            append(state, "parent", started("p-2", "ask the task to add tests")).await;
            // The task is mid-turn: the message waits as a turn of its own.
            append(state, "child", started("c-1", "Task from Dev Command Center.")).await;
            message_task(state, &SessionId("parent".to_string()), &task.id.0, "add tests")
                .await
                .expect("queued");
            let queued = queued_prompts(state, "child").await;
            assert_eq!(queued.len(), 1);
            assert!(queued[0].starts_with("[DCC] Message from the session that coordinates this task:"));
            assert!(queued[0].ends_with("add tests"));
        }

        #[tokio::test(flavor = "current_thread")]
        async fn a_task_in_the_shared_checkout_closes_without_review() {
            let fixture = fixture().await;
            let state = &fixture.state;
            let mut task = open_task(state).await;
            task.mode = DelegationMode::Implement;
            task.budget.allow_file_edits = true;
            task.budget.shared_checkout = true;
            task.touched_files = vec!["src/push.ts".to_string()];
            DelegationRepo::save_delegation(state, &task).await.expect("save");

            let status = task_result_turn(&task, None, &[]);
            assert!(status.contains("directly in your checkout"), "{status}");

            let closed = complete_task(state, &task.id).await.expect("close");
            assert_eq!(closed.status, DelegationStatus::Completed);
        }

        #[tokio::test(flavor = "current_thread")]
        async fn only_one_child_edits_the_parents_checkout_at_a_time() {
            let fixture = fixture().await;
            let state = &fixture.state;
            let mut task = open_task(state).await;
            task.mode = DelegationMode::Implement;
            task.budget.allow_file_edits = true;
            task.budget.shared_checkout = true;
            DelegationRepo::save_delegation(state, &task)
                .await
                .expect("save");
            let parent = SessionId("parent".to_string());

            // An open task that is not working holds nothing.
            assert!(child_editing_the_checkout(state, &parent, None)
                .await
                .expect("check")
                .is_none());

            append(
                state,
                "child",
                started("c-1", "Task from Dev Command Center."),
            )
            .await;
            let busy = child_editing_the_checkout(state, &parent, None)
                .await
                .expect("check")
                .expect("the task is editing");
            assert_eq!(busy.id, task.id);
            assert!(busy_checkout_error(&busy).contains("only one child edits at a time"));
            // The task itself is not in its own way.
            assert!(child_editing_the_checkout(state, &parent, Some(&task.id))
                .await
                .expect("check")
                .is_none());
        }

        #[tokio::test(flavor = "current_thread")]
        async fn an_agent_delegation_in_the_shared_checkout_finishes_without_review() {
            let fixture = fixture().await;
            let state = &fixture.state;
            let mut delegation = open_task(state).await;
            delegation.kind = DelegationKind::Delegation;
            delegation.mode = DelegationMode::Implement;
            delegation.budget.allow_file_edits = true;
            delegation.budget.shared_checkout = true;
            DelegationRepo::save_delegation(state, &delegation)
                .await
                .expect("save");

            let finished = complete_delegation_with_state(
                state,
                CompleteDelegationInput {
                    delegation_id: delegation.id.clone(),
                    summary: Some("Fixed.".to_string()),
                    touched_files: vec!["src/domain.ts".to_string()],
                    diff_summary: None,
                    validation_summary: None,
                    review_required: true,
                },
            )
            .await
            .expect("finish")
            .delegation;
            assert_eq!(finished.status, DelegationStatus::Completed);
        }

        #[test]
        fn a_shared_checkout_task_is_told_it_edits_the_parents_files() {
            let prompt = build_task_prompt(&TaskPromptContext {
                instruction: "Add push.",
                workspace_name: "dcc",
                workspace_branch: "main",
                workspace_path: "/repo",
                parent_title: "Mobile",
                scope: TaskScope::SharedCheckout,
            });
            assert!(prompt.starts_with(TASK_BRIEF_HEADER));
            assert!(prompt.contains("You edit the parent's checkout directly"));
            assert!(!prompt.contains("isolated git worktree"));
        }

        #[tokio::test(flavor = "current_thread")]
        async fn tasks_do_not_use_up_the_delegation_allowance() {
            let fixture = fixture().await;
            let state = &fixture.state;
            append(state, "parent", started("p-1", "work")).await;
            for index in 0..MAX_AGENT_DELEGATIONS_PER_TURN {
                saved_proposal(state, &format!("Fase {index}"), Some("p-1")).await;
            }
            let error = run_delegation(state, agent_review_from_parent())
                .await
                .expect_err("stops at the unknown provider");
            assert!(!error.contains("in this turn"), "{error}");
            assert!(!error.contains("already running"), "{error}");
        }

        #[tokio::test(flavor = "current_thread")]
        async fn an_agent_delegation_finds_the_parent_workspace() {
            let fixture = fixture().await;
            let error = run_delegation(
                &fixture.state,
                RunDelegationInput {
                    parent_session_id: SessionId("parent".to_string()),
                    target_provider_id: Some("not-a-provider".to_string()),
                    target_model_id: None,
                    mode: DelegationMode::Review,
                    instruction: "review".to_string(),
                    context_policy: DelegationContextPolicy::Minimal,
                    origin: DelegationOrigin::Agent,
                    prebuilt_prompt: None,
                    effort: None,
                    fast_mode: None,
                    provider_runtime: None,
                    title: None,
                    kind: Default::default(),
                    proposal_id: None,
                    shared_checkout: false,
                },
            )
            .await
            .expect_err("an unknown provider is refused");
            // The refusal must come from the provider check, past the workspace lookup.
            assert!(!error.contains("workspace not found"), "{error}");
        }

        #[tokio::test(flavor = "current_thread")]
        async fn cancelling_stops_the_child_and_hands_nothing_back() {
            let fixture = fixture().await;
            let state = &fixture.state;
            let delegation = running_delegation(state, DelegationOrigin::Agent).await;
            append(state, "parent", started("p-1", "work")).await;
            append(state, "child", started("c-1", "review")).await;

            let cancelled = cancel_delegation_for_parent(
                state,
                &SessionId("parent".to_string()),
                &delegation.id.0,
                Some("no longer needed".to_string()),
            )
            .await
            .expect("cancel");
            assert_eq!(cancelled.status, DelegationStatus::Cancelled);
            let child_events =
                SessionEventRepo::list_events_by_session(state, &SessionId("child".to_string()))
                    .await
                    .expect("child history");
            assert!(matches!(child_outcome(&child_events), ChildOutcome::Aborted(_)));

            // The abort's terminal hook finds a cancelled delegation: no hand-back.
            on_session_turn_terminal(state.clone(), SessionId("child".to_string())).await;
            reconcile_running_delegations(state).await;
            assert!(queued_prompts(state, "parent").await.is_empty());
            // Cancelling again is a no-op that reports the current state.
            assert_eq!(
                cancel_delegation_for_parent(state, &SessionId("parent".to_string()), &delegation.id.0, None)
                    .await
                    .expect("idempotent")
                    .status,
                DelegationStatus::Cancelled
            );
        }

        async fn finish_child(state: &SessionCommandState) {
            append(state, "child", started("c-1", "review")).await;
            append(
                state,
                "child",
                SessionEventKind::TurnAssistantMessageCompleted {
                    turn_id: TurnId("c-1".to_string()),
                    message_id: "m".to_string(),
                    phase: AssistantMessagePhase::FinalAnswer,
                    content: Some("Waited result.".to_string()),
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
        }

        #[tokio::test(flavor = "current_thread")]
        async fn a_waiting_call_receives_the_result_instead_of_a_hand_back() {
            let fixture = fixture().await;
            let state = fixture.state.clone();
            let delegation = running_delegation(&state, DelegationOrigin::Agent).await;
            append(&state, "parent", started("p-1", "work")).await;

            let waiter_state = state.clone();
            let id = delegation.id.0.clone();
            let waiter = tokio::spawn(async move {
                wait_for_delegation(
                    &waiter_state,
                    &SessionId("parent".to_string()),
                    &id,
                    Duration::from_secs(10),
                )
                .await
            });
            tokio::time::sleep(Duration::from_millis(50)).await;
            finish_child(&state).await;

            let outcome = waiter.await.expect("join").expect("wait");
            assert!(!outcome.timed_out);
            assert!(outcome.result.as_deref().is_some_and(|text| text.contains("Waited result.")));
            assert_eq!(outcome.delegation.status, DelegationStatus::Completed);
            assert!(queued_prompts(&state, "parent").await.is_empty(), "delivered twice");
        }

        #[tokio::test(flavor = "current_thread")]
        async fn a_timed_out_wait_leaves_the_result_to_the_hand_back() {
            let fixture = fixture().await;
            let state = fixture.state.clone();
            let delegation = running_delegation(&state, DelegationOrigin::Agent).await;
            append(&state, "parent", started("p-1", "work")).await;

            let outcome = wait_for_delegation(
                &state,
                &SessionId("parent".to_string()),
                &delegation.id.0,
                Duration::from_millis(600),
            )
            .await
            .expect("wait");
            assert!(outcome.timed_out);
            assert!(outcome.result.is_none());

            finish_child(&state).await;
            let queued = queued_prompts(&state, "parent").await;
            assert_eq!(queued.len(), 1);
            assert!(queued[0].contains("Waited result."));
        }

        #[tokio::test(flavor = "current_thread")]
        async fn only_the_requesting_session_can_wait_and_only_once() {
            let fixture = fixture().await;
            let state = fixture.state.clone();
            let delegation = running_delegation(&state, DelegationOrigin::Agent).await;
            assert!(wait_for_delegation(
                &state,
                &SessionId("child".to_string()),
                &delegation.id.0,
                Duration::from_millis(10),
            )
            .await
            .is_err());
            let guard = WaitGuard::register(&state, &delegation).expect("first wait");
            assert!(WaitGuard::register(&state, &delegation).is_err());
            drop(guard);
            assert!(WaitGuard::register(&state, &delegation).is_ok());
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
