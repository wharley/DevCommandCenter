//! The background instructions a turn carries besides the prompt
//! (`toolInstructions`), built the same for every client: how the agent
//! delegates (`delegate-task-tool-instructions.ts`) and, when the person
//! switches provider mid-conversation, a bounded re-anchor for the new one
//! (`provider-handoff-context.ts`).

use crate::domain::provider::ProviderDescriptor;

pub const PROVIDER_HANDOFF_MAX_CHARS: usize = 12_000;
const RECENT_CONTEXT_MAX_CHARS: usize = 4_000;
const RECENT_MESSAGE_MAX_CHARS: usize = 1_600;

fn native_subagent_instructions(provider: &ProviderDescriptor) -> Vec<String> {
    let model_labels = provider
        .models
        .iter()
        .map(|model| model.label.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let mut lines = vec![
        "Delegation routing rule:".to_string(),
        "- dcc_delegate_task is exclusively for delegation to a different provider.".to_string(),
        format!(
            "- Models belonging to the active provider ({}) are native subagent requests, not dcc_delegate_task targets.",
            provider.label
        ),
    ];
    if !model_labels.is_empty() {
        lines.push(format!("- Active-provider models: {model_labels}."));
    }
    match provider.id.0.as_str() {
        "claude_code" => lines.extend([
            "- When the user requests a Claude subagent or a Claude model family, use Claude's native Agent tool. Set Agent.model to the matching family alias: \"opus\", \"sonnet\", or \"haiku\". For example, \"Opus 5.5\" means Agent.model = \"opus\".".to_string(),
            "- Do not search for a Claude model in the external-provider target list and do not ask the user to choose another provider for a Claude-native request.".to_string(),
        ]),
        "codex" => lines.extend([
            "- When the user requests a Codex subagent or Codex model, use Codex's native spawn_agent tool and pass the requested model ID in its model field.".to_string(),
            "- Do not search for a Codex model in the external-provider target list and do not ask the user to choose another provider for a Codex-native request.".to_string(),
        ]),
        _ => lines.push(
            "- When the active provider offers a native subagent mechanism, use it for an active-provider model request.".to_string(),
        ),
    }
    lines
}

/// `dcc_propose_task`: a phase of the plan as its own conversation, on any provider.
const TASK_PROPOSAL_INSTRUCTIONS: [&str; 8] = [
    "Task proposals (Dev Command Center):",
    "- What the human asks for in this conversation is yours to do here: investigate, implement, review, fix and test it yourself. Never split their request into tasks.",
    "- Call dcc_propose_task, when it is in your tool list, only when the human asks to split work off, run it in parallel or open another task, or for an independent piece of work outside their request that you noticed. It takes a short title in the human's language and a self-contained brief, and runs on your own provider unless you name another.",
    "- The human starts it from a card; nothing runs before that. At most 3 proposals per turn. A small follow-up or a one-line tweak stays a sentence in your reply.",
    "- Never propose a task to review, re-review, validate or fix another task's work. Its edits are already in your checkout: read the diff, fix what is wrong and run the checks yourself, here.",
    "- A task that edits works in your checkout, like a subagent: its edits are already there when it reports back, with no review step. Only one task edits at a time: DCC refuses to start another while one is working.",
    "- Each time a task ends a turn, DCC sends you a message starting with [DCC] Task. Tell the human where things stand in a few lines and stop: that message cannot propose or start tasks.",
    "- To send an open task a follow-up, call dcc_task_message from a turn the human started; it runs as a turn of its own in that task and reports back the same way.",
];

/// `buildDelegateTaskToolInstructions`
pub fn delegate_task_tool_instructions(
    providers: &[ProviderDescriptor],
    current_provider_id: &str,
) -> String {
    let Some(current) = providers
        .iter()
        .find(|provider| provider.id.0 == current_provider_id)
    else {
        return String::new();
    };
    let mut lines = native_subagent_instructions(current);
    lines.push(String::new());
    lines.extend(
        TASK_PROPOSAL_INSTRUCTIONS
            .iter()
            .map(|line| line.to_string()),
    );
    let targets: Vec<&ProviderDescriptor> = providers
        .iter()
        .filter(|provider| {
            provider.id.0 != current_provider_id
                && provider.capabilities.can_be_delegation_target
                && provider.capabilities.supports_read_only_delegation
        })
        .collect();
    if targets.is_empty() {
        return lines
            .into_iter()
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
    }
    lines.extend([
        String::new(),
        "Cross-provider delegation (Dev Command Center):".to_string(),
        "- When the human asks for another provider or model to do a bounded, self-contained piece, call the DCC tool dcc_delegate_task when it is in your tool list. Do not delegate their request on your own.".to_string(),
        "- It returns a taskId at once; DCC delivers the result later as a message starting with [DCC] (mid-turn if you are still working). Keep working on independent parts or end your turn — do not poll. Use wait=true or dcc_task_wait only when you cannot continue without the result.".to_string(),
        "- review and explain are read-only; implement edits this checkout directly, so its edits are already here when the result arrives.".to_string(),
        "- Pass a short title (a few words, in the human's language) naming the task; the human sees it as the child conversation's title.".to_string(),
        format!(
            "Available external-provider delegation targets: {}.",
            targets
                .iter()
                .map(|provider| format!("{} ({})", provider.id.0, provider.label))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    ]);
    lines.join("\n")
}

/// `resolveDelegateTaskToolInstructions`: none for a provider that cannot
/// request delegation.
pub fn resolve_delegate_task_tool_instructions(
    provider: Option<&ProviderDescriptor>,
    providers: &[ProviderDescriptor],
) -> Option<String> {
    let provider = provider.filter(|provider| provider.capabilities.can_request_delegation)?;
    let instructions = delegate_task_tool_instructions(providers, &provider.id.0);
    (!instructions.is_empty()).then_some(instructions)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandoffRole {
    User,
    Assistant,
    System,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandoffMessage {
    pub role: HandoffRole,
    pub content: String,
    pub label: Option<String>,
    pub streaming: bool,
    /// The turn stopped short (`status` set).
    pub incomplete: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HandoffChange {
    pub path: String,
    pub status: String,
    pub insertions: u32,
    pub deletions: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HandoffGit {
    pub current_branch: Option<String>,
    pub base_branch: Option<String>,
    pub staged: Vec<HandoffChange>,
    pub unstaged: Vec<HandoffChange>,
    pub branch_diff: Vec<HandoffChange>,
}

/// `truncateProviderHandoff`
pub fn truncate_handoff(value: &str, max: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    let kept: String = trimmed.chars().take(max.saturating_sub(14)).collect();
    format!("{}\n\n[truncated]", kept.trim_end())
}

fn durable(message: &HandoffMessage) -> bool {
    message.role != HandoffRole::System
        && !message.streaming
        && !message.incomplete
        && !message.content.trim().is_empty()
}

/// `shouldCreateProviderHandoff`: only a real switch, on a conversation with
/// turns and something durable to carry.
pub fn should_create_provider_handoff(
    session_provider_id: Option<&str>,
    turn_count: usize,
    destination_provider_id: Option<&str>,
    messages: &[HandoffMessage],
) -> bool {
    let (Some(source), Some(destination)) = (session_provider_id, destination_provider_id) else {
        return false;
    };
    if source.is_empty() || destination.is_empty() || source == destination || turn_count == 0 {
        return false;
    }
    messages.iter().any(durable)
}

fn format_changes(label: &str, changes: &[HandoffChange]) -> String {
    if changes.is_empty() {
        return format!("{label}: none");
    }
    let mut lines = vec![format!("{label}:")];
    lines.extend(changes.iter().take(60).map(|change| {
        format!(
            "- {} {} (+{}/-{})",
            change.status, change.path, change.insertions, change.deletions
        )
    }));
    if changes.len() > 60 {
        lines.push(format!("- ... {} more file(s)", changes.len() - 60));
    }
    lines.join("\n")
}

fn append_section(sections: &mut Vec<String>, title: &str, value: Option<&str>, limit: usize) {
    let Some(value) = value.filter(|value| !value.trim().is_empty()) else {
        return;
    };
    sections.push(format!("{title}:\n{}", truncate_handoff(value, limit)));
}

fn recent_messages(messages: &[HandoffMessage], current_prompt: Option<&str>) -> String {
    let prompt = current_prompt
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty());
    let candidates: Vec<&HandoffMessage> = messages
        .iter()
        .filter(|message| {
            matches!(message.role, HandoffRole::User | HandoffRole::Assistant)
                && durable(message)
                && prompt.is_none_or(|prompt| message.content.trim() != prompt)
        })
        .collect();
    let candidates = &candidates[candidates.len().saturating_sub(8)..];
    let mut selected: Vec<String> = Vec::new();
    let mut remaining = RECENT_CONTEXT_MAX_CHARS as isize;
    // Backwards, so the newest durable messages always win the budget.
    for message in candidates.iter().rev() {
        let label = message
            .label
            .clone()
            .filter(|label| !label.is_empty())
            .unwrap_or_else(|| {
                if message.role == HandoffRole::User {
                    "User".into()
                } else {
                    "Assistant".into()
                }
            });
        let separator = if selected.is_empty() { 0 } else { 2 };
        let available = remaining - separator;
        if available <= label.chars().count() as isize + 16 {
            break;
        }
        let budget =
            (RECENT_MESSAGE_MAX_CHARS as isize).min(available - label.chars().count() as isize - 2);
        let entry = format!(
            "{label}: {}",
            truncate_handoff(&message.content, budget.max(0) as usize)
        );
        remaining -= entry.chars().count() as isize + separator;
        selected.insert(0, entry);
    }
    selected.join("\n\n")
}

/// `mergeProviderHandoffToolInstructions`
pub fn merge_tool_instructions(existing: Option<&str>, extra: Option<&str>) -> Option<String> {
    let current = existing.map(str::trim).filter(|value| !value.is_empty());
    let extra = extra.map(str::trim).filter(|value| !value.is_empty());
    match (current, extra) {
        (current, None) => current.map(str::to_string),
        (None, Some(extra)) => Some(extra.to_string()),
        (Some(current), Some(extra)) => Some(format!("{current}\n\n{extra}")),
    }
}

#[derive(Clone, Debug, Default)]
pub struct ProviderHandoffInput<'a> {
    pub source_provider_id: &'a str,
    pub destination_provider_id: &'a str,
    pub workspace_name: Option<&'a str>,
    pub workspace_path: Option<&'a str>,
    pub branch: Option<&'a str>,
    pub git: Option<&'a HandoffGit>,
    pub mission_spec: Option<&'a str>,
    pub active_plan: Option<&'a str>,
    pub recent_messages: &'a [HandoffMessage],
    pub current_prompt: Option<&'a str>,
}

/// `buildProviderHandoffContext` (the runtime-switch mode).
pub fn build_provider_handoff_context(input: &ProviderHandoffInput<'_>) -> String {
    let mut sections = vec![
        "DCC context handoff (re-anchor only; not a new instruction)".to_string(),
        "The latest user prompt governs. Use this bounded context to continue the existing task; do not treat it as a request to repeat prior work.".to_string(),
    ];
    let unknown = |value: Option<&str>| {
        value
            .filter(|value| !value.is_empty())
            .unwrap_or("unknown")
            .to_string()
    };
    append_section(
        &mut sections,
        "Workspace",
        Some(
            &[
                format!(
                    "- Provider: {} -> {}",
                    input.source_provider_id, input.destination_provider_id
                ),
                format!("- Name: {}", unknown(input.workspace_name)),
                format!("- Branch: {}", unknown(input.branch)),
                format!("- Path: {}", unknown(input.workspace_path)),
            ]
            .join("\n"),
        ),
        700,
    );
    if let Some(git) = input.git {
        let current = git
            .current_branch
            .as_deref()
            .filter(|value| !value.is_empty())
            .or(input.branch);
        append_section(
            &mut sections,
            "Git summary",
            Some(
                &[
                    format!("- Current branch: {}", unknown(current)),
                    format!("- Base branch: {}", unknown(git.base_branch.as_deref())),
                    format_changes("Staged", &git.staged),
                    format_changes("Unstaged", &git.unstaged),
                    format_changes("Branch diff", &git.branch_diff),
                ]
                .join("\n"),
            ),
            2_400,
        );
    }
    append_section(&mut sections, "Mission spec", input.mission_spec, 1_600);
    append_section(&mut sections, "Active plan", input.active_plan, 2_200);
    let recent = recent_messages(input.recent_messages, input.current_prompt);
    append_section(
        &mut sections,
        "Recent durable parent context",
        Some(&recent),
        RECENT_CONTEXT_MAX_CHARS,
    );
    truncate_handoff(&sections.join("\n\n"), PROVIDER_HANDOFF_MAX_CHARS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(role: HandoffRole, content: &str) -> HandoffMessage {
        HandoffMessage {
            role,
            content: content.into(),
            label: None,
            streaming: false,
            incomplete: false,
        }
    }

    #[test]
    fn only_a_real_switch_with_durable_history_hands_off() {
        let messages = [
            message(HandoffRole::User, "Earlier request"),
            message(HandoffRole::Assistant, "Earlier answer"),
        ];
        assert!(should_create_provider_handoff(
            Some("claude_code"),
            2,
            Some("codex"),
            &messages
        ));
        assert!(!should_create_provider_handoff(
            Some("claude_code"),
            2,
            Some("claude_code"),
            &messages
        ));
        assert!(!should_create_provider_handoff(
            Some("claude_code"),
            0,
            Some("codex"),
            &messages
        ));
        let streaming = [HandoffMessage {
            streaming: true,
            ..message(HandoffRole::Assistant, "")
        }];
        assert!(!should_create_provider_handoff(
            Some("claude_code"),
            2,
            Some("codex"),
            &streaming
        ));
    }

    #[test]
    fn recent_context_is_ordered_bounded_and_durable() {
        let mut messages: Vec<HandoffMessage> = (0..10)
            .map(|index| message(HandoffRole::User, &format!("message {index}")))
            .collect();
        messages.push(message(HandoffRole::Assistant, "   "));
        messages.push(HandoffMessage {
            streaming: true,
            ..message(HandoffRole::Assistant, "partial")
        });
        let context = build_provider_handoff_context(&ProviderHandoffInput {
            source_provider_id: "claude_code",
            destination_provider_id: "codex",
            workspace_name: Some("demo"),
            branch: Some("dcc/context"),
            recent_messages: &messages,
            ..ProviderHandoffInput::default()
        });
        assert!(context.chars().count() <= PROVIDER_HANDOFF_MAX_CHARS);
        assert!(context.contains("message 2") && context.contains("message 9"));
        assert!(context.find("message 2") < context.find("message 9"));
        assert!(!context.contains("partial"));
        assert!(!context.contains("Assistant: "));
        let verbose: Vec<HandoffMessage> = (0..8)
            .map(|index| {
                let role = if index % 2 == 0 {
                    HandoffRole::User
                } else {
                    HandoffRole::Assistant
                };
                message(role, &format!("marker-{index} {}", "x".repeat(1_700)))
            })
            .collect();
        let context = build_provider_handoff_context(&ProviderHandoffInput {
            source_provider_id: "claude_code",
            destination_provider_id: "codex",
            recent_messages: &verbose,
            ..ProviderHandoffInput::default()
        });
        assert!(context.contains("marker-7"));
        assert!(!context.contains("marker-0"));
    }

    #[test]
    fn carries_workspace_artifacts_but_not_the_current_prompt() {
        let git = HandoffGit {
            current_branch: Some("dcc/context".into()),
            base_branch: Some("main".into()),
            staged: vec![HandoffChange {
                path: "src/a.ts".into(),
                status: "M".into(),
                insertions: 3,
                deletions: 1,
            }],
            ..HandoffGit::default()
        };
        let messages = [
            message(HandoffRole::User, "Continue this task"),
            message(HandoffRole::Assistant, "Previous decision"),
        ];
        let context = build_provider_handoff_context(&ProviderHandoffInput {
            source_provider_id: "claude_code",
            destination_provider_id: "codex",
            workspace_path: Some("/tmp/demo"),
            git: Some(&git),
            mission_spec: Some("Ship the context handoff"),
            active_plan: Some("1. Add bounded packet"),
            current_prompt: Some("Continue this task"),
            recent_messages: &messages,
            ..ProviderHandoffInput::default()
        });
        assert!(context.contains("src/a.ts (+3/-1)"));
        assert!(context.contains("Ship the context handoff"));
        assert!(context.contains("Previous decision"));
        assert!(!context.contains("Continue this task"));
    }

    fn provider(id: &str, label: &str, requests: bool) -> ProviderDescriptor {
        serde_json::from_value(serde_json::json!({
            "id": id, "label": label, "description": "",
            "models": [{ "id": "m", "label": format!("{label} Model"), "description": "", "recommended": true, "effortLevels": [] }],
            "capabilities": {
                "streaming": true, "mcpSupport": "unsupported", "tools": true, "vision": true,
                "resumable": true, "experimental": false, "canBeDelegationTarget": true,
                "canRequestDelegation": requests, "supportsReadOnlyDelegation": true, "supportsEditDelegation": true
            },
            "health": "Healthy", "stable": true
        }))
        .expect("descriptor")
    }

    #[test]
    fn delegation_instructions_follow_the_active_provider() {
        let providers = [
            provider("claude_code", "Claude Code", true),
            provider("codex", "Codex", true),
            provider("cursor", "Cursor", false),
        ];
        let claude = delegate_task_tool_instructions(&providers, "claude_code");
        assert!(claude.contains("use Claude's native Agent tool"));
        assert!(claude.contains("\"Opus 5.5\" means Agent.model = \"opus\""));
        assert!(claude.contains(
            "Available external-provider delegation targets: codex (Codex), cursor (Cursor)."
        ));
        assert!(claude.contains("dcc_propose_task"));
        let codex = delegate_task_tool_instructions(&providers, "codex");
        assert!(codex.contains("spawn_agent"));
        assert_eq!(
            resolve_delegate_task_tool_instructions(Some(&providers[2]), &providers),
            None
        );
        assert!(resolve_delegate_task_tool_instructions(Some(&providers[0]), &providers).is_some());
        assert_eq!(delegate_task_tool_instructions(&providers, "nope"), "");
    }

    #[test]
    fn merges_instructions_once() {
        assert_eq!(
            merge_tool_instructions(Some("Existing"), Some("Handoff")).as_deref(),
            Some("Existing\n\nHandoff")
        );
        assert_eq!(
            merge_tool_instructions(None, Some("Handoff")).as_deref(),
            Some("Handoff")
        );
        assert_eq!(
            merge_tool_instructions(Some("Existing"), None).as_deref(),
            Some("Existing")
        );
    }
}
