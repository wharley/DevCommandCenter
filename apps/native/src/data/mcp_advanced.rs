//! The advanced MCP panel (`mcp-integration-form.ts`,
//! `mcp-integration-runtime.ts`): a server added by hand (HTTP or a local
//! command, its scope and credentials) checked like the desktop does, what
//! each one reported in the open session, the tools and their policies,
//! and how far the session's provider supports connectors.

use dcc_core::domain::provider::McpSupportLevel;
use serde::Deserialize;
use serde_json::{Value, json};

use super::connectors::{Integration, Tool};
use super::desktop_api::{ApiError, DesktopApi};

/// The open session the panel reads against.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionContext {
    pub project_id: Option<String>,
    pub session_id: Option<String>,
    pub provider_id: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TransportDraft {
    #[default]
    Http,
    Stdio,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScopeDraft {
    Session,
    Project,
    #[default]
    Global,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CredentialDraft {
    pub name: String,
    pub secret: String,
}

/// `McpIntegrationDraft`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Draft {
    pub display_name: String,
    pub transport: TransportDraft,
    pub url: String,
    pub executable: String,
    pub args_text: String,
    pub cwd: String,
    pub scope: ScopeDraft,
    pub credentials: Vec<CredentialDraft>,
}

impl Draft {
    /// `createMcpIntegrationDraft`: the project's scope when there is one.
    pub fn new(context: &SessionContext) -> Self {
        Self {
            scope: if context.project_id.is_some() {
                ScopeDraft::Project
            } else {
                ScopeDraft::Global
            },
            ..Self::default()
        }
    }
}

/// `McpIntegrationDraftError`: `settings.integrations.errors.<key>`.
pub fn error_key(error: &str) -> String {
    format!("settings.integrations.errors.{error}")
}

fn header_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
}

fn environment_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn http_url(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once("://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    matches!(scheme.to_lowercase().as_str(), "http" | "https")
        && !authority.is_empty()
        && !authority.contains('@')
        && !authority.contains(char::is_whitespace)
}

/// `buildMcpIntegrationInput`: the create body, or the first error's key.
pub fn build_input(draft: &Draft, context: &SessionContext) -> Result<Value, &'static str> {
    let display_name = draft.display_name.trim();
    if display_name.is_empty() {
        return Err("displayName");
    }
    let http = draft.transport == TransportDraft::Http;
    let transport = if http {
        let url = draft.url.trim();
        if !http_url(url) {
            return Err("url");
        }
        json!({ "type": "http", "url": url })
    } else {
        let executable = draft.executable.trim();
        if executable.is_empty() || executable.contains('\0') {
            return Err("executable");
        }
        let args: Vec<&str> = draft
            .args_text
            .lines()
            .map(str::trim)
            .filter(|argument| !argument.is_empty())
            .collect();
        let cwd = Some(draft.cwd.trim()).filter(|cwd| !cwd.is_empty());
        json!({ "type": "stdio", "executable": executable, "args": args, "cwd": cwd })
    };
    let scope = match draft.scope {
        ScopeDraft::Session => match &context.session_id {
            Some(session) => json!({ "type": "session", "sessionId": session }),
            None => return Err("scope"),
        },
        ScopeDraft::Project => match &context.project_id {
            Some(project) => json!({ "type": "project", "projectId": project }),
            None => return Err("scope"),
        },
        ScopeDraft::Global => json!({ "type": "global" }),
    };
    let mut credentials = Vec::new();
    let mut names: Vec<String> = Vec::new();
    for credential in &draft.credentials {
        let name = credential.name.trim();
        if name.is_empty() && credential.secret.is_empty() {
            continue;
        }
        if name.is_empty() || credential.secret.is_empty() {
            return Err("credentialPair");
        }
        let valid = if http {
            header_name(name)
        } else {
            environment_name(name)
        };
        if !valid {
            return Err("credentialName");
        }
        let normalized = if http {
            name.to_lowercase()
        } else {
            name.to_string()
        };
        if names.contains(&normalized) {
            return Err("duplicateCredential");
        }
        names.push(normalized);
        let target = if http {
            json!({ "type": "httpHeader", "name": name })
        } else {
            json!({ "type": "environmentVariable", "name": name })
        };
        credentials.push(json!({ "target": target, "secret": credential.secret }));
    }
    Ok(json!({
        "displayName": display_name,
        "transport": transport,
        "scope": scope,
        "credentials": credentials,
    }))
}

