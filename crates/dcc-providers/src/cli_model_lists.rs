//! Asking the Claude and Grok CLIs which models they offer an account.
//! Claude: its stream-json control protocol answers `initialize` with the
//! models (and `list_models` when it does not), with no MCP servers, hooks
//! or saved session (as Monocode does). Grok: `grok models` (as Synara does).

use std::path::Path;
use std::process::Stdio;

use dcc_core::domain::provider::ProviderModelDescriptor;
use dcc_core::ports::ProviderRuntimeConfig;
use dcc_core::{CoreError, Result};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

use crate::common::{apply_claude_account_environment, augmented_path, resolve_runtime_home_path};

/// The Claude CLI's model rows for this account.
pub(crate) async fn claude_model_rows(
    executable: &Path,
    runtime: Option<&ProviderRuntimeConfig>,
) -> Result<Vec<Value>> {
    let mut command = Command::new(executable);
    command
        .args([
            "--print",
            "--verbose",
            "--no-session-persistence",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--strict-mcp-config",
            "--mcp-config",
            r#"{"mcpServers":{}}"#,
            "--settings",
            r#"{"disableAllHooks":true}"#,
        ])
        .current_dir(std::env::temp_dir())
        .env("PATH", augmented_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    apply_claude_account_environment(&mut command, runtime);
    let failed = |error: String| CoreError::Provider(format!("Claude model list: {error}"));
    let mut child = command.spawn().map_err(|error| failed(error.to_string()))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| failed("no stdin".into()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| failed("no stdout".into()))?;
    let mut lines = BufReader::new(stdout).lines();
    let send = |request: &str, subtype: &str| {
        format!(
            "{}\n",
            json!({ "type": "control_request", "request_id": request, "request": { "subtype": subtype } })
        )
    };
    stdin
        .write_all(send("init-models", "initialize").as_bytes())
        .await
        .map_err(|error| failed(error.to_string()))?;
    stdin
        .flush()
        .await
        .map_err(|error| failed(error.to_string()))?;
    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|error| failed(error.to_string()))?
    {
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let response = &message["response"];
        let request = response["request_id"].as_str();
        if !matches!(request, Some("init-models" | "list-models")) {
            continue;
        }
        if response["subtype"] != "success" {
            return Err(failed(
                response["error"]
                    .as_str()
                    .unwrap_or("not answered")
                    .to_string(),
            ));
        }
        let models = response["response"]["models"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if !models.is_empty() || request == Some("list-models") {
            return Ok(models);
        }
        stdin
            .write_all(send("list-models", "list_models").as_bytes())
            .await
            .map_err(|error| failed(error.to_string()))?;
        stdin
            .flush()
            .await
            .map_err(|error| failed(error.to_string()))?;
    }
    Err(failed("Claude stopped before answering".into()))
}

/// `grok models` for this account (`GROK_HOME`).
pub(crate) async fn grok_models_output(runtime: Option<&ProviderRuntimeConfig>) -> Result<String> {
    let mut command = Command::new("grok");
    command
        .args(["--no-auto-update", "models"])
        .current_dir(std::env::temp_dir())
        .env("PATH", augmented_path())
        .stdin(Stdio::null())
        .kill_on_drop(true);
    if let Some(home) = resolve_runtime_home_path(runtime.and_then(|r| r.home_path.as_deref())) {
        command.env("GROK_HOME", home);
    }
    let output = command
        .output()
        .await
        .map_err(|error| CoreError::Provider(format!("grok models: {error}")))?;
    if !output.status.success() {
        return Err(CoreError::Provider(format!(
            "grok models exited with {}",
            output.status
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `[1m]` (the 1M-context variant) off a Claude model value.
fn without_context(value: &str) -> &str {
    let value = value.trim();
    match value.len().checked_sub(4).and_then(|cut| value.get(cut..)) {
        Some(suffix) if suffix.eq_ignore_ascii_case("[1m]") => value[..value.len() - 4].trim(),
        _ => value,
    }
}

/// Claude's rows as the picker shows them: aliases (`opus`) under the full
/// id DCC runs (`claude-opus-5-5`), the "default" row only marking the
/// recommended one, hidden, disabled and update-required rows left out.
/// No effort levels where the model takes them but the CLI named none: the
/// built-in catalog fills those in.
pub(crate) fn claude_models(rows: &[Value]) -> Vec<ProviderModelDescriptor> {
    let default = rows
        .iter()
        .find(|row| row["value"] == "default")
        .and_then(|row| row["resolvedModel"].as_str())
        .map(without_context);
    let mut seen = std::collections::HashSet::new();
    rows.iter()
        .filter_map(|row| {
            let value = without_context(row["value"].as_str()?);
            if value.is_empty()
                || value == "default"
                || value.starts_with("cc-update-required")
                || row["disabled"] == true
                || row["hidden"] == true
            {
                return None;
            }
            let resolved = without_context(row["resolvedModel"].as_str().unwrap_or(""));
            let id = if value.starts_with("claude-") || !resolved.starts_with("claude-") {
                value
            } else {
                resolved
            };
            if !seen.insert(id.to_string()) {
                return None;
            }
            let effort_levels = row["supportedEffortLevels"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter(|level| crate::model_discovery::known_effort(level))
                .map(str::to_string)
                .collect();
            Some(ProviderModelDescriptor {
                id: id.to_string(),
                label: row["displayName"].as_str().unwrap_or("").trim().to_string(),
                description: row["description"].as_str().unwrap_or("").trim().to_string(),
                recommended: Some(id) == default,
                effort_levels,
            })
        })
        .collect()
}

/// `grok models`: the lines under "Available models:", the one marked
/// "(default)" recommended; "Default model:" alone when no list follows.
pub(crate) fn grok_models(output: &str) -> Vec<ProviderModelDescriptor> {
    let model = |id: &str, recommended: bool| ProviderModelDescriptor {
        id: id.to_string(),
        label: String::new(),
        description: String::new(),
        recommended,
        effort_levels: Vec::new(),
    };
    let mut models = Vec::new();
    let mut fallback = None;
    let mut listing = false;
    for line in output.lines().map(str::trim) {
        if line.is_empty() {
            if listing && !models.is_empty() {
                break;
            }
            continue;
        }
        if let Some(id) = line.strip_prefix("Default model:") {
            fallback = id.split_whitespace().next().map(str::to_string);
            continue;
        }
        if line.eq_ignore_ascii_case("Available models:") {
            listing = true;
            continue;
        }
        if !listing {
            continue;
        }
        let entry = line.trim_start_matches(['*', '-']).trim();
        let id: String = entry
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-'))
            .collect();
        if !id.is_empty() {
            models.push(model(&id, entry[id.len()..].contains("default")));
        }
    }
    if models.is_empty() {
        models.extend(fallback.map(|id| model(&id, true)));
    }
    models
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_rows_become_full_ids_with_the_default_recommended() {
        let rows = json!([
            { "value": "default", "displayName": "Default (recommended)", "resolvedModel": "claude-opus-5-5" },
            { "value": "opus", "displayName": "Opus 5.5", "resolvedModel": "claude-opus-5-5",
              "supportedEffortLevels": ["low", "max", "ultra"] },
            { "value": "opus[1m]", "displayName": "Opus 5.5 1M", "resolvedModel": "claude-opus-5-5[1m]" },
            { "value": "claude-haiku-4-5-20251001", "displayName": "Haiku 4.5",
              "resolvedModel": "claude-haiku-4-5-20251001" },
            { "value": "cc-update-required-x", "displayName": "Update" },
            { "value": "claude-old", "displayName": "Old", "hidden": true },
            { "value": "sonnet", "displayName": "Sonnet", "disabled": true }
        ]);
        let models = claude_models(rows.as_array().unwrap());
        let found: Vec<_> = models
            .iter()
            .map(|m| (m.id.as_str(), m.label.as_str(), m.recommended))
            .collect();
        assert_eq!(
            found,
            [
                ("claude-opus-5-5", "Opus 5.5", true),
                ("claude-haiku-4-5-20251001", "Haiku 4.5", false)
            ]
        );
        assert_eq!(models[0].effort_levels, ["low", "max"]);
    }

    #[test]
    fn grok_models_reads_the_available_list() {
        let output = "You are logged in with grok.com.\n\nDefault model: grok-4.7\n\nAvailable models:\n  * grok-4.7 (default)\n  - grok-4.7-build-fast\n  - grok-4.6\n\nTip: something else\n";
        let models = grok_models(output);
        let found: Vec<_> = models
            .iter()
            .map(|m| (m.id.as_str(), m.recommended))
            .collect();
        assert_eq!(
            found,
            [
                ("grok-4.7", true),
                ("grok-4.7-build-fast", false),
                ("grok-4.6", false)
            ]
        );
        let only_default = grok_models("Default model: grok-5\n");
        assert_eq!(only_default[0].id, "grok-5");
        assert!(grok_models("not signed in").is_empty());
    }
}
