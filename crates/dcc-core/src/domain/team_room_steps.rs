//! What an answer's turn did, for the room's folded line ("Trabalhou 4 min ·
//! leu 6 arquivos · rodou 3 comandos"): the tool calls counted by kind when
//! the lines are built, and the steps listed only when the person opens one.
//! Counts and kinds are the task conversation's own (`timeline::activity`).

use crate::domain::timeline::activity::{
    classify_tool_action, mcp_tool_label, summarize_turn_work, tool_target, ToolKind,
    SUMMARY_KIND_ORDER,
};
use crate::domain::timeline::{Annotation, TimelineMessage, ToolCall};

/// A turn's tool calls by kind (edits count files), in the summary's order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TurnSteps {
    pub counts: Vec<(ToolKind, u32)>,
    pub failures: u32,
    /// Every tool call, so a turn that used none shows nothing.
    pub tools: u32,
}

impl TurnSteps {
    pub fn is_empty(&self) -> bool {
        self.tools == 0
    }

    /// Adds the calls of one message's annotations (a turn may spread its
    /// work over several).
    pub fn add(&mut self, annotations: &[Annotation]) {
        let summary = summarize_turn_work(annotations);
        self.tools += summary.tools as u32;
        self.failures += summary.failures as u32;
        for kind in SUMMARY_KIND_ORDER {
            let count = if kind == ToolKind::Edit {
                summary.edited_files
            } else {
                summary.counts.get(&kind).copied().unwrap_or(0)
            } as u32;
            if count == 0 {
                continue;
            }
            match self.counts.iter_mut().find(|(known, _)| *known == kind) {
                Some((_, total)) => *total += count,
                None => self.counts.push((kind, count)),
            }
        }
        // Kept in the summary's order whichever message came first.
        self.counts.sort_by_key(|(kind, _)| {
            SUMMARY_KIND_ORDER
                .iter()
                .position(|known| known == kind)
                .unwrap_or(usize::MAX)
        });
    }
}

/// One tool call of the turn, as the opened list shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub kind: ToolKind,
    /// The file, command or tool a row names.
    pub target: Option<String>,
    pub failed: bool,
}

/// The tool calls of `turn` in order, read from the projected conversation.
pub fn steps_of(messages: &[TimelineMessage], turn: &str) -> Vec<Step> {
    messages
        .iter()
        .filter(|message| message.turn_id.as_deref() == Some(turn))
        .flat_map(|message| &message.annotations)
        .filter_map(|annotation| match annotation {
            Annotation::ToolCall(call) => Some(step_of(call)),
            _ => None,
        })
        .collect()
}

/// One tool call as a step: its kind and what it acted on (also read by
/// the turn at work, `team_room_live`).
pub fn step_of(call: &ToolCall) -> Step {
    let kind = classify_tool_action(&call.action);
    let target = tool_target(call).or_else(|| match kind {
        ToolKind::Mcp => Some(mcp_tool_label(&call.action)),
        ToolKind::Other => Some(call.action.clone()),
        _ => None,
    });
    Step {
        kind,
        target,
        failed: call.failure.is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(id: &str, action: &str, file: Option<&str>, failed: bool) -> Annotation {
        Annotation::ToolCall(ToolCall {
            id: id.into(),
            action: action.into(),
            command: None,
            file: file.map(str::to_string),
            content: String::new(),
            detail: None,
            streaming: false,
            created_at: None,
            failure: failed.then(|| "boom".to_string()),
        })
    }

    #[test]
    fn calls_count_by_kind_in_the_summarys_order() {
        let mut steps = TurnSteps::default();
        steps.add(&[
            call("1", "Read", Some("/a/b/c.rs"), false),
            call("2", "Bash", None, true),
            call("3", "Read", Some("/a/d.rs"), false),
            call("4", "Edit", Some("/a/d.rs"), false),
            call("5", "Edit", Some("/a/d.rs"), false),
        ]);
        assert_eq!(
            steps.counts,
            [
                (ToolKind::Edit, 1),
                (ToolKind::Command, 1),
                (ToolKind::Read, 2)
            ]
        );
        assert_eq!((steps.tools, steps.failures), (5, 1));
        assert!(!steps.is_empty());
        assert!(TurnSteps::default().is_empty());
    }

    #[test]
    fn work_spread_over_messages_adds_up() {
        let mut steps = TurnSteps::default();
        steps.add(&[call("1", "Bash", None, false)]);
        steps.add(&[
            call("2", "Read", Some("/x/y.rs"), false),
            call("3", "Bash", None, false),
        ]);
        assert_eq!(steps.counts, [(ToolKind::Command, 2), (ToolKind::Read, 1)]);
    }

    #[test]
    fn the_opened_list_names_each_target() {
        let mut message = TimelineMessage {
            id: "m".into(),
            role: crate::domain::timeline::Role::Assistant,
            turn_id: Some("t".into()),
            turn_settled: true,
            turn_started_at: None,
            turn_ended_at: None,
            assistant_phase: None,
            content: String::new(),
            label: String::new(),
            model: None,
            streaming: false,
            created_at: None,
            status: None,
            annotations: vec![
                call("1", "Read", Some("/a/b/c.rs"), false),
                call("2", "mcp__slack__send", None, true),
            ],
            plan_mode: false,
            answers_plan_mode: false,
            evidence: None,
            retry_of_turn_id: None,
            action: None,
            delegation: None,
            delegation_turn_id: None,
            hand_back: None,
        };
        let steps = steps_of(std::slice::from_ref(&message), "t");
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].target.as_deref(), Some("b/c.rs"));
        assert_eq!(steps[1].target.as_deref(), Some("slack · send"));
        assert!(steps[1].failed);
        message.turn_id = Some("other".into());
        assert!(steps_of(&[message], "t").is_empty());
    }
}