/// `formatMcpTransportPreview`: the URL, or the command as JSON strings.
pub fn transport_preview(integration: &Integration) -> String {
    let transport = &integration.definition.transport;
    if transport.kind == "http" {
        return transport.url.clone().unwrap_or_default();
    }
    std::iter::once(transport.executable.clone().unwrap_or_default())
        .chain(transport.args.iter().cloned())
        .map(|part| serde_json::to_string(&part).unwrap_or_default())
        .collect::<Vec<_>>()
        .join(" ")
}

/// `integrationScope`: the first binding's, else global.
pub fn scope_of(integration: &Integration) -> (&str, Option<&str>) {
    match integration.bindings.first() {
        Some(binding) => (
            binding.scope.kind.as_str(),
            binding
                .scope
                .session_id
                .as_deref()
                .or(binding.scope.project_id.as_deref()),
        ),
        None => ("global", None),
    }
}

/// `scopeLabelKey`
pub fn scope_key(kind: &str) -> &'static str {
    match kind {
        "session" => "settings.integrations.scopeSession",
        "project" => "settings.integrations.scopeProject",
        _ => "settings.integrations.scopeGlobal",
    }
}

/// `McpRuntimeStatus`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RuntimeStatus {
    pub definition_id: String,
    pub provider_id: String,
    pub provider_version: String,
    pub session_id: String,
    /// `connected`, `failed`, `probingServer`…
    pub state: String,
    pub tools: Vec<Tool>,
    pub checked_at: String,
    pub bounded_error: Option<RuntimeError>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RuntimeError {
    pub message: String,
}

fn timestamp(value: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(value).map_or(0, |at| at.timestamp_millis())
}

/// `deriveMcpIntegrationRuntimeView`: `settings.integrations.runtime.<kind>`
/// and the status reported, if any.
pub fn runtime_view<'a>(
    integration: &Integration,
    statuses: &'a [RuntimeStatus],
    context: &SessionContext,
    support: Option<&McpSupportLevel>,
) -> (String, Option<&'a RuntimeStatus>) {
    let (Some(session), Some(provider), Some(created_at)) = (
        context.session_id.as_deref(),
        context.provider_id.as_deref(),
        context.created_at.as_deref(),
    ) else {
        return ("noSession".into(), None);
    };
    let matching: Vec<_> = integration
        .bindings
        .iter()
        .filter(|binding| binding.enabled)
        .filter(|binding| match binding.scope.kind.as_str() {
            "global" => true,
            "project" => binding.scope.project_id.as_deref() == context.project_id.as_deref(),
            _ => binding.scope.session_id.as_deref() == Some(session),
        })
        .collect();
    if matching.is_empty() {
        return ("outOfScope".into(), None);
    }
    let applicable: Vec<_> = matching
        .into_iter()
        .filter(|binding| {
            !binding
                .provider_exclusions
                .iter()
                .any(|excluded| excluded == provider)
        })
        .collect();
    if applicable.is_empty() {
        return ("providerExcluded".into(), None);
    }
    let status = statuses.iter().find(|status| {
        status.definition_id == integration.definition.id
            && status.session_id == session
            && status.provider_id == provider
    });
    let latest = std::iter::once(timestamp(&integration.definition.updated_at))
        .chain(
            applicable
                .iter()
                .map(|binding| timestamp(&binding.updated_at)),
        )
        .chain(
            integration
                .tool_policies
                .iter()
                .map(|policy| timestamp(&policy.updated_at)),
        )
        .max()
        .unwrap_or(0);
    if let Some(status) = status {
        if latest > timestamp(&status.checked_at) {
            return ("restartRequired".into(), Some(status));
        }
        return (status.state.clone(), Some(status));
    }
    if integration.needs_trust() {
        return ("needsTrust".into(), None);
    }
    if !integration.definition.enabled {
        let kind = if latest > timestamp(created_at) {
            "restartRequired"
        } else {
            "disabled"
        };
        return (kind.into(), None);
    }
    if latest > timestamp(created_at) {
        return ("restartRequired".into(), None);
    }
    if matches!(support, Some(McpSupportLevel::Unsupported)) {
        return ("unsupported".into(), None);
    }
    ("notReported".into(), None)
}

