//! `projectWorkspaceMessages`: the event-by-event fold. Rows live in one
//! vector and the maps hold their positions, where the desktop shares
//! objects; the steps are the same.

use std::collections::{HashMap, HashSet};

use super::events::TimelineEvent;
use super::fold::{fold_assistant_turns, place_delegations_after_their_turn};
use super::hand_back::parse_delegation_hand_back;
use super::summary::{delegation_text, system_note, with_session_id};
use super::{
    Annotation, DelegationPhase, IncompleteStatus, NativeSubagent, OpenSession, Role,
    TimelineDelegation, TimelineMessage, ToolCall,
};
use crate::domain::provider::ToolCallDetail;
use crate::domain::session::{AssistantMessagePhase, SessionEventKind};

/// AskUserQuestion and ExitPlanMode show as their own cards, not as tools.
fn is_hidden_tool_action(action: &str) -> bool {
    matches!(action, "AskUserQuestion" | "ExitPlanMode")
}

/// `mergeToolCallDetail`: later non-empty fields win.
pub(super) fn merge_tool_call_detail(call: &mut ToolCall, detail: Option<&ToolCallDetail>) {
    let Some(detail) = detail else { return };
    let mut next = call.detail.clone().unwrap_or_default();
    let pick = |target: &mut Option<String>, value: &Option<String>| {
        if let Some(value) = value.as_ref().filter(|value| !value.is_empty()) {
            *target = Some(value.clone());
        }
    };
    pick(&mut next.command, &detail.command);
    pick(&mut next.file, &detail.file);
    pick(&mut next.input, &detail.input);
    pick(&mut next.output, &detail.output);
    pick(&mut next.diff, &detail.diff);
    if detail.exit_code.is_some() {
        next.exit_code = detail.exit_code;
    }
    if detail.truncated {
        next.truncated = true;
    }
    if call.command.is_none() {
        call.command = next.command.clone();
    }
    if call.file.is_none() {
        call.file = next.file.clone();
    }
    call.detail = Some(next);
}

#[derive(Default)]
struct Projector {
    session_id: String,
    messages: Vec<TimelineMessage>,
    /// The row each turn's loose events (reasoning, tools…) go to.
    assistant_buckets: HashMap<String, usize>,
    /// Native assistant items by turn and message id.
    assistant_items: HashMap<(String, String), usize>,
    /// Every assistant row of a turn, in the order they appeared.
    assistant_messages_by_turn: Vec<(String, Vec<usize>)>,
    active_items_by_turn: HashMap<String, Vec<usize>>,
    completed_assistant_items: HashSet<(String, String)>,
    completed_reasoning_items: HashSet<(String, String)>,
    completed_tool_call_items: HashSet<(String, String)>,
    delegation_buckets: HashMap<String, usize>,
    completed_turns: HashSet<String>,
    aborted_turns: HashMap<String, String>,
    usage_limited_turns: HashSet<String>,
    last_usage_limited_turn: Option<String>,
    turn_started_at: HashMap<String, String>,
    turn_ended_at: HashMap<String, String>,
    turn_model: Vec<(String, Option<String>)>,
    requested_subagent_models: HashMap<String, String>,
    confirmed_subagent_models: HashMap<String, String>,
    latest_turn_id: Option<String>,
    steers: HashMap<String, usize>,
}

impl Projector {
    fn started_at(&self, turn_id: &str, fallback: &str) -> String {
        self.turn_started_at
            .get(turn_id)
            .cloned()
            .unwrap_or_else(|| fallback.to_string())
    }

    fn turn_messages_mut(&mut self, turn_id: &str) -> &mut Vec<usize> {
        if let Some(position) = self
            .assistant_messages_by_turn
            .iter()
            .position(|(turn, _)| turn == turn_id)
        {
            return &mut self.assistant_messages_by_turn[position].1;
        }
        self.assistant_messages_by_turn
            .push((turn_id.to_string(), Vec::new()));
        &mut self
            .assistant_messages_by_turn
            .last_mut()
            .expect("just pushed")
            .1
    }

