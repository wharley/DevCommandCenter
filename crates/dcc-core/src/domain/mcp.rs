use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use thiserror::Error;
use url::Url;

use super::{project::ProjectId, provider::ProviderId, session::SessionId};

const MAX_RUNTIME_ERROR_CHARS: usize = 512;
const MAX_MCP_TOOL_COUNT: usize = 256;
const MAX_MCP_TOOL_NAME_CHARS: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub struct McpDefinitionId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub struct McpBindingId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub struct McpSecretReferenceId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub struct McpTrustFingerprint(pub String);

impl McpTrustFingerprint {
    pub fn validate(&self) -> Result<(), McpValidationError> {
        if self.0.len() != 64 || !self.0.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(McpValidationError::InvalidTrustFingerprint);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum McpTransportKind {
    Stdio,
    Http,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum McpTransport {
    /// A direct executable plus an argument array. DCC must not reinterpret
    /// these fields as a shell command string.
    Stdio {
        executable: String,
        args: Vec<String>,
        cwd: Option<String>,
    },
    /// Streamable HTTP endpoint. Authentication headers are represented by
    /// `McpSecretBinding`, never embedded in this URL.
    Http { url: String },
}

impl McpTransport {
    pub fn kind(&self) -> McpTransportKind {
        match self {
            Self::Stdio { .. } => McpTransportKind::Stdio,
            Self::Http { .. } => McpTransportKind::Http,
        }
    }

    fn validate(&self) -> Result<(), McpValidationError> {
        match self {
            Self::Stdio {
                executable,
                args,
                cwd,
            } => {
                validate_non_empty("transport.executable", executable)?;
                validate_no_nul("transport.executable", executable)?;
                for argument in args {
                    validate_no_nul("transport.args", argument)?;
                }
                if let Some(cwd) = cwd {
                    validate_non_empty("transport.cwd", cwd)?;
                    validate_no_nul("transport.cwd", cwd)?;
                }
                Ok(())
            }
            Self::Http { url } => validate_http_url(url),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum McpImportSourceKind {
    ProviderConfig,
    ProjectFile,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpImportSource {
    pub kind: McpImportSourceKind,
    /// Local path or provider-owned source label. It is identity metadata, not
    /// authority for DCC to modify the source.
    pub locator: String,
    pub definition_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum McpDefinitionOwnership {
    DccManaged,
    ImportedReadOnly { source: McpImportSource },
}

impl McpDefinitionOwnership {
    pub fn is_read_only(&self) -> bool {
        matches!(self, Self::ImportedReadOnly { .. })
    }

    fn validate(&self) -> Result<(), McpValidationError> {
        if let Self::ImportedReadOnly { source } = self {
            validate_non_empty("ownership.source.locator", &source.locator)?;
            if let Some(key) = &source.definition_key {
                validate_non_empty("ownership.source.definitionKey", key)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum McpSecretTarget {
    EnvironmentVariable { name: String },
    HttpHeader { name: String },
}

impl McpSecretTarget {
    fn name(&self) -> &str {
        match self {
            Self::EnvironmentVariable { name } | Self::HttpHeader { name } => name,
        }
    }

    fn identity(&self) -> String {
        match self {
            Self::EnvironmentVariable { name } => format!("env:{name}"),
            Self::HttpHeader { name } => format!("header:{}", name.to_ascii_lowercase()),
        }
    }

    fn validate(&self) -> Result<(), McpValidationError> {
        validate_non_empty("secretRefs.target.name", self.name())?;
        match self {
            Self::EnvironmentVariable { name } if !is_valid_environment_name(name) => {
                Err(McpValidationError::InvalidEnvironmentVariableName)
            }
            Self::HttpHeader { name } if !is_valid_http_header_name(name) => {
                Err(McpValidationError::InvalidHttpHeaderName)
            }
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpSecretBinding {
    pub target: McpSecretTarget,
    /// Opaque credential-store reference. This is never the secret value.
    pub secret_ref: McpSecretReferenceId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum McpTrustDecision {
    Untrusted,
    Trusted {
        fingerprint: McpTrustFingerprint,
        trusted_at: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpTrust {
    /// Fingerprint computed from all security-relevant definition fields.
    pub current_fingerprint: McpTrustFingerprint,
    pub decision: McpTrustDecision,
}

impl McpTrust {
    pub fn requires_confirmation(&self) -> bool {
        match &self.decision {
            McpTrustDecision::Untrusted => true,
            McpTrustDecision::Trusted { fingerprint, .. } => {
                fingerprint != &self.current_fingerprint
            }
        }
    }

    fn validate(&self) -> Result<(), McpValidationError> {
        self.current_fingerprint.validate()?;
        if let McpTrustDecision::Trusted {
            fingerprint,
            trusted_at,
        } = &self.decision
        {
            fingerprint.validate()?;
            validate_non_empty("trust.decision.trustedAt", trusted_at)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpDefinition {
    pub id: McpDefinitionId,
    pub display_name: String,
    pub transport: McpTransport,
    #[serde(default)]
    pub secret_refs: Vec<McpSecretBinding>,
    pub enabled: bool,
    pub ownership: McpDefinitionOwnership,
    pub trust: McpTrust,
    pub created_at: String,
    pub updated_at: String,
}

impl McpDefinition {
    /// Computes the versioned SHA-256 digest of fields that can change what
    /// the integration executes, contacts, or authenticates to.
    ///
    /// Secret values are deliberately excluded. Opaque credential references
    /// are included so replacing the credential selected by a definition
    /// requires confirmation without exposing the credential itself.
    pub fn computed_trust_fingerprint(&self) -> McpTrustFingerprint {
        let mut encoder = TrustFingerprintEncoder::new();
        encoder.string("definition-id", &self.id.0);

        match &self.transport {
            McpTransport::Stdio {
                executable,
                args,
                cwd,
            } => {
                encoder.tag("transport", "stdio");
                encoder.string("executable", executable);
                encoder.strings("args", args.iter().map(String::as_str));
                encoder.optional_string("cwd", cwd.as_deref());
            }
            McpTransport::Http { url } => {
                encoder.tag("transport", "http");
                encoder.string("url", url);
            }
        }

        match &self.ownership {
            McpDefinitionOwnership::DccManaged => {
                encoder.tag("ownership", "dcc-managed");
            }
            McpDefinitionOwnership::ImportedReadOnly { source } => {
                encoder.tag("ownership", "imported-read-only");
                encoder.tag(
                    "source-kind",
                    match source.kind {
                        McpImportSourceKind::ProviderConfig => "provider-config",
                        McpImportSourceKind::ProjectFile => "project-file",
                        McpImportSourceKind::Other => "other",
                    },
                );
                encoder.string("source-locator", &source.locator);
                encoder.optional_string("source-definition-key", source.definition_key.as_deref());
            }
        }

        let mut secret_bindings = self
            .secret_refs
            .iter()
            .map(|binding| {
                let (kind, name) = match &binding.target {
                    McpSecretTarget::EnvironmentVariable { name } => ("env", name.clone()),
                    McpSecretTarget::HttpHeader { name } => ("header", name.to_ascii_lowercase()),
                };
                (kind, name, binding.secret_ref.0.as_str())
            })
            .collect::<Vec<_>>();
        secret_bindings.sort_unstable();

        encoder.count("secret-bindings", secret_bindings.len());
        for (kind, name, secret_ref) in secret_bindings {
            encoder.tag("secret-target-kind", kind);
            encoder.string("secret-target-name", &name);
            encoder.string("secret-reference", secret_ref);
        }

        encoder.finish()
    }

    /// Refreshes the current fingerprint while deliberately retaining the
    /// prior decision. A material change therefore becomes `NeedsTrust`.
    pub fn synchronize_trust_fingerprint(&mut self) -> bool {
        let fingerprint = self.computed_trust_fingerprint();
        let changed = self.trust.current_fingerprint != fingerprint;
        self.trust.current_fingerprint = fingerprint;
        changed
    }

    pub fn validate(&self) -> Result<(), McpValidationError> {
        validate_non_empty("id", &self.id.0)?;
        validate_non_empty("displayName", &self.display_name)?;
        validate_non_empty("createdAt", &self.created_at)?;
        validate_non_empty("updatedAt", &self.updated_at)?;
        self.transport.validate()?;
        self.ownership.validate()?;
        self.trust.validate()?;

        let mut targets = HashSet::new();
        for binding in &self.secret_refs {
            validate_non_empty("secretRefs.secretRef", &binding.secret_ref.0)?;
            validate_no_nul("secretRefs.secretRef", &binding.secret_ref.0)?;
            binding.target.validate()?;

            if !targets.insert(binding.target.identity()) {
                return Err(McpValidationError::DuplicateSecretTarget);
            }
            match (&self.transport, &binding.target) {
                (McpTransport::Stdio { .. }, McpSecretTarget::HttpHeader { .. })
                | (McpTransport::Http { .. }, McpSecretTarget::EnvironmentVariable { .. }) => {
                    return Err(McpValidationError::SecretTargetTransportMismatch);
                }
                _ => {}
            }
        }

        if self.trust.current_fingerprint != self.computed_trust_fingerprint() {
            return Err(McpValidationError::TrustFingerprintMismatch);
        }

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum McpBindingScope {
    Session { session_id: SessionId },
    Project { project_id: ProjectId },
    Global,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpBinding {
    pub id: McpBindingId,
    pub definition_id: McpDefinitionId,
    pub scope: McpBindingScope,
    pub enabled: bool,
    /// Advanced escape hatch. The default is every verified compatible
    /// provider, represented by an empty list.
    #[serde(default)]
    pub provider_exclusions: Vec<ProviderId>,
    pub created_at: String,
    pub updated_at: String,
}

impl McpBinding {
    pub fn validate(&self) -> Result<(), McpValidationError> {
        validate_non_empty("id", &self.id.0)?;
        validate_non_empty("definitionId", &self.definition_id.0)?;
        validate_non_empty("createdAt", &self.created_at)?;
        validate_non_empty("updatedAt", &self.updated_at)?;

        let mut exclusions = HashSet::new();
        for provider in &self.provider_exclusions {
            validate_non_empty("providerExclusions", &provider.0)?;
            if !exclusions.insert(&provider.0) {
                return Err(McpValidationError::DuplicateProviderExclusion);
            }
        }
        match &self.scope {
            McpBindingScope::Session { session_id } => {
                validate_non_empty("scope.sessionId", &session_id.0)
            }
            McpBindingScope::Project { project_id } => {
                validate_non_empty("scope.projectId", &project_id.0)
            }
            McpBindingScope::Global => Ok(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum McpRuntimeState {
    Disabled,
    NeedsTrust,
    ProbingServer,
    ServerReachable,
    AttachingProvider,
    Connected,
    Unsupported,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum McpErrorCategory {
    InvalidDefinition,
    Authentication,
    ExecutableNotFound,
    Timeout,
    Protocol,
    Transport,
    Provider,
    PermissionBoundary,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpRuntimeError {
    pub category: McpErrorCategory,
    pub message: String,
    pub truncated: bool,
}

impl McpRuntimeError {
    /// Bounds an already-redacted message before it crosses a process or UI
    /// boundary. Callers remain responsible for redaction.
    pub fn bounded(category: McpErrorCategory, message: impl AsRef<str>) -> Self {
        let message = message.as_ref();
        let mut chars = message.chars();
        let bounded: String = chars.by_ref().take(MAX_RUNTIME_ERROR_CHARS).collect();
        let truncated = chars.next().is_some();
        Self {
            category,
            message: bounded,
            truncated,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpToolSummary {
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum McpToolPolicyDecision {
    #[default]
    Ask,
    Allow,
    Deny,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpToolPolicy {
    pub definition_id: McpDefinitionId,
    pub tool_name: String,
    pub decision: McpToolPolicyDecision,
    pub updated_at: String,
}

impl McpToolPolicy {
    pub fn validate(&self) -> Result<(), McpValidationError> {
        validate_non_empty("definitionId", &self.definition_id.0)?;
        validate_non_empty("toolName", &self.tool_name)?;
        validate_non_empty("updatedAt", &self.updated_at)?;
        if !is_valid_tool_name(&self.tool_name) {
            return Err(McpValidationError::InvalidToolName);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpProbeReport {
    pub definition_id: McpDefinitionId,
    pub transport: McpTransportKind,
    pub protocol_version: String,
    pub tools: Vec<McpToolSummary>,
    pub checked_at: String,
}

impl McpProbeReport {
    pub fn validate(&self) -> Result<(), McpValidationError> {
        validate_non_empty("definitionId", &self.definition_id.0)?;
        validate_non_empty("protocolVersion", &self.protocol_version)?;
        validate_non_empty("checkedAt", &self.checked_at)?;
        if self.tools.len() > MAX_MCP_TOOL_COUNT {
            return Err(McpValidationError::TooManyTools);
        }

        let mut tools = HashSet::new();
        for tool in &self.tools {
            validate_non_empty("tools.name", &tool.name)?;
            if !is_valid_tool_name(&tool.name) {
                return Err(McpValidationError::InvalidToolName);
            }
            if !tools.insert(&tool.name) {
                return Err(McpValidationError::DuplicateToolName);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpRuntimeStatus {
    pub definition_id: McpDefinitionId,
    pub provider_id: ProviderId,
    pub provider_version: String,
    pub session_id: SessionId,
    pub state: McpRuntimeState,
    #[serde(default)]
    pub tools: Vec<McpToolSummary>,
    pub checked_at: String,
    pub bounded_error: Option<McpRuntimeError>,
}

impl McpRuntimeStatus {
    pub fn validate(&self) -> Result<(), McpValidationError> {
        validate_non_empty("definitionId", &self.definition_id.0)?;
        validate_non_empty("providerId", &self.provider_id.0)?;
        validate_non_empty("providerVersion", &self.provider_version)?;
        validate_non_empty("sessionId", &self.session_id.0)?;
        validate_non_empty("checkedAt", &self.checked_at)?;
        if self.tools.len() > MAX_MCP_TOOL_COUNT {
            return Err(McpValidationError::TooManyTools);
        }

        let mut tools = HashSet::new();
        for tool in &self.tools {
            validate_non_empty("tools.name", &tool.name)?;
            if !is_valid_tool_name(&tool.name) {
                return Err(McpValidationError::InvalidToolName);
            }
            if !tools.insert(&tool.name) {
                return Err(McpValidationError::DuplicateToolName);
            }
        }

        match (&self.state, &self.bounded_error) {
            (McpRuntimeState::Failed, None) => Err(McpValidationError::MissingRuntimeError),
            (McpRuntimeState::Failed | McpRuntimeState::Unsupported, _) => Ok(()),
            (_, Some(_)) => Err(McpValidationError::UnexpectedRuntimeError),
            (_, None) => Ok(()),
        }?;

        if self
            .bounded_error
            .as_ref()
            .is_some_and(|error| error.message.chars().count() > MAX_RUNTIME_ERROR_CHARS)
        {
            return Err(McpValidationError::RuntimeErrorTooLong);
        }

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum McpValidationError {
    #[error("{field} must not be empty")]
    EmptyField { field: &'static str },
    #[error("{field} must not contain a NUL byte")]
    NulByte { field: &'static str },
    #[error("HTTP transport URL must use http or https")]
    UnsupportedUrlScheme,
    #[error("HTTP transport URL must include a host")]
    MissingUrlHost,
    #[error("HTTP transport URL must not contain embedded credentials")]
    EmbeddedUrlCredentials,
    #[error("HTTP transport URL is invalid")]
    InvalidUrl,
    #[error("trust fingerprint must be a 64-character hexadecimal SHA-256 digest")]
    InvalidTrustFingerprint,
    #[error("current trust fingerprint does not match the MCP definition")]
    TrustFingerprintMismatch,
    #[error("a secret target may only be bound once")]
    DuplicateSecretTarget,
    #[error("environment variable name is invalid")]
    InvalidEnvironmentVariableName,
    #[error("HTTP header name is invalid")]
    InvalidHttpHeaderName,
    #[error("secret target does not match the selected transport")]
    SecretTargetTransportMismatch,
    #[error("a provider may only be excluded once")]
    DuplicateProviderExclusion,
    #[error("an MCP tool may only appear once")]
    DuplicateToolName,
    #[error("MCP tool count exceeds the domain size limit")]
    TooManyTools,
    #[error("MCP tool name is invalid")]
    InvalidToolName,
    #[error("failed runtime status must include a bounded error")]
    MissingRuntimeError,
    #[error("runtime error is only valid for failed or unsupported status")]
    UnexpectedRuntimeError,
    #[error("runtime error exceeds the domain size limit")]
    RuntimeErrorTooLong,
}

struct TrustFingerprintEncoder(Sha256);

impl TrustFingerprintEncoder {
    fn new() -> Self {
        let mut encoder = Self(Sha256::new());
        encoder.bytes(b"dcc-mcp-trust-fingerprint-v1");
        encoder
    }

    fn tag(&mut self, label: &str, value: &str) {
        self.string(label, value);
    }

    fn string(&mut self, label: &str, value: &str) {
        self.bytes(label.as_bytes());
        self.bytes(value.as_bytes());
    }

    fn optional_string(&mut self, label: &str, value: Option<&str>) {
        self.bytes(label.as_bytes());
        match value {
            Some(value) => {
                self.bytes(&[1]);
                self.bytes(value.as_bytes());
            }
            None => self.bytes(&[0]),
        }
    }

    fn strings<'a>(&mut self, label: &str, values: impl Iterator<Item = &'a str>) {
        let values = values.collect::<Vec<_>>();
        self.count(label, values.len());
        for value in values {
            self.bytes(value.as_bytes());
        }
    }

    fn count(&mut self, label: &str, count: usize) {
        self.bytes(label.as_bytes());
        self.0.update((count as u64).to_be_bytes());
    }

    fn bytes(&mut self, value: &[u8]) {
        self.0.update((value.len() as u64).to_be_bytes());
        self.0.update(value);
    }

    fn finish(self) -> McpTrustFingerprint {
        McpTrustFingerprint(format!("{:x}", self.0.finalize()))
    }
}

fn validate_non_empty(field: &'static str, value: &str) -> Result<(), McpValidationError> {
    if value.trim().is_empty() {
        return Err(McpValidationError::EmptyField { field });
    }
    Ok(())
}

fn validate_no_nul(field: &'static str, value: &str) -> Result<(), McpValidationError> {
    if value.contains('\0') {
        return Err(McpValidationError::NulByte { field });
    }
    Ok(())
}

fn validate_http_url(value: &str) -> Result<(), McpValidationError> {
    let url = Url::parse(value).map_err(|_| McpValidationError::InvalidUrl)?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(McpValidationError::UnsupportedUrlScheme);
    }
    if url.host_str().is_none() {
        return Err(McpValidationError::MissingUrlHost);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(McpValidationError::EmbeddedUrlCredentials);
    }
    Ok(())
}

fn is_valid_environment_name(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('_' | 'A'..='Z' | 'a'..='z'))
        && chars.all(|character| matches!(character, '_' | 'A'..='Z' | 'a'..='z' | '0'..='9'))
}

fn is_valid_http_header_name(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

fn is_valid_tool_name(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= MAX_MCP_TOOL_NAME_CHARS
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint(character: char) -> McpTrustFingerprint {
        McpTrustFingerprint(character.to_string().repeat(64))
    }

    fn trust() -> McpTrust {
        McpTrust {
            current_fingerprint: fingerprint('a'),
            decision: McpTrustDecision::Untrusted,
        }
    }

    fn definition(transport: McpTransport) -> McpDefinition {
        let mut definition = McpDefinition {
            id: McpDefinitionId("figma".to_string()),
            display_name: "Figma".to_string(),
            transport,
            secret_refs: Vec::new(),
            enabled: true,
            ownership: McpDefinitionOwnership::DccManaged,
            trust: trust(),
            created_at: "2026-07-28T00:00:00Z".to_string(),
            updated_at: "2026-07-28T00:00:00Z".to_string(),
        };
        definition.synchronize_trust_fingerprint();
        definition
    }

    #[test]
    fn accepts_direct_stdio_transport_and_argument_array() {
        let definition = definition(McpTransport::Stdio {
            executable: "npx".to_string(),
            args: vec!["@example/mcp@1.2.3".to_string()],
            cwd: Some("/workspace".to_string()),
        });

        assert_eq!(definition.validate(), Ok(()));
        assert_eq!(definition.transport.kind(), McpTransportKind::Stdio);
    }

    #[test]
    fn accepts_http_transport_without_embedded_credentials() {
        let definition = definition(McpTransport::Http {
            url: "https://mcp.example.com/v1".to_string(),
        });

        assert_eq!(definition.validate(), Ok(()));
        assert_eq!(definition.transport.kind(), McpTransportKind::Http);
    }

    #[test]
    fn rejects_non_http_urls_and_embedded_credentials() {
        let file = definition(McpTransport::Http {
            url: "file:///tmp/server.sock".to_string(),
        });
        assert_eq!(
            file.validate(),
            Err(McpValidationError::UnsupportedUrlScheme)
        );

        let credential = definition(McpTransport::Http {
            url: "https://token@example.com/mcp".to_string(),
        });
        assert_eq!(
            credential.validate(),
            Err(McpValidationError::EmbeddedUrlCredentials)
        );
    }

    #[test]
    fn secret_bindings_are_opaque_and_transport_specific() {
        let mut definition = definition(McpTransport::Http {
            url: "https://mcp.example.com".to_string(),
        });
        definition.secret_refs.push(McpSecretBinding {
            target: McpSecretTarget::HttpHeader {
                name: "Authorization".to_string(),
            },
            secret_ref: McpSecretReferenceId("credential:figma".to_string()),
        });
        definition.synchronize_trust_fingerprint();

        assert_eq!(definition.validate(), Ok(()));
        let json = serde_json::to_string(&definition).expect("serialize definition");
        assert!(json.contains("credential:figma"));
        assert!(!json.contains("Bearer secret"));

        definition.secret_refs.push(McpSecretBinding {
            target: McpSecretTarget::EnvironmentVariable {
                name: "TOKEN".to_string(),
            },
            secret_ref: McpSecretReferenceId("credential:token".to_string()),
        });
        assert_eq!(
            definition.validate(),
            Err(McpValidationError::SecretTargetTransportMismatch)
        );
    }

    #[test]
    fn imported_definitions_are_explicitly_read_only() {
        let ownership = McpDefinitionOwnership::ImportedReadOnly {
            source: McpImportSource {
                kind: McpImportSourceKind::ProjectFile,
                locator: ".mcp.json".to_string(),
                definition_key: Some("payments".to_string()),
            },
        };

        assert!(ownership.is_read_only());
    }

    #[test]
    fn changed_fingerprint_invalidates_the_prior_trust_decision() {
        let trust = McpTrust {
            current_fingerprint: fingerprint('b'),
            decision: McpTrustDecision::Trusted {
                fingerprint: fingerprint('a'),
                trusted_at: "2026-07-28T00:00:00Z".to_string(),
            },
        };

        assert!(trust.requires_confirmation());
        assert_eq!(trust.validate(), Ok(()));
    }

    #[test]
    fn matching_fingerprint_preserves_trust() {
        let fingerprint = fingerprint('a');
        let trust = McpTrust {
            current_fingerprint: fingerprint.clone(),
            decision: McpTrustDecision::Trusted {
                fingerprint,
                trusted_at: "2026-07-28T00:00:00Z".to_string(),
            },
        };

        assert!(!trust.requires_confirmation());
    }

    #[test]
    fn provider_exclusions_are_an_explicit_advanced_control() {
        let binding = McpBinding {
            id: McpBindingId("binding-1".to_string()),
            definition_id: McpDefinitionId("figma".to_string()),
            scope: McpBindingScope::Project {
                project_id: ProjectId("project-1".to_string()),
            },
            enabled: true,
            provider_exclusions: vec![
                ProviderId("cursor".to_string()),
                ProviderId("cursor".to_string()),
            ],
            created_at: "2026-07-28T00:00:00Z".to_string(),
            updated_at: "2026-07-28T00:00:00Z".to_string(),
        };

        assert_eq!(
            binding.validate(),
            Err(McpValidationError::DuplicateProviderExclusion)
        );
    }

    #[test]
    fn tagged_enum_fields_follow_the_camel_case_contract() {
        let scope = McpBindingScope::Session {
            session_id: SessionId("session-1".to_string()),
        };
        let decision = McpTrustDecision::Trusted {
            fingerprint: fingerprint('a'),
            trusted_at: "2026-07-28T00:00:00Z".to_string(),
        };

        let scope_json = serde_json::to_string(&scope).expect("serialize scope");
        let decision_json = serde_json::to_string(&decision).expect("serialize decision");
        assert!(scope_json.contains("sessionId"));
        assert!(!scope_json.contains("session_id"));
        assert!(decision_json.contains("trustedAt"));
        assert!(!decision_json.contains("trusted_at"));
    }

    #[test]
    fn runtime_errors_are_unicode_safe_and_bounded() {
        let message = "🦀".repeat(MAX_RUNTIME_ERROR_CHARS + 1);
        let error = McpRuntimeError::bounded(McpErrorCategory::Protocol, message);

        assert_eq!(error.message.chars().count(), MAX_RUNTIME_ERROR_CHARS);
        assert!(error.truncated);
    }

    #[test]
    fn header_targets_are_case_insensitively_unique_and_reject_injection() {
        let mut definition = definition(McpTransport::Http {
            url: "https://mcp.example.com".to_string(),
        });
        definition.secret_refs = vec![
            McpSecretBinding {
                target: McpSecretTarget::HttpHeader {
                    name: "Authorization".to_string(),
                },
                secret_ref: McpSecretReferenceId("credential:one".to_string()),
            },
            McpSecretBinding {
                target: McpSecretTarget::HttpHeader {
                    name: "authorization".to_string(),
                },
                secret_ref: McpSecretReferenceId("credential:two".to_string()),
            },
        ];
        assert_eq!(
            definition.validate(),
            Err(McpValidationError::DuplicateSecretTarget)
        );

        definition.secret_refs.truncate(1);
        definition.secret_refs[0].target = McpSecretTarget::HttpHeader {
            name: "Authorization\r\nX-Evil".to_string(),
        };
        assert_eq!(
            definition.validate(),
            Err(McpValidationError::InvalidHttpHeaderName)
        );
    }

    #[test]
    fn runtime_status_keeps_connected_and_failed_truth_distinct() {
        let mut status = McpRuntimeStatus {
            definition_id: McpDefinitionId("figma".to_string()),
            provider_id: ProviderId("codex".to_string()),
            provider_version: "1.0.0".to_string(),
            session_id: SessionId("session-1".to_string()),
            state: McpRuntimeState::Connected,
            tools: vec![McpToolSummary {
                name: "get_design".to_string(),
            }],
            checked_at: "2026-07-28T00:00:00Z".to_string(),
            bounded_error: None,
        };
        assert_eq!(status.validate(), Ok(()));

        status.state = McpRuntimeState::Failed;
        assert_eq!(
            status.validate(),
            Err(McpValidationError::MissingRuntimeError)
        );

        status.bounded_error = Some(McpRuntimeError::bounded(
            McpErrorCategory::Provider,
            "provider rejected attachment",
        ));
        assert_eq!(status.validate(), Ok(()));
    }

    #[test]
    fn probe_reports_require_unique_tools() {
        let report = McpProbeReport {
            definition_id: McpDefinitionId("figma".to_string()),
            transport: McpTransportKind::Http,
            protocol_version: "2025-11-25".to_string(),
            tools: vec![
                McpToolSummary {
                    name: "inspect".to_string(),
                },
                McpToolSummary {
                    name: "inspect".to_string(),
                },
            ],
            checked_at: "2026-07-28T00:00:00Z".to_string(),
        };

        assert_eq!(
            report.validate(),
            Err(McpValidationError::DuplicateToolName)
        );
    }

    #[test]
    fn tool_policies_accept_only_bounded_protocol_tool_names() {
        let mut policy = McpToolPolicy {
            definition_id: McpDefinitionId("figma".to_string()),
            tool_name: "get_design".to_string(),
            decision: McpToolPolicyDecision::Deny,
            updated_at: "2026-07-28T00:00:00Z".to_string(),
        };
        assert_eq!(policy.validate(), Ok(()));

        policy.tool_name = "unsafe tool".to_string();
        assert_eq!(policy.validate(), Err(McpValidationError::InvalidToolName));
    }
}
