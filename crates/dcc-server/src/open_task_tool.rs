//! `dcc_open_task`: in a resident agent's own conversation, a task in one of
//! the person's projects (`dcc_tauri::agent_task`). It waits as a card the
//! person starts, or starts at once when their turn asked for it; then it is
//! a task of its own in their task list, and each turn it ends comes back to
//! the conversation as a `[DCC]` status. Other sessions get an error.

use dcc_core::domain::agent_conversation::OPEN_TASK_TOOL;
use dcc_core::domain::delegation::Delegation;
use dcc_core::domain::session::SessionId;
use dcc_tauri::agent_task::{open_task, OpenTaskInput, EFFORTS};
use dcc_tauri::delegation_runtime::DELEGATION_PROVIDER_IDS;
use dcc_tauri::state::SessionCommandState;
use serde::Deserialize;
use serde_json::{json, Value};

const MAX_TITLE_CHARS: usize = 80;
const MAX_BRIEF_CHARS: usize = 32_000;
const MAX_PROJECT_CHARS: usize = 200;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenTaskArgs {
    project: String,
    title: String,
    task: String,
    #[serde(default)]
    worktree: Option<bool>,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    effort: Option<String>,
    #[serde(default)]
    start: Option<String>,
}

impl OpenTaskArgs {
    pub fn starts_now(&self) -> bool {
        self.start.as_deref() == Some("now")
    }

    /// No `start` given: it starts when DCC allows it (the person asked in
    /// this turn, or an event woke an agent that may start tasks itself).
    pub fn starts_when_allowed(&self) -> bool {
        self.start.is_none()
    }
}

pub fn is_open_task_tool(name: &str) -> bool {
    name == OPEN_TASK_TOOL
}

pub fn definition() -> Value {
    json!({
        "name": OPEN_TASK_TOOL,
        "description": "Only in your own conversation as a DCC resident agent (your instructions carry a dcc_agent_conversation block): open a task in one of the person's projects, the way they would with \"Nova tarefa\". It becomes a task of its own in their task list, in a protected worktree unless worktree=false (the project's own checkout). Call it when the person asks for work in a project; questions about where things stand you answer yourself with dcc_overview. The task sees only your brief and the project, so make it self-contained: goal, area, constraints and what done looks like. When the person asked for the work in this turn the task starts at once; when an event woke you (a trigger, a routine) it starts only if the person lets you start tasks yourself, else it waits as a card they start. Pass start=propose when they want to see it before it runs. Each time the task ends a turn DCC sends you a short [DCC] status: tell the person where it stands and stop; send it more with dcc_task_message only when they ask. At most 3 per turn.",
        "inputSchema": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "project": {"type": "string", "minLength": 1, "maxLength": MAX_PROJECT_CHARS, "description": "The project's name as dcc_overview lists it."},
                "title": {"type": "string", "minLength": 1, "maxLength": MAX_TITLE_CHARS, "description": "A short name in the person's language: the card's and the task's title."},
                "task": {"type": "string", "minLength": 1, "maxLength": MAX_BRIEF_CHARS, "description": "The self-contained brief."},
                "worktree": {"type": "boolean", "default": true, "description": "true: a protected worktree from the project's base branch. false: the project's own checkout."},
                "provider": {"type": "string", "enum": DELEGATION_PROVIDER_IDS, "description": "Omit to use your own provider."},
                "model": {"type": "string", "maxLength": 128, "description": "Model id. Omit for the provider's default."},
                "effort": {"type": "string", "enum": EFFORTS, "default": "medium", "description": "The first turn's reasoning effort."},
                "start": {"type": "string", "enum": ["propose", "now"], "default": "propose", "description": "propose shows a card the person starts; now starts at once and is refused unless the person started this turn."}
            },
            "required": ["project", "title", "task"]
        },
        "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false}
    })
}

fn short_text(value: &str, max: usize) -> bool {
    let value = value.trim();
    !value.is_empty() && value.chars().count() <= max && !value.chars().any(char::is_control)
}

pub fn args_are_well_formed(args: &OpenTaskArgs) -> bool {
    short_text(&args.project, MAX_PROJECT_CHARS)
        && short_text(&args.title, MAX_TITLE_CHARS)
        && !args.task.trim().is_empty()
        && args.task.chars().count() <= MAX_BRIEF_CHARS
        && args
            .provider
            .as_deref()
            .is_none_or(|provider| DELEGATION_PROVIDER_IDS.contains(&provider.trim()))
        && args
            .model
            .as_deref()
            .is_none_or(|model| short_text(model, 128))
        && args
            .effort
            .as_deref()
            .is_none_or(|effort| EFFORTS.contains(&effort.trim()))
        && matches!(args.start.as_deref(), None | Some("propose") | Some("now"))
}

pub fn parse(arguments: Option<Value>) -> Option<OpenTaskArgs> {
    arguments
        .and_then(|arguments| serde_json::from_value::<OpenTaskArgs>(arguments).ok())
        .filter(args_are_well_formed)
}

pub fn call_is_well_formed(arguments: &Option<Value>) -> bool {
    parse(arguments.clone()).is_some()
}

/// Proposes or starts the task for the conversation `parent`.
pub async fn open(
    sessions: &SessionCommandState,
    parent: SessionId,
    args: OpenTaskArgs,
) -> Result<Delegation, String> {
    let start_now = args.starts_now();
    let start_when_allowed = args.starts_when_allowed();
    let clean = |value: Option<String>| {
        value
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    open_task(
        sessions,
        OpenTaskInput {
            parent_session_id: parent,
            project: args.project,
            title: args.title.trim().to_string(),
            instruction: args.task,
            worktree: args.worktree.unwrap_or(true),
            target_provider_id: clean(args.provider),
            target_model_id: clean(args.model),
            effort: clean(args.effort),
            start_now,
            start_when_allowed,
        },
    )
    .await
}

/// What the agent is told after the call, besides the task's view.
pub fn delivery(task: &Delegation) -> &'static str {
    match task.child_session_id.is_some() {
        true => "The task started in the person's task list. When it ends a turn, DCC sends you a short status starting with [DCC]. Do not poll.",
        _ => "The proposal waits as a card in this conversation; nothing runs until the person starts it. Mention it in your reply and end your turn.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_project_title_and_brief_are_needed() {
        let parse = |value: Value| parse(Some(value));
        let args = parse(json!({"project": "Checkout", "title": "Fase 2", "task": "Do it."}))
            .expect("the minimum");
        assert!(!args.starts_now() && args.starts_when_allowed());
        assert!(args.worktree.is_none());
        let shown = parse(
            json!({"project": "Checkout", "title": "Fase 2", "task": "Do it.", "start": "propose"}),
        )
        .unwrap();
        assert!(!shown.starts_now() && !shown.starts_when_allowed());
        assert!(parse(json!({"project": "Checkout", "title": "Fase 2", "task": "Do it.", "worktree": false, "effort": "high", "start": "now", "provider": "codex"})).is_some_and(|args| args.starts_now()));
        for bad in [
            json!({"title": "Fase 2", "task": "Do it."}),
            json!({"project": " ", "title": "Fase 2", "task": "Do it."}),
            json!({"project": "Checkout", "title": "Fase 2", "task": "Do it.", "effort": "turbo"}),
            json!({"project": "Checkout", "title": "Fase 2", "task": "Do it.", "start": "later"}),
            json!({"project": "Checkout", "title": "Fase 2", "task": "Do it.", "edits": true}),
        ] {
            assert!(parse(bad.clone()).is_none(), "{bad}");
        }
        assert_eq!(definition()["name"], OPEN_TASK_TOOL);
    }
}