    fn turn_messages(&self, turn_id: &str) -> Option<&Vec<usize>> {
        self.assistant_messages_by_turn
            .iter()
            .find(|(turn, _)| turn == turn_id)
            .map(|(_, rows)| rows)
    }

    /// `ensureAssistantMessage`
    fn ensure_assistant(&mut self, turn_id: &str, created_at: String) -> usize {
        if let Some(&index) = self.assistant_buckets.get(turn_id) {
            return index;
        }
        let mut message = TimelineMessage::new(
            format!("assistant-{}-{turn_id}", self.session_id),
            Role::Assistant,
            "Assistant",
            String::new(),
        );
        message.turn_id = Some(turn_id.to_string());
        message.streaming = true;
        message.created_at = Some(created_at);
        self.messages.push(message);
        let index = self.messages.len() - 1;
        self.assistant_buckets.insert(turn_id.to_string(), index);
        index
    }

    /// `ensureAssistantItemMessage`
    fn ensure_assistant_item(
        &mut self,
        turn_id: &str,
        message_id: &str,
        phase: AssistantMessagePhase,
        make_current: bool,
        created_at: String,
    ) -> usize {
        let key = (turn_id.to_string(), message_id.to_string());
        if let Some(&index) = self.assistant_items.get(&key) {
            if phase != AssistantMessagePhase::Unknown {
                self.messages[index].assistant_phase = Some(phase);
            }
            if make_current {
                self.assistant_buckets.insert(turn_id.to_string(), index);
            }
            return index;
        }
        let mut message = TimelineMessage::new(
            format!("assistant-{}-{turn_id}-{message_id}", self.session_id),
            Role::Assistant,
            "Assistant",
            String::new(),
        );
        message.turn_id = Some(turn_id.to_string());
        message.assistant_phase = Some(phase);
        message.streaming = true;
        message.created_at = Some(created_at);
        self.messages.push(message);
        let index = self.messages.len() - 1;
        self.assistant_items.insert(key, index);
        let previous_bucket = self.assistant_buckets.get(turn_id).copied();
        let rows = self.turn_messages_mut(turn_id);
        if let Some(previous) = previous_bucket {
            if !rows.contains(&previous) {
                rows.push(previous);
            }
        }
        rows.push(index);
        if make_current {
            self.assistant_buckets.insert(turn_id.to_string(), index);
        }
        index
    }

    fn mark_active(&mut self, turn_id: &str, index: usize) {
        let active = self
            .active_items_by_turn
            .entry(turn_id.to_string())
            .or_default();
        if !active.contains(&index) {
            active.push(index);
        }
    }

    fn is_running(&self, turn_id: &str) -> bool {
        !self.completed_turns.contains(turn_id) && !self.aborted_turns.contains_key(turn_id)
    }

    fn incomplete(&self, turn_id: &str, reason: String) -> IncompleteStatus {
        IncompleteStatus {
            reason: Some(reason),
            usage_limited: self.usage_limited_turns.contains(turn_id),
            continued_on_account: None,
        }
    }

    /// `getOrCreateAnnotation`: same kind and id.
    fn annotation(&mut self, index: usize, annotation: Annotation) -> &mut Annotation {
        let annotations = &mut self.messages[index].annotations;
        let position = annotations
            .iter()
            .position(|item| item.kind() == annotation.kind() && item.id() == annotation.id());
        match position {
            Some(position) => &mut annotations[position],
            None => {
                annotations.push(annotation);
                annotations.last_mut().expect("just pushed")
            }
        }
    }

    fn find_tool_call(&mut self, turn_id: &str, id: &str) -> Option<&mut ToolCall> {
        let index = *self.assistant_buckets.get(turn_id)?;
        self.messages[index]
            .annotations
            .iter_mut()
            .find_map(|annotation| match annotation {
                Annotation::ToolCall(call) if call.id == id => Some(call),
                _ => None,
            })
    }