/// `runtimeStatusVariant`: green, red, amber or quiet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Success,
    Destructive,
    Warn,
    Secondary,
    Outline,
}

pub fn runtime_tone(kind: &str) -> Tone {
    match kind {
        "connected" | "serverReachable" => Tone::Success,
        "failed" => Tone::Destructive,
        "restartRequired" | "needsTrust" | "probingServer" | "attachingProvider" => Tone::Warn,
        "disabled" => Tone::Secondary,
        _ => Tone::Outline,
    }
}

/// `findOrphanMcpRuntimeStatuses`: reported for connectors no longer here.
pub fn orphans<'a>(
    integrations: &[Integration],
    statuses: &'a [RuntimeStatus],
) -> Vec<&'a RuntimeStatus> {
    statuses
        .iter()
        .filter(|status| {
            !integrations
                .iter()
                .any(|integration| integration.definition.id == status.definition_id)
        })
        .collect()
}

/// `listMcpIntegrationTools`: reported and with a policy, sorted.
pub fn tool_names(integration: &Integration, reported: &[Tool]) -> Vec<String> {
    let mut names: Vec<String> = reported
        .iter()
        .map(|tool| tool.name.clone())
        .chain(
            integration
                .tool_policies
                .iter()
                .map(|policy| policy.tool_name.clone()),
        )
        .collect();
    names.sort();
    names.dedup();
    names
}

/// `summarizeMcpToolPolicies`: (total, ask, allow, deny).
pub fn policy_summary(integration: &Integration, tools: &[String]) -> (usize, usize, usize, usize) {
    let mut summary = (tools.len(), 0, 0, 0);
    for tool in tools {
        match integration.tool_decision(tool) {
            "allow" => summary.2 += 1,
            "deny" => summary.3 += 1,
            _ => summary.1 += 1,
        }
    }
    summary
}

/// `providerSupportKind` and the bridge's version.
pub fn support_kind(support: Option<&McpSupportLevel>) -> (&'static str, Option<String>) {
    match support {
        Some(McpSupportLevel::VerifiedBridge { .. }) => ("verified", None),
        Some(McpSupportLevel::RuntimeBridge { provider_version }) => {
            ("runtime", Some(provider_version.clone()))
        }
        Some(McpSupportLevel::NativeConfig) => ("native", None),
        Some(McpSupportLevel::Unsupported) => ("unsupported", None),
        None => ("unknown", None),
    }
}

impl DesktopApi {
    /// `disableMcpIntegration`
    pub fn disable_connector(&self, id: &str) -> Result<(), ApiError> {
        self.post(
            "/api/v1/mcp/integrations/disable",
            &json!({ "definitionId": id }),
        )
        .map(|_| ())
    }

    /// `removeMcpIntegration`, keeping the credentials unless asked.
    pub fn remove_integration(&self, id: &str, delete_credentials: bool) -> Result<(), ApiError> {
        self.post(
            "/api/v1/mcp/integrations/remove",
            &json!({ "definitionId": id, "deleteCredentials": delete_credentials }),
        )
        .map(|_| ())
    }

    /// `createMcpIntegration` with the advanced form's body.
    pub fn create_integration(&self, body: &Value) -> Result<Integration, ApiError> {
        let value = self.post("/api/v1/mcp/integrations", body)?;
        serde_json::from_value(value.get("integration").cloned().unwrap_or(Value::Null))
            .map_err(|error| ApiError::Failed(error.to_string()))
    }

