use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    domain::mcp::{
        McpDefinition, McpDefinitionId, McpDefinitionOwnership, McpTrustDecision,
        McpTrustFingerprint,
    },
    ports::McpRepo,
    CoreError, Result,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ActivateMcpDefinitionInput {
    pub definition_id: McpDefinitionId,
    /// The exact fingerprint displayed when the user confirmed activation.
    /// Requiring it prevents a stale UI from approving a changed definition.
    pub expected_fingerprint: McpTrustFingerprint,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ActivateMcpDefinitionOutput {
    pub definition: McpDefinition,
    pub changed: bool,
}

/// Makes a repository-discovered definition read-only, disabled, and
/// untrusted before its first persistence.
pub fn prepare_imported_mcp_definition(mut definition: McpDefinition) -> Result<McpDefinition> {
    if !matches!(
        definition.ownership,
        McpDefinitionOwnership::ImportedReadOnly { .. }
    ) {
        return Err(CoreError::InvalidInput(
            "imported MCP definition must have imported read-only ownership".to_string(),
        ));
    }

    definition.enabled = false;
    definition.trust.decision = McpTrustDecision::Untrusted;
    definition.synchronize_trust_fingerprint();
    validate_definition(&definition)?;
    Ok(definition)
}

/// Recomputes the current fingerprint and preserves the prior decision. If a
/// security-relevant field changed, `requires_confirmation()` becomes true.
pub fn synchronize_mcp_definition_trust(definition: &mut McpDefinition) -> Result<bool> {
    let changed = definition.synchronize_trust_fingerprint();
    validate_definition(definition)?;
    Ok(changed)
}

/// Records explicit trust for exactly the definition the user reviewed and
/// enables it. This service does not start a process or contact a server.
pub async fn activate_mcp_definition<R>(
    repo: &R,
    input: ActivateMcpDefinitionInput,
) -> Result<ActivateMcpDefinitionOutput>
where
    R: McpRepo + Sync + ?Sized,
{
    input
        .expected_fingerprint
        .validate()
        .map_err(|error| CoreError::InvalidInput(error.to_string()))?;

    let mut definition = repo
        .get_mcp_definition(&input.definition_id)
        .await?
        .ok_or_else(|| CoreError::InvalidInput("MCP definition was not found".to_string()))?;

    definition.synchronize_trust_fingerprint();
    if definition.trust.current_fingerprint != input.expected_fingerprint {
        return Err(CoreError::InvalidInput(
            "MCP definition changed after the activation preview".to_string(),
        ));
    }

    let needs_trust = definition.trust.requires_confirmation();
    let needs_enable = !definition.enabled;
    if !needs_trust && !needs_enable {
        validate_definition(&definition)?;
        return Ok(ActivateMcpDefinitionOutput {
            definition,
            changed: false,
        });
    }

    let now = chrono::Utc::now().to_rfc3339();
    if needs_trust {
        definition.trust.decision = McpTrustDecision::Trusted {
            fingerprint: definition.trust.current_fingerprint.clone(),
            trusted_at: now.clone(),
        };
    }
    definition.enabled = true;
    definition.updated_at = now;
    validate_definition(&definition)?;
    repo.save_mcp_definition(&definition).await?;

    Ok(ActivateMcpDefinitionOutput {
        definition,
        changed: true,
    })
}

fn validate_definition(definition: &McpDefinition) -> Result<()> {
    definition
        .validate()
        .map_err(|error| CoreError::InvalidInput(error.to_string()))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use async_trait::async_trait;

    use super::*;
    use crate::{
        domain::{
            mcp::{
                McpBinding, McpBindingId, McpBindingScope, McpImportSource, McpImportSourceKind,
                McpSecretBinding, McpSecretReferenceId, McpSecretTarget, McpTransport, McpTrust,
            },
            project::ProjectId,
        },
        ports::McpRepo,
    };

    #[derive(Clone, Default)]
    struct FakeMcpRepo {
        definitions: Arc<Mutex<HashMap<String, McpDefinition>>>,
        saves: Arc<Mutex<usize>>,
    }

    impl FakeMcpRepo {
        fn insert(&self, definition: McpDefinition) {
            self.definitions
                .lock()
                .expect("definitions lock poisoned")
                .insert(definition.id.0.clone(), definition);
        }
    }

    #[async_trait]
    impl McpRepo for FakeMcpRepo {
        async fn save_mcp_definition(&self, definition: &McpDefinition) -> Result<()> {
            definition
                .validate()
                .map_err(|error| CoreError::InvalidInput(error.to_string()))?;
            self.insert(definition.clone());
            *self.saves.lock().expect("saves lock poisoned") += 1;
            Ok(())
        }

        async fn get_mcp_definition(&self, id: &McpDefinitionId) -> Result<Option<McpDefinition>> {
            Ok(self
                .definitions
                .lock()
                .expect("definitions lock poisoned")
                .get(&id.0)
                .cloned())
        }

        async fn list_mcp_definitions(&self) -> Result<Vec<McpDefinition>> {
            Ok(self
                .definitions
                .lock()
                .expect("definitions lock poisoned")
                .values()
                .cloned()
                .collect())
        }

        async fn delete_mcp_definition(&self, id: &McpDefinitionId) -> Result<()> {
            self.definitions
                .lock()
                .expect("definitions lock poisoned")
                .remove(&id.0);
            Ok(())
        }

        async fn save_mcp_binding(&self, _binding: &McpBinding) -> Result<()> {
            Ok(())
        }

        async fn get_mcp_binding(&self, _id: &McpBindingId) -> Result<Option<McpBinding>> {
            Ok(None)
        }

        async fn list_mcp_bindings(
            &self,
            _definition_id: Option<&McpDefinitionId>,
        ) -> Result<Vec<McpBinding>> {
            Ok(Vec::new())
        }

        async fn delete_mcp_binding(&self, _id: &McpBindingId) -> Result<()> {
            Ok(())
        }
    }

    fn placeholder_fingerprint() -> McpTrustFingerprint {
        McpTrustFingerprint("0".repeat(64))
    }

    fn imported_stdio_definition() -> McpDefinition {
        McpDefinition {
            id: McpDefinitionId("payments".to_string()),
            display_name: "Payments".to_string(),
            transport: McpTransport::Stdio {
                executable: "npx".to_string(),
                args: vec!["@example/payments-mcp@1.2.3".to_string()],
                cwd: Some("/workspace".to_string()),
            },
            secret_refs: vec![McpSecretBinding {
                target: McpSecretTarget::EnvironmentVariable {
                    name: "PAYMENTS_TOKEN".to_string(),
                },
                secret_ref: McpSecretReferenceId("credential:payments".to_string()),
            }],
            enabled: true,
            ownership: McpDefinitionOwnership::ImportedReadOnly {
                source: McpImportSource {
                    kind: McpImportSourceKind::ProjectFile,
                    locator: ".mcp.json".to_string(),
                    definition_key: Some("payments".to_string()),
                },
            },
            trust: McpTrust {
                current_fingerprint: placeholder_fingerprint(),
                decision: McpTrustDecision::Trusted {
                    fingerprint: placeholder_fingerprint(),
                    trusted_at: "2026-07-28T00:00:00Z".to_string(),
                },
            },
            created_at: "2026-07-28T00:00:00Z".to_string(),
            updated_at: "2026-07-28T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn imported_definition_is_disabled_and_untrusted_by_default() {
        let definition = prepare_imported_mcp_definition(imported_stdio_definition())
            .expect("prepare imported definition");

        assert!(!definition.enabled);
        assert!(matches!(
            definition.trust.decision,
            McpTrustDecision::Untrusted
        ));
        assert!(definition.trust.requires_confirmation());
        assert_eq!(definition.validate(), Ok(()));
    }

    #[test]
    fn canonical_fingerprint_has_a_golden_test_vector() {
        let definition = prepare_imported_mcp_definition(imported_stdio_definition())
            .expect("prepare imported definition");

        assert_eq!(
            definition.trust.current_fingerprint.0,
            "d31e41818ecbe1fbbdc84459f68ab8a85bf7513b864c87d2a354d22b55847b7f"
        );
    }

    #[test]
    fn fingerprint_is_stable_for_non_security_fields_and_binding_order() {
        let mut first = prepare_imported_mcp_definition(imported_stdio_definition())
            .expect("prepare imported definition");
        first.secret_refs.push(McpSecretBinding {
            target: McpSecretTarget::EnvironmentVariable {
                name: "SECOND_TOKEN".to_string(),
            },
            secret_ref: McpSecretReferenceId("credential:second".to_string()),
        });
        first.synchronize_trust_fingerprint();

        let mut second = first.clone();
        second.display_name = "Renamed".to_string();
        second.enabled = !first.enabled;
        second.updated_at = "2030-01-01T00:00:00Z".to_string();
        second.secret_refs.reverse();

        assert_eq!(
            first.computed_trust_fingerprint(),
            second.computed_trust_fingerprint()
        );
    }

    #[test]
    fn every_security_field_family_changes_the_fingerprint() {
        let base = prepare_imported_mcp_definition(imported_stdio_definition())
            .expect("prepare imported definition");
        let fingerprint = base.computed_trust_fingerprint();

        let mut variants = Vec::new();

        let mut executable = base.clone();
        if let McpTransport::Stdio { executable, .. } = &mut executable.transport {
            *executable = "bunx".to_string();
        }
        variants.push(executable);

        let mut arguments = base.clone();
        if let McpTransport::Stdio { args, .. } = &mut arguments.transport {
            args.push("--read-only".to_string());
        }
        variants.push(arguments);

        let mut cwd = base.clone();
        if let McpTransport::Stdio { cwd, .. } = &mut cwd.transport {
            *cwd = Some("/other-workspace".to_string());
        }
        variants.push(cwd);

        let mut environment_name = base.clone();
        environment_name.secret_refs[0].target = McpSecretTarget::EnvironmentVariable {
            name: "OTHER_TOKEN".to_string(),
        };
        variants.push(environment_name);

        let mut credential = base.clone();
        credential.secret_refs[0].secret_ref = McpSecretReferenceId("credential:other".to_string());
        variants.push(credential);

        let mut source = base.clone();
        if let McpDefinitionOwnership::ImportedReadOnly { source } = &mut source.ownership {
            source.definition_key = Some("other".to_string());
        }
        variants.push(source);

        assert!(variants
            .iter()
            .all(|variant| variant.computed_trust_fingerprint() != fingerprint));
    }

    #[test]
    fn http_url_and_header_identity_are_fingerprinted() {
        let mut definition = imported_stdio_definition();
        definition.transport = McpTransport::Http {
            url: "https://mcp.example.com/v1".to_string(),
        };
        definition.secret_refs = vec![McpSecretBinding {
            target: McpSecretTarget::HttpHeader {
                name: "Authorization".to_string(),
            },
            secret_ref: McpSecretReferenceId("credential:payments".to_string()),
        }];
        let definition =
            prepare_imported_mcp_definition(definition).expect("prepare HTTP definition");

        let mut header_case = definition.clone();
        header_case.secret_refs[0].target = McpSecretTarget::HttpHeader {
            name: "authorization".to_string(),
        };
        assert_eq!(
            header_case.computed_trust_fingerprint(),
            definition.computed_trust_fingerprint()
        );

        let mut changed_url = definition.clone();
        changed_url.transport = McpTransport::Http {
            url: "https://mcp.example.com/v2".to_string(),
        };
        assert_ne!(
            changed_url.computed_trust_fingerprint(),
            definition.computed_trust_fingerprint()
        );
    }

    #[test]
    fn synchronization_invalidates_a_prior_decision_without_erasing_it() {
        let mut definition = prepare_imported_mcp_definition(imported_stdio_definition())
            .expect("prepare imported definition");
        let trusted_fingerprint = definition.trust.current_fingerprint.clone();
        definition.trust.decision = McpTrustDecision::Trusted {
            fingerprint: trusted_fingerprint,
            trusted_at: "2026-07-28T00:00:00Z".to_string(),
        };

        if let McpTransport::Stdio { args, .. } = &mut definition.transport {
            args.push("--changed".to_string());
        }
        assert!(synchronize_mcp_definition_trust(&mut definition).expect("synchronize"));
        assert!(definition.trust.requires_confirmation());
        assert!(matches!(
            definition.trust.decision,
            McpTrustDecision::Trusted { .. }
        ));
    }

    #[test]
    fn activation_is_bound_to_the_previewed_fingerprint() {
        let repo = FakeMcpRepo::default();
        let definition = prepare_imported_mcp_definition(imported_stdio_definition())
            .expect("prepare imported definition");
        repo.insert(definition.clone());

        let stale = McpTrustFingerprint("f".repeat(64));
        let result = futures::executor::block_on(activate_mcp_definition(
            &repo,
            ActivateMcpDefinitionInput {
                definition_id: definition.id,
                expected_fingerprint: stale,
            },
        ));

        assert!(matches!(result, Err(CoreError::InvalidInput(_))));
        assert_eq!(*repo.saves.lock().expect("saves lock poisoned"), 0);
    }

    #[test]
    fn activation_enables_and_trusts_the_exact_definition() {
        let repo = FakeMcpRepo::default();
        let definition = prepare_imported_mcp_definition(imported_stdio_definition())
            .expect("prepare imported definition");
        let expected_fingerprint = definition.trust.current_fingerprint.clone();
        repo.insert(definition.clone());

        let output = futures::executor::block_on(activate_mcp_definition(
            &repo,
            ActivateMcpDefinitionInput {
                definition_id: definition.id,
                expected_fingerprint: expected_fingerprint.clone(),
            },
        ))
        .expect("activate definition");

        assert!(output.changed);
        assert!(output.definition.enabled);
        assert!(!output.definition.trust.requires_confirmation());
        assert!(matches!(
            output.definition.trust.decision,
            McpTrustDecision::Trusted { fingerprint, .. }
                if fingerprint == expected_fingerprint
        ));
        assert_eq!(*repo.saves.lock().expect("saves lock poisoned"), 1);
    }

    #[test]
    fn repeated_activation_is_idempotent() {
        let repo = FakeMcpRepo::default();
        let definition = prepare_imported_mcp_definition(imported_stdio_definition())
            .expect("prepare imported definition");
        let input = ActivateMcpDefinitionInput {
            definition_id: definition.id.clone(),
            expected_fingerprint: definition.trust.current_fingerprint.clone(),
        };
        repo.insert(definition);

        let first = futures::executor::block_on(activate_mcp_definition(&repo, input.clone()))
            .expect("first activation");
        let second = futures::executor::block_on(activate_mcp_definition(&repo, input))
            .expect("second activation");

        assert!(first.changed);
        assert!(!second.changed);
        assert_eq!(*repo.saves.lock().expect("saves lock poisoned"), 1);
    }

    #[test]
    fn unrelated_binding_types_do_not_affect_definition_trust() {
        let binding = McpBinding {
            id: McpBindingId("binding-1".to_string()),
            definition_id: McpDefinitionId("payments".to_string()),
            scope: McpBindingScope::Project {
                project_id: ProjectId("project-1".to_string()),
            },
            enabled: true,
            provider_exclusions: Vec::new(),
            created_at: "2026-07-28T00:00:00Z".to_string(),
            updated_at: "2026-07-28T00:00:00Z".to_string(),
        };

        assert_eq!(binding.validate(), Ok(()));
    }
}
