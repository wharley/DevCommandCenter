//! `foldAssistantTurnMessages` and `placeDelegationsAfterTheirTurn`.

use std::collections::{HashMap, HashSet};

use super::{Annotation, Role, TimelineMessage};
use crate::domain::session::AssistantMessagePhase;

fn same_native_subagent(left: &Annotation, right: &Annotation) -> bool {
    match (left, right) {
        (Annotation::NativeSubagent(left), Annotation::NativeSubagent(right)) => {
            left.id == right.id
                || (right.agent_thread_id.is_some()
                    && left.agent_thread_id == right.agent_thread_id)
                || (right.agent_id.is_some() && left.agent_id == right.agent_id)
        }
        _ => false,
    }
}

/// `mergeTurnAnnotation`: a later copy of the same item replaces it; a
/// subagent keeps the identity first shown and takes the fields it got.
fn merge_annotation(annotations: &mut Vec<Annotation>, incoming: &Annotation) {
    let position = annotations.iter().position(|existing| {
        if existing.kind() != incoming.kind() {
            return false;
        }
        match existing {
            Annotation::NativeSubagent(_) => same_native_subagent(existing, incoming),
            _ => existing.id() == incoming.id(),
        }
    });
    let Some(position) = position else {
        annotations.push(incoming.clone());
        return;
    };
    match (&mut annotations[position], incoming) {
        (Annotation::NativeSubagent(existing), Annotation::NativeSubagent(incoming)) => {
            let id = existing.id.clone();
            let merge = |target: &mut Option<String>, value: &Option<String>| {
                if value.is_some() {
                    *target = value.clone();
                }
            };
            merge(&mut existing.agent_id, &incoming.agent_id);
            merge(&mut existing.agent_thread_id, &incoming.agent_thread_id);
            merge(&mut existing.path, &incoming.path);
            merge(&mut existing.name, &incoming.name);
            merge(&mut existing.role, &incoming.role);
            merge(&mut existing.model, &incoming.model);
            merge(&mut existing.requested_model, &incoming.requested_model);
            merge(&mut existing.created_at, &incoming.created_at);
            existing.status = incoming.status.clone();
            existing.streaming = incoming.streaming;
            existing.id = id;
        }
        (slot, incoming) => *slot = incoming.clone(),
    }
}

/// A turn's rows cut where messages were sent mid-turn (`breaks`, the
/// indexes of those messages): each stretch folds on its own, so the message
/// sits between what the agent did before and after it.
fn stretches(rows: &[usize], breaks: &[usize]) -> Vec<Vec<usize>> {
    let mut stretches: Vec<Vec<usize>> = vec![Vec::new(); breaks.len() + 1];
    for &index in rows {
        let stretch = breaks.iter().filter(|&&at| at <= index).count();
        stretches[stretch].push(index);
    }
    stretches.retain(|rows| !rows.is_empty());
    stretches
}

/// Every structured turn becomes one assistant row (one per stretch when a
/// message was sent mid-turn): commentary goes to its activity and the final
/// answer (or, once settled, the last text) is the row's content. While the
/// turn runs the content stays empty; a stretch a message cut is done.
pub(super) fn fold_assistant_turns(
    mut messages: Vec<TimelineMessage>,
    by_turn: &[(String, Vec<usize>)],
    settled_turns: &HashSet<String>,
    breaks: &HashMap<String, Vec<usize>>,
    session_id: &str,
) -> Vec<TimelineMessage> {
    let mut hidden: HashSet<usize> = HashSet::new();
    let mut groups: Vec<(&String, Vec<usize>, bool, Option<usize>)> = Vec::new();
    for (turn_id, rows) in by_turn {
        let cuts = breaks.get(turn_id).map(Vec::as_slice).unwrap_or_default();
        let parts = stretches(rows, cuts);
        let count = parts.len();
        for (position, part) in parts.into_iter().enumerate() {
            let last = position + 1 == count;
            let settled = settled_turns.contains(turn_id) || !last;
            groups.push((turn_id, part, settled, (!last).then_some(position + 1)));
        }
    }
    for (turn_id, rows, settled, before) in groups {
        let rows = &rows;
        if rows.is_empty() {
            continue;
        }
        let final_answer = rows.iter().rev().copied().find(|&index| {
            messages[index].assistant_phase == Some(AssistantMessagePhase::FinalAnswer)
        });
        let last = *rows.last().expect("not empty");
        let terminal = if settled {
            final_answer
                .or_else(|| {
                    rows.iter()
                        .rev()
                        .copied()
                        .find(|&index| !messages[index].content.trim().is_empty())
                })
                .unwrap_or(last)
        } else {
            final_answer.unwrap_or(last)
        };

        let mut folded: Vec<Annotation> = Vec::new();
        for &index in rows {
            let message = &messages[index];
            // While the turn runs every message stays in order with the rest
            // of the activity, even one labelled final answer.
            let visible_answer = index == terminal && settled;
            if !visible_answer && !message.content.trim().is_empty() {
                merge_annotation(
                    &mut folded,
                    &Annotation::Commentary {
                        id: format!("commentary-{}", message.id),
                        content: message.content.clone(),
                        streaming: if !settled && index == terminal {
                            true
                        } else {
                            message.streaming
                        },
                        created_at: message.created_at.clone(),
                    },
                );
            }
            for annotation in &message.annotations {
                merge_annotation(&mut folded, annotation);
            }
            if index != terminal {
                hidden.insert(index);
            }
        }
        let message = &mut messages[terminal];
        message.id = match before {
            Some(part) => format!("assistant-{session_id}-{turn_id}-before-{part}"),
            None => format!("assistant-{session_id}-{turn_id}"),
        };
        message.turn_id = Some(turn_id.clone());
        message.streaming = !settled;
        message.annotations = folded;
        if !settled {
            message.content.clear();
        }
    }
    if hidden.is_empty() {
        return messages;
    }
    messages
        .into_iter()
        .enumerate()
        .filter(|(index, _)| !hidden.contains(index))
        .map(|(_, message)| message)
        .collect()
}

/// A delegation is requested mid-turn, but the turn's reply renders after
/// its folded activity: the card goes right after that turn's last row.
pub(super) fn place_delegations_after_their_turn(
    messages: Vec<TimelineMessage>,
) -> Vec<TimelineMessage> {
    if !messages
        .iter()
        .any(|message| message.delegation.is_some() && message.delegation_turn_id.is_some())
    {
        return messages;
    }
    let mut last_row_by_turn: HashMap<String, usize> = HashMap::new();
    for (index, message) in messages.iter().enumerate() {
        if let (Role::Assistant, Some(turn)) = (message.role, &message.turn_id) {
            last_row_by_turn.insert(turn.clone(), index);
        }
    }
    let mut anchored: HashMap<usize, Vec<TimelineMessage>> = HashMap::new();
    let mut kept: Vec<(usize, TimelineMessage)> = Vec::new();
    for (index, message) in messages.into_iter().enumerate() {
        let anchor = message
            .delegation_turn_id
            .as_ref()
            .and_then(|turn| last_row_by_turn.get(turn))
            .copied();
        match anchor {
            Some(anchor) if message.delegation.is_some() && anchor > index => {
                anchored.entry(anchor).or_default().push(message);
            }
            _ => kept.push((index, message)),
        }
    }
    let mut placed = Vec::with_capacity(kept.len());
    for (index, message) in kept {
        placed.push(message);
        if let Some(cards) = anchored.remove(&index) {
            placed.extend(cards);
        }
    }
    placed
}