    /// `loadMcpRuntimeStatuses`
    pub fn runtime_statuses(&self, session_id: &str) -> Result<Vec<RuntimeStatus>, ApiError> {
        let value = self.post("/api/v1/mcp/runtime", &json!({ "sessionId": session_id }))?;
        serde_json::from_value(
            value
                .get("statuses")
                .cloned()
                .unwrap_or(Value::Array(Vec::new())),
        )
        .map_err(|error| ApiError::Failed(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> SessionContext {
        SessionContext {
            project_id: Some("p-1".into()),
            session_id: Some("s-1".into()),
            provider_id: Some("codex".into()),
            created_at: Some("2026-10-27T10:00:00Z".into()),
        }
    }

    #[test]
    fn the_form_is_checked_like_the_desktop() {
        let mut draft = Draft::new(&context());
        assert_eq!(draft.scope, ScopeDraft::Project);
        assert_eq!(build_input(&draft, &context()), Err("displayName"));
        draft.display_name = " Acme ".into();
        draft.url = "ftp://x".into();
        assert_eq!(build_input(&draft, &context()), Err("url"));
        draft.url = "https://mcp.acme.dev/rpc".into();
        draft.credentials = vec![CredentialDraft {
            name: "Authorization".into(),
            secret: String::new(),
        }];
        assert_eq!(build_input(&draft, &context()), Err("credentialPair"));
        draft.credentials = vec![
            CredentialDraft {
                name: "Authorization".into(),
                secret: "a".into(),
            },
            CredentialDraft {
                name: "authorization".into(),
                secret: "b".into(),
            },
        ];
        assert_eq!(build_input(&draft, &context()), Err("duplicateCredential"));
        draft.credentials.pop();
        let body = build_input(&draft, &context()).unwrap();
        assert_eq!(
            body["scope"],
            json!({ "type": "project", "projectId": "p-1" })
        );
        assert_eq!(body["credentials"][0]["target"]["type"], "httpHeader");
        draft.transport = TransportDraft::Stdio;
        draft.executable = "/bin/mcp".into();
        draft.args_text = "--transport\n\n stdio ".into();
        draft.credentials = vec![CredentialDraft {
            name: "9BAD".into(),
            secret: "x".into(),
        }];
        assert_eq!(build_input(&draft, &context()), Err("credentialName"));
        draft.credentials = vec![CredentialDraft {
            name: "API_KEY".into(),
            secret: "x".into(),
        }];
        let body = build_input(&draft, &context()).unwrap();
        assert_eq!(body["transport"]["args"], json!(["--transport", "stdio"]));
        assert_eq!(body["transport"]["cwd"], Value::Null);
        draft.scope = ScopeDraft::Session;
        assert!(build_input(&draft, &SessionContext::default()).is_err());
    }

    #[test]
    fn runtime_views_read_like_the_desktop() {
        let integration: Integration = serde_json::from_value(json!({
            "definition": { "id": "d-1", "displayName": "Acme", "enabled": true,
                "updatedAt": "2026-10-27T09:00:00Z",
                "transport": { "type": "stdio", "executable": "/bin/mcp", "args": ["-v"] },
                "trust": { "currentFingerprint": "f", "decision": { "type": "trusted", "fingerprint": "f" } } },
            "bindings": [{ "scope": { "type": "global" }, "enabled": true, "updatedAt": "2026-10-27T09:00:00Z" }],
            "toolPolicies": [{ "toolName": "write", "decision": "deny", "updatedAt": "2026-10-27T09:00:00Z" }],
        }))
        .unwrap();
        assert_eq!(transport_preview(&integration), r#""/bin/mcp" "-v""#);
        let (kind, _) = runtime_view(&integration, &[], &context(), None);
        assert_eq!(kind, "notReported");
        let (kind, _) = runtime_view(&integration, &[], &SessionContext::default(), None);
        assert_eq!(kind, "noSession");
        let status = RuntimeStatus {
            definition_id: "d-1".into(),
            provider_id: "codex".into(),
            session_id: "s-1".into(),
            state: "connected".into(),
            checked_at: "2026-10-27T11:00:00Z".into(),
            tools: vec![Tool {
                name: "search".into(),
                ..Tool::default()
            }],
            ..RuntimeStatus::default()
        };
        let statuses = [status];
        let (kind, found) = runtime_view(&integration, &statuses, &context(), None);
        assert_eq!(kind, "connected");
        let tools = tool_names(&integration, &found.unwrap().tools);
        assert_eq!(tools, ["search", "write"]);
        assert_eq!(policy_summary(&integration, &tools), (2, 1, 0, 1));
        assert_eq!(runtime_tone("probingServer"), Tone::Warn);
        assert_eq!(orphans(&[], &statuses).len(), 1);
        assert_eq!(
            support_kind(Some(&McpSupportLevel::NativeConfig)).0,
            "native"
        );
    }
}
