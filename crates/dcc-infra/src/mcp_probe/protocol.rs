use dcc_core::{
    domain::mcp::{McpErrorCategory, McpRuntimeError, McpToolSummary},
    ports::McpProbeResult,
};
use serde_json::{json, Value};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

use super::{probe_error, McpProbeLimits};

pub(super) const PROTOCOL_VERSION: &str = "2025-11-25";
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26"];
const MAX_IGNORED_MESSAGES: usize = 32;

pub(super) fn initialize_request() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {
                "name": "dev-command-center",
                "version": env!("CARGO_PKG_VERSION")
            }
        }
    })
}

pub(super) fn initialized_notification() -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    })
}

pub(super) fn list_tools_request() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    })
}

pub(super) fn parse_initialize_response(value: Value) -> McpProbeResult<String> {
    let result = response_result(value, 1)?;
    let version = result
        .get("protocolVersion")
        .and_then(Value::as_str)
        .filter(|version| SUPPORTED_PROTOCOL_VERSIONS.contains(version))
        .ok_or_else(|| {
            probe_error(
                McpErrorCategory::Protocol,
                "MCP server negotiated an unsupported protocol version",
            )
        })?;
    if !result.get("capabilities").is_some_and(Value::is_object)
        || !result.get("serverInfo").is_some_and(Value::is_object)
    {
        return Err(probe_error(
            McpErrorCategory::Protocol,
            "MCP initialize response is invalid",
        ));
    }
    Ok(version.to_string())
}

pub(super) fn parse_tools_response(
    value: Value,
    limits: &McpProbeLimits,
) -> McpProbeResult<Vec<McpToolSummary>> {
    let result = response_result(value, 2)?;
    let tools = result
        .get("tools")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            probe_error(
                McpErrorCategory::Protocol,
                "MCP tools/list response is invalid",
            )
        })?;
    if tools.len() > limits.max_tools {
        return Err(probe_error(
            McpErrorCategory::Protocol,
            "MCP server returned too many tools",
        ));
    }

    let mut summaries = Vec::with_capacity(tools.len());
    let mut names = std::collections::HashSet::new();
    for tool in tools {
        let name = tool.get("name").and_then(Value::as_str).ok_or_else(|| {
            probe_error(
                McpErrorCategory::Protocol,
                "MCP server returned a tool without a valid name",
            )
        })?;
        if name.is_empty()
            || name.chars().count() > limits.max_tool_name_chars
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
            || !tool.get("inputSchema").is_some_and(Value::is_object)
            || !names.insert(name)
        {
            return Err(probe_error(
                McpErrorCategory::Protocol,
                "MCP server returned an invalid tool definition",
            ));
        }
        summaries.push(McpToolSummary {
            name: name.to_string(),
        });
    }
    Ok(summaries)
}

pub(super) async fn write_message<W>(writer: &mut W, message: &Value) -> McpProbeResult<()>
where
    W: AsyncWrite + Unpin,
{
    let encoded = serde_json::to_vec(message).map_err(|_| {
        probe_error(
            McpErrorCategory::Protocol,
            "failed to encode an MCP probe request",
        )
    })?;
    writer.write_all(&encoded).await.map_err(|_| {
        probe_error(
            McpErrorCategory::Transport,
            "failed to write to the MCP server",
        )
    })?;
    writer.write_all(b"\n").await.map_err(|_| {
        probe_error(
            McpErrorCategory::Transport,
            "failed to write to the MCP server",
        )
    })?;
    writer.flush().await.map_err(|_| {
        probe_error(
            McpErrorCategory::Transport,
            "failed to write to the MCP server",
        )
    })
}

pub(super) async fn read_response<R>(
    reader: &mut R,
    expected_id: u64,
    max_bytes: usize,
) -> McpProbeResult<Value>
where
    R: AsyncBufRead + Unpin,
{
    for _ in 0..MAX_IGNORED_MESSAGES {
        let value = read_json_line(reader, max_bytes).await?;
        if value.get("id").and_then(Value::as_u64) == Some(expected_id) {
            return Ok(value);
        }
    }
    Err(probe_error(
        McpErrorCategory::Protocol,
        "MCP server sent too many unrelated messages",
    ))
}

