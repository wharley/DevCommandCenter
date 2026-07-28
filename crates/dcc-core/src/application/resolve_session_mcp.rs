use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
};

use sha2::{Digest, Sha256};

use crate::{
    domain::{
        mcp::{McpBindingScope, McpDefinition, McpSecretTarget, McpTransport},
        project::ProjectId,
        provider::ProviderId,
        session::SessionId,
    },
    ports::{
        CredentialStore, McpRepo, ProviderMcpSecret, ProviderMcpServerConfig, ProviderMcpTransport,
    },
    CoreError, Result,
};

/// Provider and scope identity used to resolve the DCC-owned MCP projection
/// for one provider session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolveSessionMcpInput {
    pub provider_id: ProviderId,
    pub project_id: ProjectId,
    pub session_id: SessionId,
}

/// Resolves enabled, trusted definitions selected by at least one applicable
/// binding, then replaces opaque credential references with backend-only
/// secret values.
///
/// Provider exclusions are binding-local: one excluded binding does not veto
/// a separate applicable binding for the same definition.
pub async fn resolve_session_mcp_servers(
    repo: &dyn McpRepo,
    credential_store: &dyn CredentialStore,
    input: &ResolveSessionMcpInput,
) -> Result<Vec<ProviderMcpServerConfig>> {
    let definitions = repo.list_mcp_definitions().await?;
    let bindings = repo.list_mcp_bindings(None).await?;

    for definition in &definitions {
        definition
            .validate()
            .map_err(|_| invalid_registry("definition"))?;
    }
    for binding in &bindings {
        binding
            .validate()
            .map_err(|_| invalid_registry("binding"))?;
    }

    let selected_ids = bindings
        .iter()
        .filter(|binding| {
            binding.enabled
                && binding_applies(&binding.scope, input)
                && !binding
                    .provider_exclusions
                    .iter()
                    .any(|provider_id| provider_id == &input.provider_id)
        })
        .map(|binding| binding.definition_id.0.clone())
        .collect::<HashSet<_>>();

    let mut definitions_by_id = HashMap::with_capacity(definitions.len());
    for definition in definitions {
        if definitions_by_id
            .insert(definition.id.0.clone(), definition)
            .is_some()
        {
            return Err(invalid_registry("definition"));
        }
    }
    if selected_ids
        .iter()
        .any(|definition_id| !definitions_by_id.contains_key(definition_id))
    {
        return Err(invalid_registry("binding"));
    }

    let mut selected = selected_ids
        .into_iter()
        .filter_map(|definition_id| definitions_by_id.remove(&definition_id))
        .filter(|definition| definition.enabled && !definition.trust.requires_confirmation())
        .collect::<Vec<_>>();
    selected.sort_unstable_by(|left, right| left.id.0.cmp(&right.id.0));

    let mut server_names = HashSet::with_capacity(selected.len());
    let mut servers = Vec::with_capacity(selected.len());
    for definition in selected {
        let server_name = provider_server_name(&definition);
        if !server_names.insert(server_name.clone()) {
            return Err(invalid_registry("definition"));
        }
        servers.push(project_definition(definition, server_name, credential_store).await?);
    }
    Ok(servers)
}

fn binding_applies(scope: &McpBindingScope, input: &ResolveSessionMcpInput) -> bool {
    match scope {
        McpBindingScope::Session { session_id } => session_id == &input.session_id,
        McpBindingScope::Project { project_id } => project_id == &input.project_id,
        McpBindingScope::Global => true,
    }
}

fn provider_server_name(definition: &McpDefinition) -> String {
    let digest = Sha256::digest(definition.id.0.as_bytes());
    let mut name = String::with_capacity(36);
    name.push_str("dcc-");
    for byte in &digest[..16] {
        write!(&mut name, "{byte:02x}").expect("writing to a String cannot fail");
    }
    name
}

