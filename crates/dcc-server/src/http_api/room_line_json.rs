//! A room's line as the phone gets it: the line, what its turn did counted
//! by kind (the desktop's folded "Trabalhou 4 min · rodou 1 comando", worded
//! on the phone) and the tasks it reports, to open them; and an approval a
//! room's conversation waits on, to answer it in place.

use std::collections::HashMap;

use dcc_core::domain::agent_routine::{is_quiet, routine_request, ROUTINE_MARK};
use dcc_core::domain::agent_source::{source_request, SOURCE_MARK};
use dcc_core::domain::delegation::Delegation;
use dcc_core::domain::room_approval::{may_always_allow, short_tool, PendingApproval};
use dcc_core::domain::task_report::{lines_changed, report_state, report_task_id, report_title};
use dcc_core::domain::team_room_lines::{RoomLine, Speaker};
use serde_json::{json, Map, Value};

/// A line DCC wrote to wake the agent, not the person: a routine's turn or
/// a source's news, as `(kind, what it is about)`.
pub(super) fn wake(line: &RoomLine) -> Option<(&'static str, String)> {
    if line.speaker != Speaker::Person {
        return None;
    }
    if line.text.starts_with(ROUTINE_MARK) {
        let about = routine_request(&line.text).unwrap_or_default();
        return Some(("routine", about.trim().to_string()));
    }
    if line.text.starts_with(SOURCE_MARK) {
        let about = source_request(&line.text)
            .map(|(watch, connector)| format!("{watch} ({connector})"))
            .unwrap_or_default();
        return Some(("source", about));
    }
    None
}

/// The lines the phone shows, as the desktop's room does: an answer with
/// nothing to report stays quiet, and so does the wake that asked for it.
pub(super) fn shown(lines: Vec<RoomLine>) -> Vec<RoomLine> {
    let mut hidden = vec![false; lines.len()];
    for (index, line) in lines.iter().enumerate() {
        if line.speaker != Speaker::Agent || !is_quiet(&line.text) {
            continue;
        }
        hidden[index] = true;
        // The wake of that conversation right before it, if no other line
        // of the conversation came in between.
        if let Some(asked) = lines[..index]
            .iter()
            .rposition(|before| before.session_id == line.session_id)
            .filter(|asked| wake(&lines[*asked]).is_some())
        {
            hidden[asked] = true;
        }
    }
    lines
        .into_iter()
        .zip(hidden)
        .filter_map(|(line, hidden)| (!hidden).then_some(line))
        .collect()
}

/// `text` as a preview reads it: a link by its label, no emphasis marks.
pub(super) fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let (before, link) = rest.split_at(open);
        out.push_str(before);
        let label_end = link.find("](");
        let url_end = label_end.and_then(|end| link[end..].find(')').map(|close| end + close));
        match (label_end, url_end) {
            (Some(label_end), Some(url_end)) if !link[1..label_end].contains('\n') => {
                out.push_str(&link[1..label_end]);
                rest = &link[url_end + 1..];
            }
            _ => {
                out.push('[');
                rest = &link[1..];
            }
        }
    }
    out.push_str(rest);
    out.replace("**", "").replace('`', "")
}

/// What the answer's turn did: `{seconds, tools, failures, counts}` with
/// `counts` by kind (`command`, `read`, `edit` in files, `search`, …).
fn steps_json(line: &RoomLine) -> Option<Value> {
    if line.steps.is_empty() {
        return None;
    }
    let counts: Map<String, Value> = line
        .steps
        .counts
        .iter()
        .map(|(kind, count)| (kind.key().to_string(), json!(count)))
        .collect();
    Some(json!({
        "seconds": line.receipt.and_then(|receipt| receipt.seconds),
        "tools": line.steps.tools,
        "failures": line.steps.failures,
        "counts": counts,
    }))
}

/// A task an agent opened, as the result card shows it.
pub(super) fn task_json(delegation: &Delegation) -> Value {
    let (additions, deletions) = lines_changed(delegation.diff_summary.as_deref());
    json!({
        "delegationId": delegation.id.0,
        "workspaceId": report_task_id(delegation),
        "sessionId": delegation.child_session_id.as_ref().map(|id| id.0.clone()),
        "title": report_title(delegation),
        "state": report_state(delegation).as_str(),
        "files": delegation.touched_files.len(),
        "additions": additions,
        "deletions": deletions,
    })
}

/// `line` with its `steps` and, when it reports tasks known in `tasks`
/// (by delegation id), the first as `task` and all as `reports`.
pub(super) fn line_json(line: &RoomLine, tasks: &HashMap<String, Value>) -> Value {
    let mut value = serde_json::to_value(line).unwrap_or(Value::Null);
    let Some(object) = value.as_object_mut() else {
        return value;
    };
    if let Some(steps) = steps_json(line) {
        object.insert("steps".into(), steps);
    }
    if let Some((kind, about)) = wake(line) {
        object.insert("kind".into(), json!(kind));
        object.insert("about".into(), json!(about));
    }
    let reports: Vec<Value> = line
        .tasks
        .iter()
        .filter_map(|id| tasks.get(id).cloned())
        .collect();
    if let Some(first) = reports.first() {
        object.insert("task".into(), first.clone());
        object.insert("reports".into(), Value::Array(reports));
    }
    value
}

