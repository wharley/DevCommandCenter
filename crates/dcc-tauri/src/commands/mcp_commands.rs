use std::collections::{HashMap, HashSet};

use chrono::Utc;
use dcc_core::{
    application::{activate_mcp_definition, ActivateMcpDefinitionInput},
    domain::mcp::{
        McpBinding, McpBindingId, McpBindingScope, McpDefinition, McpDefinitionId,
        McpDefinitionOwnership, McpSecretBinding, McpSecretReferenceId, McpSecretTarget,
        McpTransport, McpTrust, McpTrustDecision, McpTrustFingerprint,
    },
    ports::{CredentialStore, McpRepo, SecretValue},
    CoreError,
};
use dcc_infra::{credential_store::SystemCredentialStore, mcp_db::SqliteMcpRepo};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;
use thiserror::Error;
use uuid::Uuid;

use crate::state::WorkspaceCommandState;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpIntegrationRecord {
    pub definition: McpDefinition,
    pub bindings: Vec<McpBinding>,
    pub credential_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListMcpIntegrationsOutput {
    pub integrations: Vec<McpIntegrationRecord>,
}

#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpCredentialInput {
    pub target: McpSecretTarget,
    /// Write-only renderer input. It is moved into `SecretValue`, zeroized on
    /// drop, and never included in a command output.
    pub secret: String,
}