async fn project_definition(
    definition: McpDefinition,
    server_name: String,
    credential_store: &dyn CredentialStore,
) -> Result<ProviderMcpServerConfig> {
    let mut secrets = Vec::with_capacity(definition.secret_refs.len());
    for binding in &definition.secret_refs {
        let value = credential_store
            .resolve_secret(&binding.secret_ref)
            .await
            .map_err(|_| credential_resolution_failed())?
            .ok_or_else(credential_resolution_failed)?;
        let name = match &binding.target {
            McpSecretTarget::EnvironmentVariable { name }
            | McpSecretTarget::HttpHeader { name } => name.clone(),
        };
        secrets.push(ProviderMcpSecret::new(name, value));
    }

    let transport = match definition.transport {
        McpTransport::Stdio {
            executable,
            args,
            cwd,
        } => ProviderMcpTransport::Stdio {
            executable,
            args,
            cwd,
            environment: secrets,
        },
        McpTransport::Http { url } => ProviderMcpTransport::Http {
            url,
            headers: secrets,
        },
    };

    Ok(ProviderMcpServerConfig {
        definition_id: definition.id,
        server_name,
        transport,
    })
}

fn invalid_registry(entity: &str) -> CoreError {
    CoreError::Repository(format!("MCP registry contains an invalid {entity}"))
}

