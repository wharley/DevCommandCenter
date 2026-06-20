use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{
    CodeRabbitFinding, CodeRabbitFindingSeverity, CodeRabbitReviewComplete,
    CodeRabbitReviewStatusEvent,
};

#[derive(Clone, Debug, Default)]
pub(crate) struct ParsedCodeRabbitAgentOutput {
    pub findings: Vec<CodeRabbitFinding>,
    pub statuses: Vec<CodeRabbitReviewStatusEvent>,
    pub complete: Option<CodeRabbitReviewComplete>,
    pub errors: Vec<String>,
    pub event_count: u32,
}

pub(crate) fn parse_agent_jsonl(stdout: &str) -> ParsedCodeRabbitAgentOutput {
    let mut parsed = ParsedCodeRabbitAgentOutput::default();

    for (index, line) in stdout.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let Ok(value) = serde_json::from_str::<Value>(line) else {
            parsed
                .errors
                .push(format!("line {} is not valid JSON", index + 1));
            continue;
        };

        parsed.event_count += 1;
        match value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "finding" => {
                if let Some(finding) = parse_finding(&value, parsed.findings.len()) {
                    parsed.findings.push(finding);
                }
            }
            "status" => parsed.statuses.push(parse_status_event(&value)),
            "complete" => parsed.complete = Some(parse_complete_event(&value)),
            "error" => parsed.errors.push(
                value
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("CodeRabbit emitted an error event")
                    .to_string(),
            ),
            "heartbeat" | "review_context" => {}
            other if !other.is_empty() => parsed.statuses.push(CodeRabbitReviewStatusEvent {
                event_type: other.to_string(),
                status: string_field(&value, &["status"]),
                message: string_field(&value, &["message", "detail"]),
            }),
            _ => {}
        }
    }

    parsed
}

fn parse_finding(value: &Value, index: usize) -> Option<CodeRabbitFinding> {
    let path = string_field(value, &["fileName", "filename", "file", "path"])?;
    let severity_raw = string_field(value, &["severity"]).unwrap_or_else(|| "info".to_string());
    let severity = CodeRabbitFindingSeverity::from_cli_value(&severity_raw);
    let comment = string_field(value, &["comment", "message", "title"]);
    let codegen_instructions =
        string_field(value, &["codegenInstructions", "codegen_instructions"]);
    let suggestions = string_array_field(value, "suggestions");
    let start_line = number_field(value, &["startLine", "start_line", "line", "lineNumber"])
        .or_else(|| nested_number_field(value, &["location"], &["startLine", "line"]));
    let end_line = number_field(value, &["endLine", "end_line"])
        .or_else(|| nested_number_field(value, &["location"], &["endLine"]))
        .or(start_line);
    let side = string_field(value, &["side"]);

    Some(CodeRabbitFinding {
        id: finding_id(
            &path,
            start_line,
            comment.as_deref(),
            codegen_instructions.as_deref(),
            index,
        ),
        severity,
        severity_raw,
        path,
        start_line,
        end_line,
        side,
        comment,
        codegen_instructions,
        suggestions,
    })
}

fn parse_status_event(value: &Value) -> CodeRabbitReviewStatusEvent {
    CodeRabbitReviewStatusEvent {
        event_type: "status".to_string(),
        status: string_field(value, &["status"]),
        message: string_field(value, &["message", "detail"]),
    }
}

fn parse_complete_event(value: &Value) -> CodeRabbitReviewComplete {
    CodeRabbitReviewComplete {
        status: string_field(value, &["status"]),
        findings: number_field(value, &["findings"]).map(|value| value as u32),
        message: string_field(value, &["message"]),
    }
}

fn string_field(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(raw) = value.get(*key).and_then(Value::as_str) {
            let raw = raw.trim();
            if !raw.is_empty() {
                return Some(raw.to_string());
            }
        }
    }
    None
}

fn number_field(value: &Value, keys: &[&str]) -> Option<u32> {
    for key in keys {
        if let Some(raw) = value.get(*key).and_then(Value::as_u64) {
            return Some(raw as u32);
        }
    }
    None
}

fn nested_number_field(value: &Value, parents: &[&str], keys: &[&str]) -> Option<u32> {
    for parent in parents {
        if let Some(child) = value.get(*parent) {
            if let Some(value) = number_field(child, keys) {
                return Some(value);
            }
        }
    }
    None
}

fn string_array_field(value: &Value, key: &str) -> Vec<String> {
    match value.get(key) {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(|value| match value {
                Value::String(text) => Some(text.trim().to_string()),
                other => Some(other.to_string()),
            })
            .filter(|value| !value.is_empty())
            .collect(),
        Some(Value::String(text)) if !text.trim().is_empty() => vec![text.trim().to_string()],
        _ => Vec::new(),
    }
}

fn finding_id(
    path: &str,
    start_line: Option<u32>,
    comment: Option<&str>,
    codegen_instructions: Option<&str>,
    index: usize,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.as_bytes());
    hasher.update(b"\0");
    hasher.update(start_line.map(|line| line.to_string()).unwrap_or_default());
    hasher.update(b"\0");
    hasher.update(comment.unwrap_or_default().as_bytes());
    hasher.update(b"\0");
    hasher.update(codegen_instructions.unwrap_or_default().as_bytes());
    hasher.update(b"\0");
    hasher.update(index.to_string().as_bytes());
    let hash = hasher.finalize();
    format!("crf-{}", hex_prefix(&hash, 12))
}

fn hex_prefix(bytes: &[u8], chars: usize) -> String {
    let mut out = String::with_capacity(chars);
    for byte in bytes {
        if out.len() >= chars {
            break;
        }
        out.push_str(&format!("{byte:02x}"));
    }
    out.truncate(chars);
    out
}

#[cfg(test)]
mod tests {
    use super::parse_agent_jsonl;
    use crate::commands::coderabbit::CodeRabbitFindingSeverity;

    #[test]
    fn parses_findings_and_complete_events() {
        let output = r#"{"type":"review_context","base":"main"}
{"type":"finding","severity":"critical","fileName":"src/auth.ts","startLine":48,"comment":"Token never expires","codegenInstructions":"Add expiry handling","suggestions":["Use exp"]}
{"type":"status","status":"running","message":"Reviewing files"}
{"type":"complete","status":"complete","findings":1,"message":"Done"}"#;

        let parsed = parse_agent_jsonl(output);

        assert_eq!(parsed.findings.len(), 1);
        assert_eq!(parsed.findings[0].path, "src/auth.ts");
        assert_eq!(parsed.findings[0].start_line, Some(48));
        assert_eq!(
            parsed.findings[0].severity,
            CodeRabbitFindingSeverity::Critical
        );
        assert_eq!(parsed.statuses.len(), 1);
        assert_eq!(parsed.complete.unwrap().findings, Some(1));
    }

    #[test]
    fn records_invalid_json_lines_without_failing_parse() {
        let parsed = parse_agent_jsonl("not json\n{\"type\":\"complete\",\"findings\":0}");

        assert_eq!(parsed.event_count, 1);
        assert_eq!(parsed.errors.len(), 1);
        assert_eq!(parsed.complete.unwrap().findings, Some(0));
    }
}
