//! Settings → Conectores (`connector-model.ts`, `catalog.ts`, `user-app.ts`):
//! the MCP integrations the desktop keeps, what Settings says of each
//! (working, needs reconnecting, off…), the curated catalog, the Slack app
//! the person creates, and the calls of DCC's own sign-in.

use dcc_core::domain::provider::{McpSupportLevel, ProviderDescriptor};
use serde::Deserialize;
use serde_json::{Value, json};

pub use super::connector_catalog::*;
pub use super::connector_sign_in::{Finish, Start};
use super::desktop_api::{ApiError, DesktopApi};

/// `DCC_GRANT_OWNER`: sign-ins DCC performs itself.
pub const DCC_GRANT_OWNER: &str = "dcc";

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Transport {
    #[serde(rename = "type")]
    pub kind: String,
    pub url: Option<String>,
    /// stdio: the program, its arguments and folder.
    pub executable: Option<String>,
    pub args: Vec<String>,
    pub cwd: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TrustDecision {
    #[serde(rename = "type")]
    pub kind: String,
    pub fingerprint: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Trust {
    pub current_fingerprint: String,
    pub decision: TrustDecision,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Definition {
    pub id: String,
    pub display_name: String,
    pub transport: Transport,
    pub enabled: bool,
    pub trust: Trust,
    pub updated_at: String,
    /// Where each credential goes (a header or a variable), never its value.
    pub secret_refs: Vec<SecretRef>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SecretRef {
    pub target: SecretTarget,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SecretTarget {
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Check {
    /// `connected` | `needsAuth` | `failed`
    pub state: String,
    pub tools: Vec<Tool>,
    pub message: Option<String>,
    pub checked_at: Option<String>,
}

/// `McpToolAnnotations`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Annotations {
    pub read_only_hint: Option<bool>,
    pub destructive_hint: Option<bool>,
    pub idempotent_hint: Option<bool>,
    pub open_world_hint: Option<bool>,
}

/// `McpToolSummary`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Tool {
    pub name: String,
    pub annotations: Annotations,
}

impl Tool {
    /// `listMcpToolAnnotationHints`: `settings.integrations.toolAnnotation.<hint>`.
    pub fn hints(&self) -> Vec<&'static str> {
        let hints = &self.annotations;
        [
            hints
                .read_only_hint
                .map(|on| if on { "readOnly" } else { "mayModify" }),
            hints
                .destructive_hint
                .map(|on| if on { "destructive" } else { "nonDestructive" }),
            hints
                .idempotent_hint
                .map(|on| if on { "idempotent" } else { "nonIdempotent" }),
            hints
                .open_world_hint
                .map(|on| if on { "openWorld" } else { "closedWorld" }),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BindingScope {
    #[serde(rename = "type")]
    pub kind: String,
    pub session_id: Option<String>,
    pub project_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Binding {
    pub scope: BindingScope,
    pub enabled: bool,
    pub provider_exclusions: Vec<String>,
    pub updated_at: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ToolPolicy {
    pub tool_name: String,
    /// `ask` | `allow` | `deny`
    pub decision: String,
    pub updated_at: String,
}

/// `McpIntegrationRecord` (what the panel reads of it).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Integration {
    pub definition: Definition,
    pub oauth_provider_ids: Vec<String>,
    pub connection_check: Option<Check>,
    /// A `data:` image for connectors outside the catalog.
    pub logo: Option<String>,
    pub bindings: Vec<Binding>,
    pub tool_policies: Vec<ToolPolicy>,
    pub excluded_project_ids: Vec<String>,
    pub credential_count: usize,
    /// Scopes the person chose; none uses the server's suggestion.
    pub scopes: Option<String>,
}

impl Integration {
    /// `getMcpToolPolicyDecision`: `ask` unless set.
    pub fn tool_decision(&self, tool: &str) -> &str {
        self.tool_policies
            .iter()
            .find(|policy| policy.tool_name == tool)
            .map_or("ask", |policy| policy.decision.as_str())
    }

    /// Bound everywhere: it can be switched off per project.
    pub fn global(&self) -> bool {
        self.bindings
            .iter()
            .any(|binding| binding.enabled && binding.scope.kind == "global")
    }

    /// `connectorUrl`
    pub fn url(&self) -> Option<&str> {
        (self.definition.transport.kind == "http")
            .then_some(self.definition.transport.url.as_deref())
            .flatten()
    }

    /// `isSignedIn`
    pub fn signed_in(&self) -> bool {
        self.oauth_provider_ids
            .iter()
            .any(|owner| owner == DCC_GRANT_OWNER)
    }

    /// `mcpIntegrationNeedsTrust`
    pub fn needs_trust(&self) -> bool {
        let trust = &self.definition.trust;
        trust.decision.kind == "untrusted"
            || trust.decision.fingerprint.as_deref() != Some(trust.current_fingerprint.as_str())
    }
}

/// `ConnectorStatus`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Connected,
    NeedsAuth,
    Failed,
    Off,
    Unchecked,
}

/// `ConnectorView`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub integration: Integration,
    pub preset: Option<&'static Preset>,
    pub name: String,
    pub status: Status,
    pub tool_count: usize,
    pub remote: bool,
    pub signed_in: bool,
}

impl View {
    /// `deriveConnectorView`
    pub fn of(integration: &Integration) -> Self {
        let url = integration.url();
        let preset = url.and_then(preset_for_url);
        let check = integration.connection_check.as_ref();
        let enabled = integration.definition.enabled && !integration.needs_trust();
        // Signed in only through a provider's own login (the old path).
        let legacy =
            url.is_some() && !integration.signed_in() && !integration.oauth_provider_ids.is_empty();
        let state = check.map(|check| check.state.as_str());
        let status = if !enabled {
            Status::Off
        } else if legacy || state == Some("needsAuth") {
            Status::NeedsAuth
        } else if state == Some("failed") {
            Status::Failed
        } else if state == Some("connected") || integration.signed_in() {
            Status::Connected
        } else {
            Status::Unchecked
        };
        Self {
            integration: integration.clone(),
            preset,
            name: integration.definition.display_name.clone(),
            status,
            tool_count: check.map_or(0, |check| check.tools.len()),
            remote: url.is_some(),
            signed_in: integration.signed_in(),
        }
    }

    pub fn id(&self) -> &str {
        &self.integration.definition.id
    }

    /// The logo a non-catalog connector carries.
    pub fn logo_uri(&self) -> Option<&str> {
        self.preset
            .is_none()
            .then_some(self.integration.logo.as_deref())
            .flatten()
    }
}

/// `sortConnectorViews`: the ones that need the person first, then by name.
pub fn sorted_views(integrations: &[Integration]) -> Vec<View> {
    let mut views: Vec<View> = integrations.iter().map(View::of).collect();
    views.sort_by(|left, right| {
        let rank = |view: &View| u8::from(view.status != Status::NeedsAuth);
        rank(left)
            .cmp(&rank(right))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    views
}

/// `findIntegrationForUrl`
pub fn integration_for_url<'a>(
    integrations: &'a [Integration],
    url: &str,
) -> Option<&'a Integration> {
    integrations.iter().find(|integration| {
        integration
            .url()
            .is_some_and(|existing| same_url(existing, url))
    })
}

/// `providersWithoutConnectors`: enabled providers that cannot receive them.
pub fn providers_without_connectors(providers: &[ProviderDescriptor]) -> Vec<String> {
    providers
        .iter()
        .filter(|provider| provider.enabled)
        .filter(|provider| {
            !matches!(
                provider.capabilities.mcp_support,
                McpSupportLevel::RuntimeBridge { .. } | McpSupportLevel::VerifiedBridge { .. }
            )
        })
        .map(|provider| provider.label.clone())
        .collect()
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
}

fn integration_of(value: Value) -> Result<Integration, ApiError> {
    decode(value.get("integration").cloned().unwrap_or(Value::Null))
}

impl DesktopApi {
    /// `listMcpIntegrations`
    pub fn connectors(&self) -> Result<Vec<Integration>, ApiError> {
        let value = self.get("/api/v1/mcp/integrations")?;
        decode(
            value
                .get("integrations")
                .cloned()
                .unwrap_or(Value::Array(Vec::new())),
        )
    }

    /// `createMcpIntegration` for a remote server, with a bearer token when
    /// it signs in with one.
    pub fn create_connector(
        &self,
        name: &str,
        url: &str,
        token: Option<&str>,
    ) -> Result<Integration, ApiError> {
        let credentials = match token {
            Some(token) => json!([{
                "target": { "type": "httpHeader", "name": "Authorization" },
                "secret": format!("Bearer {token}"),
            }]),
            None => json!([]),
        };
        integration_of(self.post(
            "/api/v1/mcp/integrations",
            &json!({
                "displayName": name,
                "transport": { "type": "http", "url": url },
                "scope": { "type": "global" },
                "credentials": credentials,
            }),
        )?)
    }

    /// `activateMcpIntegration`: clicking Connect is the consent.
    pub fn activate_connector(&self, integration: &Integration) -> Result<Integration, ApiError> {
        integration_of(self.post(
            "/api/v1/mcp/integrations/activate",
            &json!({
                "definitionId": integration.definition.id,
                "expectedFingerprint": integration.definition.trust.current_fingerprint,
            }),
        )?)
    }

    /// `removeMcpIntegration` (its credentials too).
    pub fn remove_connector(&self, id: &str) -> Result<(), ApiError> {
        self.post(
            "/api/v1/mcp/integrations/remove",
            &json!({ "definitionId": id, "deleteCredentials": true }),
        )
        .map(|_| ())
    }

    /// `setMcpConnectorScopes`
    pub fn set_connector_scopes(&self, id: &str, scopes: &str) -> Result<(), ApiError> {
        self.post(
            "/api/v1/mcp/scopes",
            &json!({ "definitionId": id, "scopes": scopes }),
        )
        .map(|_| ())
    }

    /// `disconnectMcpConnector`
    pub fn disconnect_connector(&self, id: &str) -> Result<(), ApiError> {
        self.post("/api/v1/mcp/disconnect", &json!({ "definitionId": id }))
            .map(|_| ())
    }

    /// `testMcpIntegration`
    pub fn test_connector(&self, id: &str) -> Result<Check, ApiError> {
        let value = self.post("/api/v1/mcp/test", &json!({ "definitionId": id }))?;
        decode(value.get("check").cloned().unwrap_or(Value::Null))
    }

    /// `applyNewConnectorDetails`: the scopes chosen (or the preset's), and
    /// the uploaded logo — without one, the site's icon is looked for later.
    fn apply_new_details(
        &self,
        integration: &Integration,
        target: &Target,
    ) -> Result<(), ApiError> {
        let id = &integration.definition.id;
        let scopes = target
            .scopes
            .clone()
            .filter(|scopes| !scopes.trim().is_empty())
            .or_else(|| target.preset.and_then(Preset::scopes));
        if let Some(scopes) = scopes {
            self.set_connector_scopes(id, &scopes)?;
        }
        if let Some(logo) = &target.logo {
            self.set_connector_logo(id, Some(logo))?;
        } else if target.preset.is_none() {
            let (api, id) = (self.clone(), id.clone());
            std::thread::spawn(move || {
                let _ = api.fetch_connector_logo(&id);
            });
        }
        Ok(())
    }

    /// `ensureConnectorIntegration`: an enabled, trusted integration for the
    /// target, reusing one with the same URL (a token always makes a new one).
    pub fn ensure_connector(
        &self,
        target: &Target,
        integrations: &[Integration],
        token: Option<&str>,
    ) -> Result<Integration, ApiError> {
        let existing = token
            .is_none()
            .then(|| integration_for_url(integrations, &target.url).cloned())
            .flatten();
        let mut integration = match existing {
            Some(integration) => integration,
            None => {
                let created = self.create_connector(&target.name, &target.url, token)?;
                self.apply_new_details(&created, target)?;
                created
            }
        };
        if !integration.definition.enabled || integration.needs_trust() {
            integration = self.activate_connector(&integration)?;
        }
        Ok(integration)
    }
}

/// `ConnectorTarget`: what the person is connecting.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Target {
    pub name: String,
    pub url: String,
    pub preset: Option<&'static Preset>,
    /// A custom connector's scopes instead of the server's.
    pub scopes: Option<String>,
    /// A custom connector's uploaded logo (`data:` URI).
    pub logo: Option<String>,
}

impl Target {
    pub fn of_preset(preset: &'static Preset) -> Self {
        Self {
            name: preset.name.into(),
            url: preset.url.into(),
            preset: Some(preset),
            ..Self::default()
        }
    }

    pub fn auth(&self) -> Auth {
        self.preset.map_or(Auth::OAuth, |preset| preset.auth)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn integration(value: Value) -> Integration {
        serde_json::from_value(value).unwrap()
    }

    fn record(url: &str, enabled: bool, check: Value, owners: &[&str]) -> Integration {
        integration(json!({
            "definition": { "id": url, "displayName": url, "enabled": enabled,
                "transport": { "type": "http", "url": url },
                "trust": { "currentFingerprint": "f", "decision": { "type": "trusted", "fingerprint": "f" } } },
            "oauthProviderIds": owners, "connectionCheck": check,
        }))
    }

    #[test]
    fn statuses_and_order_read_like_the_desktop() {
        let linear = record(
            "https://mcp.linear.app/mcp/",
            true,
            json!({ "state": "connected", "tools": [{}, {}] }),
            &["dcc"],
        );
        let view = View::of(&linear);
        assert_eq!(view.status, Status::Connected);
        assert_eq!(view.tool_count, 2);
        assert_eq!(view.preset.unwrap().id, "linear");
        let off = record("https://x.dev/mcp", false, Value::Null, &[]);
        assert_eq!(View::of(&off).status, Status::Off);
        // Signed in only by a provider's own login: reconnect.
        let legacy = record("https://y.dev/mcp", true, Value::Null, &["claude_code"]);
        assert_eq!(View::of(&legacy).status, Status::NeedsAuth);
        assert_eq!(
            View::of(&record("https://z.dev", true, Value::Null, &[])).status,
            Status::Unchecked
        );
        let ids: Vec<String> = sorted_views(&[off.clone(), linear.clone(), legacy.clone()])
            .into_iter()
            .map(|view| view.id().to_string())
            .collect();
        assert_eq!(ids[0], "https://y.dev/mcp");
        assert!(integration_for_url(&[linear], "HTTPS://MCP.LINEAR.APP/mcp#x").is_some());
        assert!(same_url("https://a.com/mcp/?x=1", "https://A.com/mcp?x=1"));
        assert!(!same_url("https://a.com/mcp", "https://a.com/other"));
    }

    #[test]
    fn the_catalog_and_the_slack_app() {
        assert_eq!(visible_presets().count(), 6);
        assert_eq!(
            preset_for_url("https://mcp.slack.com/mcp").unwrap().auth,
            Auth::SlackApp
        );
        assert!(
            PRESETS[5]
                .scopes()
                .unwrap()
                .starts_with("search:read.public")
        );
        let url = slack_create_app_url();
        assert!(url.starts_with("https://api.slack.com/apps?new_app=1&manifest_json=%7B"));
        assert!(url.contains("46327"));
        let untrusted = integration(json!({ "definition": { "trust": {
            "currentFingerprint": "b", "decision": { "type": "trusted", "fingerprint": "a" } } } }));
        assert!(untrusted.needs_trust());
    }
}