fn credential_resolution_failed() -> CoreError {
    CoreError::Provider("MCP credential resolution failed".to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;

    use crate::{
        domain::mcp::{
            McpBinding, McpBindingId, McpDefinitionId, McpDefinitionOwnership, McpSecretBinding,
            McpSecretReferenceId, McpTrust, McpTrustDecision, McpTrustFingerprint,
        },
        ports::{CredentialStoreError, CredentialStoreResult, SecretValue},
    };

    use super::*;

    #[derive(Clone, Default)]
    struct FakeMcpRepo {
        definitions: Arc<Mutex<Vec<McpDefinition>>>,
        bindings: Arc<Mutex<Vec<McpBinding>>>,
    }

    #[async_trait]
    impl McpRepo for FakeMcpRepo {
        async fn save_mcp_definition(&self, definition: &McpDefinition) -> Result<()> {
            self.definitions.lock().unwrap().push(definition.clone());
            Ok(())
        }

        async fn get_mcp_definition(&self, id: &McpDefinitionId) -> Result<Option<McpDefinition>> {
            Ok(self
                .definitions
                .lock()
                .unwrap()
                .iter()
                .find(|definition| &definition.id == id)
                .cloned())
        }

        async fn list_mcp_definitions(&self) -> Result<Vec<McpDefinition>> {
            Ok(self.definitions.lock().unwrap().clone())
        }

        async fn delete_mcp_definition(&self, id: &McpDefinitionId) -> Result<()> {
            self.definitions
                .lock()
                .unwrap()
                .retain(|definition| &definition.id != id);
            Ok(())
        }

        async fn save_mcp_binding(&self, binding: &McpBinding) -> Result<()> {
            self.bindings.lock().unwrap().push(binding.clone());
            Ok(())
        }

        async fn get_mcp_binding(&self, id: &McpBindingId) -> Result<Option<McpBinding>> {
            Ok(self
                .bindings
                .lock()
                .unwrap()
                .iter()
                .find(|binding| &binding.id == id)
                .cloned())
        }

        async fn list_mcp_bindings(
            &self,
            definition_id: Option<&McpDefinitionId>,
        ) -> Result<Vec<McpBinding>> {
            Ok(self
                .bindings
                .lock()
                .unwrap()
                .iter()
                .filter(|binding| {
                    definition_id
                        .map(|definition_id| &binding.definition_id == definition_id)
                        .unwrap_or(true)
                })
                .cloned()
                .collect())
        }

        async fn delete_mcp_binding(&self, id: &McpBindingId) -> Result<()> {
            self.bindings
                .lock()
                .unwrap()
                .retain(|binding| &binding.id != id);
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct FakeCredentialStore {
        secrets: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    }

    impl FakeCredentialStore {
        fn insert(&self, reference: &str, value: &str) {
            self.secrets
                .lock()
                .unwrap()
                .insert(reference.to_string(), value.as_bytes().to_vec());
        }
    }

    #[async_trait]
    impl CredentialStore for FakeCredentialStore {
        async fn store_secret(
            &self,
            reference: &McpSecretReferenceId,
            secret: SecretValue,
        ) -> CredentialStoreResult<()> {
            self.secrets
                .lock()
                .map_err(|_| CredentialStoreError::OperationFailed)?
                .insert(reference.0.clone(), secret.expose_secret().to_vec());
            Ok(())
        }

        async fn resolve_secret(
            &self,
            reference: &McpSecretReferenceId,
        ) -> CredentialStoreResult<Option<SecretValue>> {
            self.secrets
                .lock()
                .map_err(|_| CredentialStoreError::OperationFailed)?
                .get(&reference.0)
                .cloned()
                .map(SecretValue::new)
                .transpose()
        }

        async fn delete_secret(
            &self,
            reference: &McpSecretReferenceId,
        ) -> CredentialStoreResult<bool> {
            Ok(self
                .secrets
                .lock()
                .map_err(|_| CredentialStoreError::OperationFailed)?
                .remove(&reference.0)
                .is_some())
        }
    }

    fn trusted_definition(
        id: &str,
        transport: McpTransport,
        secret_refs: Vec<McpSecretBinding>,
    ) -> McpDefinition {
        let placeholder = McpTrustFingerprint("0".repeat(64));
        let mut definition = McpDefinition {
            id: McpDefinitionId(id.to_string()),
            display_name: id.to_string(),
            transport,
            secret_refs,
            enabled: true,
            ownership: McpDefinitionOwnership::DccManaged,
            trust: McpTrust {
                current_fingerprint: placeholder,
                decision: McpTrustDecision::Untrusted,
            },
            created_at: "2026-07-28T00:00:00Z".to_string(),
            updated_at: "2026-07-28T00:00:00Z".to_string(),
        };
        definition.synchronize_trust_fingerprint();
        definition.trust.decision = McpTrustDecision::Trusted {
            fingerprint: definition.trust.current_fingerprint.clone(),
            trusted_at: "2026-07-28T00:00:00Z".to_string(),
        };
        definition
    }

    fn binding(
        id: &str,
        definition_id: &str,
        scope: McpBindingScope,
        exclusions: Vec<&str>,
    ) -> McpBinding {
        McpBinding {
            id: McpBindingId(id.to_string()),
            definition_id: McpDefinitionId(definition_id.to_string()),
            scope,
            enabled: true,
            provider_exclusions: exclusions
                .into_iter()
                .map(|id| ProviderId(id.to_string()))
                .collect(),
            created_at: "2026-07-28T00:00:00Z".to_string(),
            updated_at: "2026-07-28T00:00:00Z".to_string(),
        }
    }

    fn input() -> ResolveSessionMcpInput {
        ResolveSessionMcpInput {
            provider_id: ProviderId("claude_code".to_string()),
            project_id: ProjectId("project-a".to_string()),
            session_id: SessionId("session-a".to_string()),
        }
    }

    #[test]
    fn resolves_matching_scopes_once_and_honors_binding_local_exclusions() {
        let repo = FakeMcpRepo::default();
        let credentials = FakeCredentialStore::default();
        credentials.insert("credential:http", "Bearer secret-canary");

        let stdio = trusted_definition(
            "stdio",
            McpTransport::Stdio {
                executable: "fixture".to_string(),
                args: vec!["stdio".to_string()],
                cwd: Some("/workspace".to_string()),
            },
            Vec::new(),
        );
        let http = trusted_definition(
            "http",
            McpTransport::Http {
                url: "https://example.com/mcp".to_string(),
            },
            vec![McpSecretBinding {
                target: McpSecretTarget::HttpHeader {
                    name: "Authorization".to_string(),
                },
                secret_ref: McpSecretReferenceId("credential:http".to_string()),
            }],
        );
        let session_only = trusted_definition(
            "session-only",
            McpTransport::Http {
                url: "https://session.example/mcp".to_string(),
            },
            Vec::new(),
        );
        let disabled_binding = trusted_definition(
            "disabled-binding",
            McpTransport::Http {
                url: "https://disabled-binding.example/mcp".to_string(),
            },
            Vec::new(),
        );
        let mut disabled = trusted_definition(
            "disabled",
            McpTransport::Http {
                url: "https://disabled.example/mcp".to_string(),
            },
            Vec::new(),
        );
        disabled.enabled = false;
        let mut untrusted = trusted_definition(
            "untrusted",
            McpTransport::Http {
                url: "https://untrusted.example/mcp".to_string(),
            },
            Vec::new(),
        );
        untrusted.trust.decision = McpTrustDecision::Untrusted;

        repo.definitions.lock().unwrap().extend([
            stdio,
            http,
            session_only,
            disabled_binding,
            disabled,
            untrusted,
        ]);
        let mut inactive_binding = binding(
            "inactive-global",
            "disabled-binding",
            McpBindingScope::Global,
            Vec::new(),
        );
        inactive_binding.enabled = false;
        repo.bindings.lock().unwrap().extend([
            binding("stdio-global", "stdio", McpBindingScope::Global, Vec::new()),
            binding(
                "stdio-project",
                "stdio",
                McpBindingScope::Project {
                    project_id: ProjectId("project-a".to_string()),
                },
                Vec::new(),
            ),
            binding(
                "session-only",
                "session-only",
                McpBindingScope::Session {
                    session_id: SessionId("session-a".to_string()),
                },
                Vec::new(),
            ),
            binding(
                "wrong-session",
                "session-only",
                McpBindingScope::Session {
                    session_id: SessionId("session-b".to_string()),
                },
                Vec::new(),
            ),
            inactive_binding,
            binding(
                "http-excluded-session",
                "http",
                McpBindingScope::Session {
                    session_id: SessionId("session-a".to_string()),
                },
                vec!["claude_code"],
            ),
            binding(
                "http-project",
                "http",
                McpBindingScope::Project {
                    project_id: ProjectId("project-a".to_string()),
                },
                Vec::new(),
            ),
            binding(
                "disabled-global",
                "disabled",
                McpBindingScope::Global,
                Vec::new(),
            ),
            binding(
                "untrusted-global",
                "untrusted",
                McpBindingScope::Global,
                Vec::new(),
            ),
            binding(
                "wrong-project",
                "http",
                McpBindingScope::Project {
                    project_id: ProjectId("project-b".to_string()),
                },
                Vec::new(),
            ),
        ]);

        let servers =
            futures::executor::block_on(resolve_session_mcp_servers(&repo, &credentials, &input()))
                .expect("resolve MCP servers");

        assert_eq!(
            servers
                .iter()
                .map(|server| server.definition_id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["http", "session-only", "stdio"]
        );
        assert!(servers
            .iter()
            .all(|server| server.server_name.starts_with("dcc-")));
        let http = &servers[0];
        let ProviderMcpTransport::Http { headers, .. } = &http.transport else {
            panic!("expected HTTP transport");
        };
        assert_eq!(headers[0].name, "Authorization");
        assert_eq!(headers[0].expose_secret(), b"Bearer secret-canary");
        let ProviderMcpTransport::Stdio { cwd, .. } = &servers[2].transport else {
            panic!("expected stdio transport");
        };
        assert_eq!(cwd.as_deref(), Some("/workspace"));
        assert!(!format!("{servers:?}").contains("secret-canary"));
    }

    #[test]
    fn missing_credentials_fail_closed_without_leaking_registry_identity() {
        let repo = FakeMcpRepo::default();
        let definition = trusted_definition(
            "private-payment-gateway",
            McpTransport::Http {
                url: "https://payments.example/mcp".to_string(),
            },
            vec![McpSecretBinding {
                target: McpSecretTarget::HttpHeader {
                    name: "Authorization".to_string(),
                },
                secret_ref: McpSecretReferenceId("credential:super-secret-name".to_string()),
            }],
        );
        repo.definitions.lock().unwrap().push(definition);
        repo.bindings.lock().unwrap().push(binding(
            "payment-global",
            "private-payment-gateway",
            McpBindingScope::Global,
            Vec::new(),
        ));

        let error = futures::executor::block_on(resolve_session_mcp_servers(
            &repo,
            &FakeCredentialStore::default(),
            &input(),
        ))
        .expect_err("missing credential must fail");
        let message = error.to_string();

        assert_eq!(message, "provider error: MCP credential resolution failed");
        assert!(!message.contains("private-payment-gateway"));
        assert!(!message.contains("super-secret-name"));
    }
}