    /// The rows a turn's end touches: its native items, or its single bucket.
    fn rows_of_turn(&self, turn_id: &str) -> Vec<usize> {
        match self.turn_messages(turn_id) {
            Some(rows) => rows.clone(),
            None => self
                .assistant_buckets
                .get(turn_id)
                .copied()
                .into_iter()
                .collect(),
        }
    }

    fn apply(&mut self, event: &TimelineEvent) {
        let at = event.occurred_at.clone();
        match &event.kind {
            SessionEventKind::TurnStarted {
                turn_id,
                prompt,
                plan_mode,
                model,
                evidence,
                retry_of_turn_id,
                ..
            } => {
                let turn = turn_id.0.clone();
                self.latest_turn_id = Some(turn.clone());
                self.turn_model.retain(|(id, _)| *id != turn);
                self.turn_model.push((turn.clone(), model.clone()));
                self.turn_started_at.insert(turn.clone(), at.clone());
                let mut message = TimelineMessage::new(
                    format!("user-{}-{turn}", self.session_id),
                    Role::User,
                    "User",
                    prompt.clone(),
                );
                message.turn_id = Some(turn);
                message.created_at = Some(at);
                message.plan_mode = *plan_mode == Some(true);
                message.evidence = evidence.clone();
                message.retry_of_turn_id = retry_of_turn_id.as_ref().map(|id| id.0.clone());
                self.messages.push(message);
            }
            SessionEventKind::TurnSteered { turn_id, prompt } => {
                let count = self.steers.entry(turn_id.0.clone()).or_default();
                *count += 1;
                let mut message = TimelineMessage::new(
                    format!("user-steer-{}-{}-{count}", self.session_id, turn_id.0),
                    Role::User,
                    "User",
                    prompt.clone(),
                );
                message.created_at = Some(at);
                self.messages.push(message);
            }
            SessionEventKind::TurnDelta { turn_id, content } => {
                let turn = &turn_id.0;
                let created = self.started_at(turn, &at);
                let index = self.ensure_assistant(turn, created);
                let running = self.is_running(turn);
                let aborted = self.aborted_turns.get(turn).cloned();
                let status = aborted.map(|reason| self.incomplete(turn, reason));
                let message = &mut self.messages[index];
                message.content.push_str(content);
                message.streaming = running;
                if let Some(status) = status {
                    message.streaming = false;
                    message.status = Some(status);
                }
            }
            SessionEventKind::TurnAssistantMessageStarted {
                turn_id,
                message_id,
                phase,
            } => {
                let key = (turn_id.0.clone(), message_id.clone());
                if self.completed_assistant_items.contains(&key) {
                    return;
                }
                let created = self.started_at(&turn_id.0, &at);
                let index = self.ensure_assistant_item(
                    &turn_id.0,
                    message_id,
                    phase.clone(),
                    true,
                    created,
                );
                self.mark_active(&turn_id.0, index);
            }
            SessionEventKind::TurnAssistantMessageDelta {
                turn_id,
                message_id,
                content,
            } => {
                let key = (turn_id.0.clone(), message_id.clone());
                if self.completed_assistant_items.contains(&key) {
                    return;
                }
                let created = self.started_at(&turn_id.0, &at);
                let index = self.ensure_assistant_item(
                    &turn_id.0,
                    message_id,
                    AssistantMessagePhase::Unknown,
                    true,
                    created,
                );
                let running = self.is_running(&turn_id.0);
                let message = &mut self.messages[index];
                message.content.push_str(content);
                message.streaming = running;
                self.mark_active(&turn_id.0, index);
            }
            SessionEventKind::TurnAssistantMessageCompleted {
                turn_id,
                message_id,
                phase,
                content,
            } => {
                let turn = &turn_id.0;
                let key = (turn.clone(), message_id.clone());
                let exact = self.assistant_items.get(&key).copied();
                let active = self
                    .active_items_by_turn
                    .get(turn)
                    .cloned()
                    .unwrap_or_default();
                let sole = if exact.is_none() && active.len() == 1 {
                    Some(active[0])
                } else {
                    None
                };
                // Older adapters finish a streamed root message under the
                // authoritative snapshot id: alias it when exactly one
                // compatible item is active.
                let lifecycle = sole.filter(|&index| {
                    let current = self.messages[index].assistant_phase.clone();
                    current.is_none()
                        || current == Some(AssistantMessagePhase::Unknown)
                        || *phase == AssistantMessagePhase::Unknown
                        || current.as_ref() == Some(phase)
                });
                let index = match (exact, lifecycle) {
                    (Some(index), _) => index,
                    (None, Some(index)) => index,
                    (None, None) => {
                        let created = self.started_at(turn, &at);
                        self.ensure_assistant_item(turn, message_id, phase.clone(), false, created)
                    }
                };
                if lifecycle.is_some() && exact.is_none() {
                    self.assistant_items.insert(key, index);
                }
                let message = &mut self.messages[index];
                message.assistant_phase = Some(phase.clone());
                if let Some(content) = content {
                    message.content = content.clone();
                }
                message.streaming = false;
                for (candidate_key, &candidate) in &self.assistant_items {
                    if candidate == index {
                        self.completed_assistant_items.insert(candidate_key.clone());
                    }
                }
                if let Some(active) = self.active_items_by_turn.get_mut(turn) {
                    active.retain(|&item| item != index);
                }
            }
            SessionEventKind::TurnReasoningStarted {
                turn_id,
                reasoning_id,
                label,
            } => {
                let key = (turn_id.0.clone(), reasoning_id.clone());
                if self.completed_reasoning_items.contains(&key) {
                    return;
                }
                let created = self.started_at(&turn_id.0, &at);
                let index = self.ensure_assistant(&turn_id.0, created);
                self.annotation(
                    index,
                    Annotation::Reasoning {
                        id: reasoning_id.clone(),
                        label: Some(label.clone().unwrap_or_else(|| "Thinking".to_string())),
                        content: String::new(),
                        streaming: true,
                        created_at: Some(at),
                    },
                );
            }
            SessionEventKind::TurnReasoningDelta {
                turn_id,
                reasoning_id,
                content,
            } => {
                let key = (turn_id.0.clone(), reasoning_id.clone());
                if self.completed_reasoning_items.contains(&key) {
                    return;
                }
                let created = self.started_at(&turn_id.0, &at);
                let index = self.ensure_assistant(&turn_id.0, created);
                let annotation = self.annotation(
                    index,
                    Annotation::Reasoning {
                        id: reasoning_id.clone(),
                        label: Some("Thinking".to_string()),
                        content: String::new(),
                        streaming: true,
                        created_at: Some(at.clone()),
                    },
                );
                if let Annotation::Reasoning {
                    content: text,
                    streaming,
                    created_at,
                    ..
                } = annotation
                {
                    text.push_str(content);
                    *streaming = true;
                    created_at.get_or_insert(at);
                }
            }
            SessionEventKind::TurnReasoningCompleted {
                turn_id,
                reasoning_id,
            } => {
                self.completed_reasoning_items
                    .insert((turn_id.0.clone(), reasoning_id.clone()));
                if let Some(&index) = self.assistant_buckets.get(&turn_id.0) {
                    for annotation in &mut self.messages[index].annotations {
                        if let Annotation::Reasoning { id, streaming, .. } = annotation {
                            if id == reasoning_id {
                                *streaming = false;
                            }
                        }
                    }
                }
            }
            SessionEventKind::TurnToolCallStarted {
                turn_id,
                tool_call_id,
                action,
                command,
                file,
            } => {
                if is_hidden_tool_action(action) {
                    return;
                }
                let key = (turn_id.0.clone(), tool_call_id.clone());
                if self.completed_tool_call_items.contains(&key) {
                    return;
                }
                let created = self.started_at(&turn_id.0, &at);
                let index = self.ensure_assistant(&turn_id.0, created);
                self.annotation(
                    index,
                    Annotation::ToolCall(ToolCall {
                        id: tool_call_id.clone(),
                        action: action.clone(),
                        command: command.clone(),
                        file: file.clone(),
                        content: String::new(),
                        detail: None,
                        streaming: true,
                        created_at: Some(at),
                        failure: None,
                    }),
                );
            }
            SessionEventKind::TurnToolCallDelta {
                turn_id,
                tool_call_id,
                content,
            } => {
                let key = (turn_id.0.clone(), tool_call_id.clone());
                if self.completed_tool_call_items.contains(&key) {
                    return;
                }
                let created = self.started_at(&turn_id.0, &at);
                self.ensure_assistant(&turn_id.0, created);
                if let Some(call) = self.find_tool_call(&turn_id.0, tool_call_id) {
                    call.content.push_str(content);
                    call.streaming = true;
                    call.created_at.get_or_insert(at);
                }
            }
            SessionEventKind::TurnToolCallCompleted {
                turn_id,
                tool_call_id,
                detail,
            } => {
                self.completed_tool_call_items
                    .insert((turn_id.0.clone(), tool_call_id.clone()));
                if let Some(call) = self.find_tool_call(&turn_id.0, tool_call_id) {
                    call.streaming = false;
                    merge_tool_call_detail(call, detail.as_ref());
                }
            }
            SessionEventKind::TurnToolCallUpdated {
                turn_id,
                tool_call_id,
                detail,
            } => {
                if let Some(call) = self.find_tool_call(&turn_id.0, tool_call_id) {
                    merge_tool_call_detail(call, Some(detail));
                }
            }
            SessionEventKind::TurnToolCallFailed {
                turn_id,
                tool_call_id,
                reason,
                detail,
            } => {
                self.completed_tool_call_items
                    .insert((turn_id.0.clone(), tool_call_id.clone()));
                if let Some(call) = self.find_tool_call(&turn_id.0, tool_call_id) {
                    call.streaming = false;
                    call.failure = Some(
                        reason
                            .clone()
                            .unwrap_or_else(|| "Tool call failed".to_string()),
                    );
                    merge_tool_call_detail(call, detail.as_ref());
                }
            }
            SessionEventKind::TurnUserInputRequested {
                turn_id,
                request_id,
                questions,
            } => {
                let created = self.started_at(&turn_id.0, &at);
                let index = self.ensure_assistant(&turn_id.0, created);
                self.annotation(
                    index,
                    Annotation::UserInput {
                        id: request_id.clone(),
                        questions: questions.clone(),
                        answers: Vec::new(),
                        streaming: true,
                        created_at: Some(at),
                    },
                );
            }
            SessionEventKind::TurnUserInputResolved {
                turn_id,
                request_id,
                answers,
            } => {
                if let Some(&index) = self.assistant_buckets.get(&turn_id.0) {
                    for annotation in &mut self.messages[index].annotations {
                        if let Annotation::UserInput {
                            id,
                            answers: stored,
                            streaming,
                            created_at,
                            ..
                        } = annotation
                        {
                            if id == request_id {
                                *stored = answers.clone();
                                *streaming = false;
                                created_at.get_or_insert(at.clone());
                            }
                        }
                    }
                }
            }
            SessionEventKind::TurnPermissionRequested {
                turn_id,
                request_id,
                tool_name,
                title,
                description,
                command,
                file,
            } => {
                let created = self.started_at(&turn_id.0, &at);
                let index = self.ensure_assistant(&turn_id.0, created);
                self.annotation(
                    index,
                    Annotation::Approval {
                        id: request_id.clone(),
                        tool_name: tool_name.clone(),
                        title: title.clone(),
                        description: description.clone(),
                        command: command.clone(),
                        file: file.clone(),
                        behavior: None,
                        streaming: true,
                        created_at: Some(at),
                    },
                );
            }
            SessionEventKind::TurnPermissionResolved {
                turn_id,
                request_id,
                behavior,
            } => {
                if let Some(&index) = self.assistant_buckets.get(&turn_id.0) {
                    for annotation in &mut self.messages[index].annotations {
                        if let Annotation::Approval {
                            id,
                            behavior: stored,
                            streaming,
                            created_at,
                            ..
                        } = annotation
                        {
                            if id == request_id {
                                *stored = Some(behavior.clone());
                                *streaming = false;
                                created_at.get_or_insert(at.clone());
                            }
                        }
                    }
                }
            }
            SessionEventKind::TurnNativeSubagentActivity {
                turn_id,
                id,
                agent_id,
                agent_thread_id,
                path,
                name,
                role,
                model,
                status,
            } => {
                let created = self.started_at(&turn_id.0, &at);
                let index = self.ensure_assistant(&turn_id.0, created);
                let correlation = agent_thread_id.clone().unwrap_or_else(|| id.clone());
                let requested = self.requested_subagent_models.get(&correlation).cloned();
                let confirmed = self.confirmed_subagent_models.get(&correlation).cloned();
                let annotations = &mut self.messages[index].annotations;
                let existing = annotations
                    .iter_mut()
                    .find_map(|annotation| match annotation {
                        Annotation::NativeSubagent(agent)
                            if agent.id == *id
                                || (agent_thread_id.is_some()
                                    && agent.agent_thread_id == *agent_thread_id)
                                || (agent_id.is_some() && agent.agent_id == *agent_id) =>
                        {
                            Some(agent)
                        }
                        _ => None,
                    });
                match existing {
                    Some(agent) => {
                        agent.agent_id = agent_id.clone().or(agent.agent_id.take());
                        agent.agent_thread_id =
                            agent_thread_id.clone().or(agent.agent_thread_id.take());
                        agent.path = path.clone().or(agent.path.take());
                        agent.name = name.clone().or(agent.name.take());
                        agent.role = role.clone().or(agent.role.take());
                        agent.model = model.clone().or(agent.model.take());
                        agent.model = confirmed.or(agent.model.take());
                        agent.requested_model = requested.or(agent.requested_model.take());
                        agent.status = status.clone();
                        agent.created_at.get_or_insert(at);
                    }
                    None => annotations.push(Annotation::NativeSubagent(NativeSubagent {
                        id: id.clone(),
                        agent_id: agent_id.clone(),
                        agent_thread_id: agent_thread_id.clone(),
                        path: path.clone(),
                        name: name.clone(),
                        role: role.clone(),
                        model: confirmed.or_else(|| model.clone()),
                        requested_model: requested,
                        status: status.clone(),
                        streaming: false,
                        created_at: Some(at),
                    })),
                }
            }
            SessionEventKind::TurnNativeSubagentModelRequested {
                turn_id,
                correlation_id,
                model,
            }
            | SessionEventKind::TurnNativeSubagentModelConfirmed {
                turn_id,
                correlation_id,
                model,
            } => {
                let confirmed = matches!(
                    event.kind,
                    SessionEventKind::TurnNativeSubagentModelConfirmed { .. }
                );
                if confirmed {
                    self.confirmed_subagent_models
                        .insert(correlation_id.clone(), model.clone());
                } else {
                    self.requested_subagent_models
                        .insert(correlation_id.clone(), model.clone());
                }
                if let Some(&index) = self.assistant_buckets.get(&turn_id.0) {
                    for annotation in &mut self.messages[index].annotations {
                        if let Annotation::NativeSubagent(agent) = annotation {
                            if agent.agent_thread_id.as_deref() == Some(correlation_id)
                                || agent.id == *correlation_id
                            {
                                if confirmed {
                                    agent.model = Some(model.clone());
                                } else {
                                    agent.requested_model = Some(model.clone());
                                }
                                break;
                            }
                        }
                    }
                }
            }
            SessionEventKind::TurnModelEffective { turn_id, model } => {
                self.turn_model.retain(|(id, _)| *id != turn_id.0);
                self.turn_model
                    .push((turn_id.0.clone(), Some(model.clone())));
            }
            SessionEventKind::TurnCompleted { turn_id } => {
                let turn = turn_id.0.clone();
                self.completed_turns.insert(turn.clone());
                self.turn_ended_at.insert(turn.clone(), at);
                for index in self.rows_of_turn(&turn) {
                    let message = &mut self.messages[index];
                    message.streaming = false;
                    for annotation in &mut message.annotations {
                        annotation.set_streaming(false);
                    }
                }
            }
            SessionEventKind::UsageLimitReached { turn_id, .. } => {
                self.usage_limited_turns.insert(turn_id.0.clone());
                self.last_usage_limited_turn = Some(turn_id.0.clone());
            }
            SessionEventKind::UsageLimitResumed { account_label, .. } => {
                if let (Some(account), Some(turn)) = (account_label, &self.last_usage_limited_turn)
                {
                    for message in &mut self.messages {
                        if message.turn_id.as_deref() == Some(turn.as_str()) {
                            if let Some(status) = message
                                .status
                                .as_mut()
                                .filter(|status| status.usage_limited)
                            {
                                status.continued_on_account = Some(account.clone());
                            }
                        }
                    }
                }
            }
            SessionEventKind::TurnAborted { turn_id, reason } => {
                let turn = turn_id.0.clone();
                let reason = reason.clone().unwrap_or_else(|| "Turn aborted".to_string());
                self.aborted_turns.insert(turn.clone(), reason.clone());
                self.turn_ended_at.insert(turn.clone(), at.clone());
                let mut rows = self.rows_of_turn(&turn);
                if rows.is_empty() && self.turn_started_at.contains_key(&turn) {
                    // A turn that died before any output still needs a place
                    // for its reason and retry/continue.
                    let created = self.started_at(&turn, &at);
                    rows.push(self.ensure_assistant(&turn, created));
                }
                let status = self.incomplete(&turn, reason);
                for index in rows {
                    let message = &mut self.messages[index];
                    message.streaming = false;
                    message.status = Some(status.clone());
                    for annotation in &mut message.annotations {
                        annotation.set_streaming(false);
                    }
                }
            }
            SessionEventKind::SessionStarted { forked_from, .. } => {
                // Session creation stays in the log; only a fork is shown.
                let Some(origin) = forked_from else { return };
                let mut message = TimelineMessage::new(
                    format!("session.started-{}", self.messages.len()),
                    Role::System,
                    "session.forked",
                    format!(
                        "Forked from an earlier thread{}. A bounded snapshot of that thread up to the fork point follows the first turn here; nothing after it was carried over.",
                        if origin.turn_id.is_some() { " at one of its turns" } else { "" }
                    ),
                );
                message.created_at = Some(at);
                message.action = Some(OpenSession {
                    session_id: origin.session_id.0.clone(),
                    label: "Open source thread".to_string(),
                });
                self.messages.push(message);
            }
            SessionEventKind::DelegationRequested { delegation_id }
            | SessionEventKind::DelegationStarted { delegation_id, .. }
            | SessionEventKind::DelegationDelta { delegation_id, .. }
            | SessionEventKind::DelegationCompleted { delegation_id, .. }
            | SessionEventKind::DelegationFailed { delegation_id, .. }
            | SessionEventKind::DelegationCancelled { delegation_id, .. } => {
                self.apply_delegation(&delegation_id.0, &event.kind, at);
            }
            kind => {
                if let Some((label, content)) = system_note(kind) {
                    let mut message = TimelineMessage::new(
                        format!("{label}-{}", self.messages.len()),
                        Role::System,
                        label,
                        with_session_id(label, content, &self.session_id),
                    );
                    message.created_at = Some(at);
                    self.messages.push(message);
                }
            }
        }
    }