async fn read_json_line<R>(reader: &mut R, max_bytes: usize) -> McpProbeResult<Value>
where
    R: AsyncBufRead + Unpin,
{
    let mut line = Vec::new();
    loop {
        let (take, found_newline, reached_eof) = {
            let available = reader.fill_buf().await.map_err(|_| {
                probe_error(
                    McpErrorCategory::Transport,
                    "failed to read from the MCP server",
                )
            })?;
            if available.is_empty() {
                (0, false, true)
            } else if let Some(position) = available.iter().position(|byte| *byte == b'\n') {
                if line.len() + position > max_bytes {
                    return Err(response_too_large());
                }
                line.extend_from_slice(&available[..position]);
                (position + 1, true, false)
            } else {
                if line.len() + available.len() > max_bytes {
                    return Err(response_too_large());
                }
                line.extend_from_slice(available);
                (available.len(), false, false)
            }
        };
        reader.consume(take);

        if found_newline {
            break;
        }
        if reached_eof {
            if line.is_empty() {
                return Err(probe_error(
                    McpErrorCategory::Transport,
                    "MCP server exited before responding",
                ));
            }
            break;
        }
    }

    if line.last() == Some(&b'\r') {
        line.pop();
    }
    serde_json::from_slice(&line).map_err(|_| {
        probe_error(
            McpErrorCategory::Protocol,
            "MCP server returned malformed JSON",
        )
    })
}

pub(super) fn response_result(value: Value, expected_id: u64) -> McpProbeResult<Value> {
    if value.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
        || value.get("id").and_then(Value::as_u64) != Some(expected_id)
    {
        return Err(probe_error(
            McpErrorCategory::Protocol,
            "MCP server returned an invalid JSON-RPC response",
        ));
    }
    if value.get("error").is_some() {
        return Err(probe_error(
            McpErrorCategory::Protocol,
            "MCP server returned a protocol error",
        ));
    }
    value.get("result").cloned().ok_or_else(|| {
        probe_error(
            McpErrorCategory::Protocol,
            "MCP server response did not contain a result",
        )
    })
}

pub(super) fn response_too_large() -> McpRuntimeError {
    probe_error(
        McpErrorCategory::Protocol,
        "MCP server response exceeded the size limit",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_parser_returns_only_bounded_names() {
        let tools = parse_tools_response(
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "result": {
                    "tools": [{
                        "name": "fixture.echo",
                        "description": "ignored",
                        "inputSchema": { "type": "object" }
                    }]
                }
            }),
            &McpProbeLimits::default(),
        )
        .expect("parse tools");

        assert_eq!(
            tools,
            vec![McpToolSummary {
                name: "fixture.echo".to_string()
            }]
        );
    }

    #[test]
    fn tool_parser_rejects_payload_shaped_names_and_duplicates() {
        let error = parse_tools_response(
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "result": {
                    "tools": [
                        { "name": "bad\nsecret", "inputSchema": {} },
                        { "name": "bad\nsecret", "inputSchema": {} }
                    ]
                }
            }),
            &McpProbeLimits::default(),
        )
        .expect_err("invalid tools");

        assert_eq!(error.category, McpErrorCategory::Protocol);
        assert!(!error.message.contains("secret"));
    }

    #[tokio::test]
    async fn line_reader_rejects_oversized_messages_without_unbounded_storage() {
        let input = vec![b'a'; 2_048];
        let mut reader = tokio::io::BufReader::new(input.as_slice());
        let error = read_json_line(&mut reader, 1_024)
            .await
            .expect_err("oversized response");

        assert_eq!(error.category, McpErrorCategory::Protocol);
    }
}