/// An approval waiting in a room: who asks, the tool by its own name and
/// what it is about, on one line.
pub(super) fn approval_json(
    session_id: &str,
    agent_id: &str,
    agent_name: &str,
    request: &PendingApproval,
) -> Value {
    json!({
        "sessionId": session_id,
        "requestId": request.request_id,
        "agentId": agent_id,
        "agentName": agent_name,
        "toolName": short_tool(&request.tool_name),
        "title": request.title,
        "summary": request.detail(),
        "mayAlwaysAllow": may_always_allow(&request.tool_name),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::team_room_lines::{Receipt, Speaker};
    use dcc_core::domain::team_room_steps::TurnSteps;
    use dcc_core::domain::timeline::activity::ToolKind;

    fn line(tasks: &[&str], steps: TurnSteps) -> RoomLine {
        RoomLine {
            key: "s:t".into(),
            agent_id: "dev".into(),
            session_id: "s".into(),
            speaker: Speaker::Agent,
            text: "Feito.".into(),
            at: "2026-11-10T10:04:00Z".into(),
            receipt: Some(Receipt {
                seconds: Some(240),
                tokens: Some(1200),
            }),
            tasks: tasks.iter().map(|id| id.to_string()).collect(),
            steps,
        }
    }

    fn said(session: &str, speaker: Speaker, text: &str) -> RoomLine {
        RoomLine {
            session_id: session.into(),
            speaker,
            text: text.into(),
            ..line(&[], TurnSteps::default())
        }
    }

    #[test]
    fn a_quiet_answer_and_the_wake_that_asked_for_it_are_left_out() {
        let source = "[DCC] Source \"menções no Slack\" (Slack): 1 new item(s).\n\n- oi";
        let lines = vec![
            said("a", Speaker::Person, "oi"),
            said("a", Speaker::Agent, "Olá."),
            said("a", Speaker::Person, source),
            said("b", Speaker::Agent, "Em outra conversa."),
            said("a", Speaker::Agent, "`NOTHING_TO_REPORT`"),
            said("a", Speaker::Person, source),
            said("a", Speaker::Agent, "Chegou uma menção que pede você."),
            // Quiet after the person's own words: only the answer goes.
            said("b", Speaker::Person, "tudo certo?"),
            said("b", Speaker::Agent, "NOTHING_TO_REPORT"),
        ];
        let texts: Vec<String> = shown(lines).into_iter().map(|line| line.text).collect();
        assert_eq!(
            texts,
            [
                "oi",
                "Olá.",
                "Em outra conversa.",
                source,
                "Chegou uma menção que pede você.",
                "tudo certo?"
            ]
        );
        let woke = said("a", Speaker::Person, source);
        assert_eq!(
            wake(&woke),
            Some(("source", "menções no Slack (Slack)".to_string()))
        );
        let value = line_json(&woke, &HashMap::new());
        assert_eq!(value["kind"], "source");
        assert_eq!(value["about"], "menções no Slack (Slack)");
        assert!(wake(&said("a", Speaker::Person, "oi")).is_none());
        assert!(wake(&said("a", Speaker::Agent, source)).is_none());
    }

    #[test]
    fn a_preview_reads_a_link_by_its_label() {
        assert_eq!(
            plain("[Enviado no #tech](https://x.slack.com/a) para o **time**, veja `a.rs`"),
            "Enviado no #tech para o time, veja a.rs"
        );
        assert_eq!(plain("lista [1] e [2](sem fim"), "lista [1] e [2](sem fim");
        assert_eq!(plain("nada"), "nada");
    }

    #[test]
    fn a_line_carries_its_steps_and_the_tasks_it_reports() {
        let steps = TurnSteps {
            counts: vec![(ToolKind::Command, 1), (ToolKind::Read, 2)],
            failures: 1,
            tools: 3,
        };
        let tasks = HashMap::from([(
            "t-1".to_string(),
            json!({ "workspaceId": "w-campo", "title": "Campo X" }),
        )]);
        let value = line_json(&line(&["t-1", "t-gone"], steps), &tasks);
        assert_eq!(value["text"], "Feito.");
        assert_eq!(value["receipt"]["seconds"], 240);
        assert_eq!(
            value["steps"],
            json!({ "seconds": 240, "tools": 3, "failures": 1,
                    "counts": { "command": 1, "read": 2 } })
        );
        assert_eq!(value["task"]["workspaceId"], "w-campo");
        assert_eq!(value["reports"].as_array().unwrap().len(), 1);
        let quiet = line_json(&line(&[], TurnSteps::default()), &tasks);
        assert!(quiet.get("steps").is_none());
        assert!(quiet.get("task").is_none());
    }

    #[test]
    fn an_approval_names_the_tool_and_what_it_does() {
        let request = PendingApproval {
            request_id: "r-1".into(),
            tool_name: "mcp__dcc-slack__slack_send_message".into(),
            title: Some("Approve MCP tool call".into()),
            description: Some("Envia no #geral\nmais".into()),
            command: None,
            file: None,
        };
        let value = approval_json("s", "dev", "Dev", &request);
        assert_eq!(value["toolName"], "slack_send_message");
        assert_eq!(value["summary"], "Envia no #geral");
        assert_eq!(value["mayAlwaysAllow"], true);
        assert_eq!(value["agentName"], "Dev");
    }
}