#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateMcpIntegrationInput {
    pub display_name: String,
    pub transport: McpTransport,
    pub scope: McpBindingScope,
    #[serde(default)]
    pub credentials: Vec<McpCredentialInput>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateMcpIntegrationOutput {
    pub integration: McpIntegrationRecord,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ActivateMcpIntegrationOutput {
    pub integration: McpIntegrationRecord,
    pub changed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DisableMcpIntegrationInput {
    pub definition_id: McpDefinitionId,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DisableMcpIntegrationOutput {
    pub integration: McpIntegrationRecord,
    pub changed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RemoveMcpIntegrationInput {
    pub definition_id: McpDefinitionId,
    #[serde(default)]
    pub delete_credentials: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RemoveMcpIntegrationOutput {
    pub removed: bool,
    pub deleted_credentials: usize,
}

#[derive(Debug, Error)]
enum McpCommandError {
    #[error("{0}")]
    Core(#[from] CoreError),
    #[error("credential operation failed: {0}")]
    Credential(#[from] dcc_core::ports::CredentialStoreError),
}

type McpCommandResult<T> = Result<T, McpCommandError>;

#[tauri::command]
pub async fn list_mcp_integrations(
    state: State<'_, WorkspaceCommandState>,
) -> Result<ListMcpIntegrationsOutput, String> {
    let repo = SqliteMcpRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    list_integrations(&repo)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn create_mcp_integration(
    state: State<'_, WorkspaceCommandState>,
    input: CreateMcpIntegrationInput,
) -> Result<CreateMcpIntegrationOutput, String> {
    let repo = SqliteMcpRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    create_integration(&repo, &SystemCredentialStore::default(), input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn activate_mcp_integration(
    state: State<'_, WorkspaceCommandState>,
    input: ActivateMcpDefinitionInput,
) -> Result<ActivateMcpIntegrationOutput, String> {
    let repo = SqliteMcpRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    activate_integration(&repo, input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn disable_mcp_integration(
    state: State<'_, WorkspaceCommandState>,
    input: DisableMcpIntegrationInput,
) -> Result<DisableMcpIntegrationOutput, String> {
    let repo = SqliteMcpRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    disable_integration(&repo, input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn remove_mcp_integration(
    state: State<'_, WorkspaceCommandState>,
    input: RemoveMcpIntegrationInput,
) -> Result<RemoveMcpIntegrationOutput, String> {
    let repo = SqliteMcpRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    remove_integration(&repo, &SystemCredentialStore::default(), input)
        .await
        .map_err(|error| error.to_string())
}

async fn list_integrations<R>(repo: &R) -> McpCommandResult<ListMcpIntegrationsOutput>
where
    R: McpRepo + Sync + ?Sized,
{
    let definitions = repo.list_mcp_definitions().await?;
    let mut bindings_by_definition = HashMap::<McpDefinitionId, Vec<McpBinding>>::new();
    for binding in repo.list_mcp_bindings(None).await? {
        bindings_by_definition
            .entry(binding.definition_id.clone())
            .or_default()
            .push(binding);
    }

    let integrations = definitions
        .into_iter()
        .map(|definition| {
            let bindings = bindings_by_definition
                .remove(&definition.id)
                .unwrap_or_default();
            integration_record(definition, bindings)
        })
        .collect();
    Ok(ListMcpIntegrationsOutput { integrations })
}

async fn create_integration<R, C>(
    repo: &R,
    credential_store: &C,
    input: CreateMcpIntegrationInput,
) -> McpCommandResult<CreateMcpIntegrationOutput>
where
    R: McpRepo + Sync + ?Sized,
    C: CredentialStore + Sync + ?Sized,
{
    let definition_id = McpDefinitionId(format!("mcp-{}", Uuid::new_v4()));
    let mut credential_values = Vec::with_capacity(input.credentials.len());
    let mut secret_refs = Vec::with_capacity(input.credentials.len());
    for credential in input.credentials {
        let reference =
            McpSecretReferenceId(format!("credential:{}:{}", definition_id.0, Uuid::new_v4()));
        let secret = SecretValue::new(credential.secret.into_bytes())?;
        secret_refs.push(McpSecretBinding {
            target: credential.target,
            secret_ref: reference.clone(),
        });
        credential_values.push((reference, secret));
    }

    let now = Utc::now().to_rfc3339();
    let mut definition = McpDefinition {
        id: definition_id.clone(),
        display_name: input.display_name.trim().to_string(),
        transport: input.transport,
        secret_refs,
        enabled: false,
        ownership: McpDefinitionOwnership::DccManaged,
        trust: McpTrust {
            current_fingerprint: McpTrustFingerprint("0".repeat(64)),
            decision: McpTrustDecision::Untrusted,
        },
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    definition.synchronize_trust_fingerprint();
    definition
        .validate()
        .map_err(|error| CoreError::InvalidInput(error.to_string()))?;

    let binding = McpBinding {
        id: McpBindingId(format!("binding-{}", Uuid::new_v4())),
        definition_id,
        scope: input.scope,
        enabled: true,
        provider_exclusions: Vec::new(),
        created_at: now.clone(),
        updated_at: now,
    };
    binding
        .validate()
        .map_err(|error| CoreError::InvalidInput(error.to_string()))?;

    let mut stored_references = Vec::with_capacity(credential_values.len());
    for (reference, secret) in credential_values {
        if let Err(error) = credential_store.store_secret(&reference, secret).await {
            delete_credentials_best_effort(credential_store, &stored_references).await;
            return Err(error.into());
        }
        stored_references.push(reference);
    }

    if let Err(error) = repo.save_mcp_definition(&definition).await {
        delete_credentials_best_effort(credential_store, &stored_references).await;
        return Err(error.into());
    }
    if let Err(error) = repo.save_mcp_binding(&binding).await {
        let _ = repo.delete_mcp_definition(&definition.id).await;
        delete_credentials_best_effort(credential_store, &stored_references).await;
        return Err(error.into());
    }

    Ok(CreateMcpIntegrationOutput {
        integration: integration_record(definition, vec![binding]),
    })
}

async fn activate_integration<R>(
    repo: &R,
    input: ActivateMcpDefinitionInput,
) -> McpCommandResult<ActivateMcpIntegrationOutput>
where
    R: McpRepo + Sync + ?Sized,
{
    let output = activate_mcp_definition(repo, input).await?;
    let bindings = repo.list_mcp_bindings(Some(&output.definition.id)).await?;
    Ok(ActivateMcpIntegrationOutput {
        integration: integration_record(output.definition, bindings),
        changed: output.changed,
    })
}

async fn disable_integration<R>(
    repo: &R,
    input: DisableMcpIntegrationInput,
) -> McpCommandResult<DisableMcpIntegrationOutput>
where
    R: McpRepo + Sync + ?Sized,
{
    let mut definition = repo
        .get_mcp_definition(&input.definition_id)
        .await?
        .ok_or_else(|| CoreError::InvalidInput("MCP definition was not found".to_string()))?;
    let changed = definition.enabled;
    if changed {
        definition.enabled = false;
        definition.updated_at = Utc::now().to_rfc3339();
        repo.save_mcp_definition(&definition).await?;
    }
    let bindings = repo.list_mcp_bindings(Some(&definition.id)).await?;
    Ok(DisableMcpIntegrationOutput {
        integration: integration_record(definition, bindings),
        changed,
    })
}

async fn remove_integration<R, C>(
    repo: &R,
    credential_store: &C,
    input: RemoveMcpIntegrationInput,
) -> McpCommandResult<RemoveMcpIntegrationOutput>
where
    R: McpRepo + Sync + ?Sized,
    C: CredentialStore + Sync + ?Sized,
{
    let Some(mut definition) = repo.get_mcp_definition(&input.definition_id).await? else {
        return Ok(RemoveMcpIntegrationOutput {
            removed: false,
            deleted_credentials: 0,
        });
    };

    let mut deleted_credentials = 0;
    if input.delete_credentials {
        if definition.enabled {
            definition.enabled = false;
            definition.updated_at = Utc::now().to_rfc3339();
            repo.save_mcp_definition(&definition).await?;
        }

        let mut unique_references = HashSet::new();
        for binding in &definition.secret_refs {
            if unique_references.insert(binding.secret_ref.clone())
                && credential_store.delete_secret(&binding.secret_ref).await?
            {
                deleted_credentials += 1;
            }
        }
    }

    repo.delete_mcp_definition(&definition.id).await?;
    Ok(RemoveMcpIntegrationOutput {
        removed: true,
        deleted_credentials,
    })
}

fn integration_record(
    definition: McpDefinition,
    bindings: Vec<McpBinding>,
) -> McpIntegrationRecord {
    let credential_count = definition.secret_refs.len();
    McpIntegrationRecord {
        definition,
        bindings,
        credential_count,
    }
}

async fn delete_credentials_best_effort<C>(
    credential_store: &C,
    references: &[McpSecretReferenceId],
) where
    C: CredentialStore + Sync + ?Sized,
{
    for reference in references {
        let _ = credential_store.delete_secret(reference).await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    };

    use async_trait::async_trait;
    use dcc_core::{
        domain::mcp::{McpSecretTarget, McpTransport},
        ports::{CredentialStoreError, CredentialStoreResult},
    };
    use dcc_infra::mcp_db::SqliteMcpRepo;
    use rusqlite::Connection;

    use super::*;

    #[derive(Clone, Default)]
    struct TestCredentialStore {
        values: Arc<Mutex<HashMap<String, Vec<u8>>>>,
        fail_deletes: Arc<AtomicBool>,
    }

    #[async_trait]
    impl CredentialStore for TestCredentialStore {
        async fn store_secret(
            &self,
            reference: &McpSecretReferenceId,
            secret: SecretValue,
        ) -> CredentialStoreResult<()> {
            self.values
                .lock()
                .map_err(|_| CredentialStoreError::OperationFailed)?
                .insert(reference.0.clone(), secret.expose_secret().to_vec());
            Ok(())
        }

        async fn resolve_secret(
            &self,
            reference: &McpSecretReferenceId,
        ) -> CredentialStoreResult<Option<SecretValue>> {
            self.values
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
            if self.fail_deletes.load(Ordering::Relaxed) {
                return Err(CredentialStoreError::AccessDenied);
            }
            Ok(self
                .values
                .lock()
                .map_err(|_| CredentialStoreError::OperationFailed)?
                .remove(&reference.0)
                .is_some())
        }
    }

    fn repo() -> SqliteMcpRepo {
        SqliteMcpRepo::from_connection(Arc::new(Mutex::new(
            Connection::open_in_memory().expect("open sqlite"),
        )))
        .expect("create MCP repo")
    }

    fn create_input(secret: &str) -> CreateMcpIntegrationInput {
        CreateMcpIntegrationInput {
            display_name: "Payments".to_string(),
            transport: McpTransport::Http {
                url: "https://mcp.example.test".to_string(),
            },
            scope: McpBindingScope::Global,
            credentials: vec![McpCredentialInput {
                target: McpSecretTarget::HttpHeader {
                    name: "Authorization".to_string(),
                },
                secret: secret.to_string(),
            }],
        }
    }

    #[tokio::test]
    async fn create_and_list_return_only_opaque_credential_metadata() {
        let repo = repo();
        let credentials = TestCredentialStore::default();
        let secret = "renderer-write-only-token";
        let created = create_integration(&repo, &credentials, create_input(secret))
            .await
            .expect("create integration");

        assert!(!created.integration.definition.enabled);
        assert!(created.integration.definition.trust.requires_confirmation());
        assert_eq!(created.integration.credential_count, 1);
        let serialized = serde_json::to_string(&created).expect("serialize output");
        assert!(!serialized.contains(secret));

        let reference = &created.integration.definition.secret_refs[0].secret_ref;
        assert_eq!(
            credentials
                .resolve_secret(reference)
                .await
                .expect("resolve credential")
                .expect("stored credential")
                .expose_secret(),
            secret.as_bytes()
        );
        let listed = list_integrations(&repo).await.expect("list integrations");
        assert_eq!(listed.integrations, vec![created.integration]);
    }

    #[tokio::test]
    async fn activation_is_fingerprint_bound_and_disable_is_idempotent() {
        let repo = repo();
        let credentials = TestCredentialStore::default();
        let created = create_integration(&repo, &credentials, create_input("token"))
            .await
            .expect("create integration");
        let definition_id = created.integration.definition.id.clone();

        let stale = activate_integration(
            &repo,
            ActivateMcpDefinitionInput {
                definition_id: definition_id.clone(),
                expected_fingerprint: McpTrustFingerprint("f".repeat(64)),
            },
        )
        .await;
        assert!(matches!(
            stale,
            Err(McpCommandError::Core(CoreError::InvalidInput(_)))
        ));

        let activated = activate_integration(
            &repo,
            ActivateMcpDefinitionInput {
                definition_id: definition_id.clone(),
                expected_fingerprint: created
                    .integration
                    .definition
                    .trust
                    .current_fingerprint
                    .clone(),
            },
        )
        .await
        .expect("activate integration");
        assert!(activated.changed);
        assert!(activated.integration.definition.enabled);

        let disabled = disable_integration(
            &repo,
            DisableMcpIntegrationInput {
                definition_id: definition_id.clone(),
            },
        )
        .await
        .expect("disable integration");
        assert!(disabled.changed);
        assert!(!disabled.integration.definition.enabled);
        assert!(
            !disable_integration(&repo, DisableMcpIntegrationInput { definition_id })
                .await
                .expect("disable again")
                .changed
        );
    }

    #[tokio::test]
    async fn removal_deletes_credentials_only_when_explicitly_requested() {
        let repo = repo();
        let credentials = TestCredentialStore::default();
        let kept = create_integration(&repo, &credentials, create_input("kept"))
            .await
            .expect("create kept integration");
        let kept_reference = kept.integration.definition.secret_refs[0]
            .secret_ref
            .clone();
        let removed = remove_integration(
            &repo,
            &credentials,
            RemoveMcpIntegrationInput {
                definition_id: kept.integration.definition.id,
                delete_credentials: false,
            },
        )
        .await
        .expect("remove and keep credential");
        assert!(removed.removed);
        assert_eq!(removed.deleted_credentials, 0);
        assert!(credentials
            .resolve_secret(&kept_reference)
            .await
            .expect("resolve kept credential")
            .is_some());

        let deleted = create_integration(&repo, &credentials, create_input("deleted"))
            .await
            .expect("create deleted integration");
        let deleted_reference = deleted.integration.definition.secret_refs[0]
            .secret_ref
            .clone();
        let removed = remove_integration(
            &repo,
            &credentials,
            RemoveMcpIntegrationInput {
                definition_id: deleted.integration.definition.id,
                delete_credentials: true,
            },
        )
        .await
        .expect("remove and delete credential");
        assert!(removed.removed);
        assert_eq!(removed.deleted_credentials, 1);
        assert!(credentials
            .resolve_secret(&deleted_reference)
            .await
            .expect("resolve deleted credential")
            .is_none());
    }

    #[tokio::test]
    async fn credential_deletion_failure_keeps_the_definition_disabled_for_retry() {
        let repo = repo();
        let credentials = TestCredentialStore::default();
        let created = create_integration(&repo, &credentials, create_input("protected"))
            .await
            .expect("create integration");
        let definition_id = created.integration.definition.id.clone();
        activate_integration(
            &repo,
            ActivateMcpDefinitionInput {
                definition_id: definition_id.clone(),
                expected_fingerprint: created.integration.definition.trust.current_fingerprint,
            },
        )
        .await
        .expect("activate integration");

        credentials.fail_deletes.store(true, Ordering::Relaxed);
        let removal = remove_integration(
            &repo,
            &credentials,
            RemoveMcpIntegrationInput {
                definition_id: definition_id.clone(),
                delete_credentials: true,
            },
        )
        .await;
        assert!(matches!(
            removal,
            Err(McpCommandError::Credential(
                CredentialStoreError::AccessDenied
            ))
        ));

        let retained = repo
            .get_mcp_definition(&definition_id)
            .await
            .expect("load retained definition")
            .expect("definition retained");
        assert!(!retained.enabled);
    }
}