    fn apply_delegation(&mut self, delegation_id: &str, kind: &SessionEventKind, at: String) {
        let text = delegation_text(kind).unwrap_or_default();
        let index = match self.delegation_buckets.get(delegation_id) {
            Some(&index) => index,
            None => {
                let mut message = TimelineMessage::new(
                    format!("delegation-{delegation_id}"),
                    Role::System,
                    "delegation",
                    text.clone(),
                );
                message.created_at = Some(at);
                message.delegation = Some(TimelineDelegation {
                    id: delegation_id.to_string(),
                    phase: DelegationPhase::Requested,
                    child_session_id: None,
                    summary: None,
                    reason: None,
                });
                message.delegation_turn_id = self.latest_turn_id.clone();
                self.messages.push(message);
                let index = self.messages.len() - 1;
                self.delegation_buckets
                    .insert(delegation_id.to_string(), index);
                index
            }
        };
        let message = &mut self.messages[index];
        let Some(delegation) = message.delegation.as_mut() else {
            return;
        };
        match kind {
            SessionEventKind::DelegationStarted {
                child_session_id, ..
            } => {
                delegation.phase = DelegationPhase::Running;
                if let Some(child) = child_session_id {
                    delegation.child_session_id = Some(child.0.clone());
                }
                if let Some(child) = &delegation.child_session_id {
                    message.action = Some(OpenSession {
                        session_id: child.clone(),
                        label: "Open child session".to_string(),
                    });
                }
                message.content = text;
            }
            SessionEventKind::DelegationDelta { .. } => message.content = text,
            SessionEventKind::DelegationCompleted { summary, .. } => {
                delegation.phase = DelegationPhase::Completed;
                delegation.summary = summary.clone();
                message.content = text;
            }
            SessionEventKind::DelegationFailed { reason, .. } => {
                delegation.phase = DelegationPhase::Failed;
                delegation.reason = reason.clone();
                message.content = text;
            }
            SessionEventKind::DelegationCancelled { reason, .. } => {
                delegation.phase = DelegationPhase::Cancelled;
                delegation.reason = reason.clone();
                message.content = text;
            }
            _ => {}
        }
    }

    fn finish(mut self) -> Vec<TimelineMessage> {
        for (turn, model) in std::mem::take(&mut self.turn_model) {
            let Some(model) = model else { continue };
            for index in self.turn_messages(&turn).cloned().unwrap_or_default() {
                self.messages[index].model = Some(model.clone());
            }
            if let Some(&index) = self.assistant_buckets.get(&turn) {
                self.messages[index].model = Some(model);
            }
        }
        let mut last_user_plan_mode = false;
        for message in &mut self.messages {
            match message.role {
                Role::User => {
                    last_user_plan_mode = message.plan_mode;
                    message.hand_back = parse_delegation_hand_back(&message.content);
                }
                Role::Assistant => message.answers_plan_mode = last_user_plan_mode,
                Role::System => {}
            }
        }
        for message in &mut self.messages {
            let (Role::Assistant, Some(turn)) = (message.role, message.turn_id.as_ref()) else {
                continue;
            };
            message.turn_settled =
                self.completed_turns.contains(turn) || self.aborted_turns.contains_key(turn);
            message.turn_started_at = self.turn_started_at.get(turn).cloned();
            message.turn_ended_at = self.turn_ended_at.get(turn).cloned();
        }
        let settled: HashSet<String> = self
            .completed_turns
            .iter()
            .chain(self.aborted_turns.keys())
            .cloned()
            .collect();
        let messages = fold_assistant_turns(
            self.messages,
            &self.assistant_messages_by_turn,
            &settled,
            &self.session_id,
        );
        place_delegations_after_their_turn(messages)
    }
}

pub(super) fn project(events: &[TimelineEvent], session_id: &str) -> Vec<TimelineMessage> {
    let mut projector = Projector {
        session_id: session_id.to_string(),
        ..Projector::default()
    };
    for event in events {
        projector.apply(event);
    }
    projector.finish()
}
