use std::sync::Arc;

#[cfg(any(test, feature = "test-support"))]
use std::{collections::HashMap, sync::Mutex};

use async_trait::async_trait;
use keyring::{Entry, Error as KeyringError};

use dcc_core::{
    domain::mcp::McpSecretReferenceId,
    ports::{CredentialStore, CredentialStoreError, CredentialStoreResult, SecretValue},
};

pub const DCC_MCP_CREDENTIAL_SERVICE: &str = "com.devcommandcenter.app.mcp";
const MAX_REFERENCE_CHARS: usize = 512;

#[derive(Clone, Debug)]
pub struct SystemCredentialStore {
    service: Arc<str>,
}

impl Default for SystemCredentialStore {
    fn default() -> Self {
        Self {
            service: Arc::from(DCC_MCP_CREDENTIAL_SERVICE),
        }
    }
}

impl SystemCredentialStore {
    pub fn new(service: impl Into<String>) -> CredentialStoreResult<Self> {
        let service = service.into();
        validate_identifier(&service)?;
        Ok(Self {
            service: Arc::from(service),
        })
    }

    async fn run<T: Send + 'static>(
        operation: impl FnOnce() -> CredentialStoreResult<T> + Send + 'static,
    ) -> CredentialStoreResult<T> {
        tokio::task::spawn_blocking(operation)
            .await
            .map_err(|_| CredentialStoreError::OperationFailed)?
    }
}

#[async_trait]
impl CredentialStore for SystemCredentialStore {
    async fn store_secret(
        &self,
        reference: &McpSecretReferenceId,
        secret: SecretValue,
    ) -> CredentialStoreResult<()> {
        validate_reference(reference)?;
        let service = self.service.clone();
        let reference = reference.0.clone();
        Self::run(move || {
            let entry = Entry::new(&service, &reference).map_err(map_keyring_error)?;
            entry
                .set_secret(secret.expose_secret())
                .map_err(map_keyring_error)
        })
        .await
    }

    async fn resolve_secret(
        &self,
        reference: &McpSecretReferenceId,
    ) -> CredentialStoreResult<Option<SecretValue>> {
        validate_reference(reference)?;
        let service = self.service.clone();
        let reference = reference.0.clone();
        Self::run(move || {
            let entry = Entry::new(&service, &reference).map_err(map_keyring_error)?;
            match entry.get_secret() {
                Ok(secret) => SecretValue::new(secret).map(Some),
                Err(KeyringError::NoEntry) => Ok(None),
                Err(error) => Err(map_keyring_error(error)),
            }
        })
        .await
    }

    async fn delete_secret(&self, reference: &McpSecretReferenceId) -> CredentialStoreResult<bool> {
        validate_reference(reference)?;
        let service = self.service.clone();
        let reference = reference.0.clone();
        Self::run(move || {
            let entry = Entry::new(&service, &reference).map_err(map_keyring_error)?;
            match entry.delete_credential() {
                Ok(()) => Ok(true),
                Err(KeyringError::NoEntry) => Ok(false),
                Err(error) => Err(map_keyring_error(error)),
            }
        })
        .await
    }
}

/// Deterministic fake for application and repository tests. It is not wired
/// into production composition.
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Debug, Default)]
pub struct InMemoryCredentialStore {
    secrets: Arc<Mutex<HashMap<String, SecretValue>>>,
}

#[cfg(any(test, feature = "test-support"))]
#[async_trait]
impl CredentialStore for InMemoryCredentialStore {
    async fn store_secret(
        &self,
        reference: &McpSecretReferenceId,
        secret: SecretValue,
    ) -> CredentialStoreResult<()> {
        validate_reference(reference)?;
        self.secrets
            .lock()
            .map_err(|_| CredentialStoreError::OperationFailed)?
            .insert(reference.0.clone(), secret);
        Ok(())
    }

    async fn resolve_secret(
        &self,
        reference: &McpSecretReferenceId,
    ) -> CredentialStoreResult<Option<SecretValue>> {
        validate_reference(reference)?;
        self.secrets
            .lock()
            .map_err(|_| CredentialStoreError::OperationFailed)?
            .get(&reference.0)
            .map(|secret| SecretValue::new(secret.expose_secret().to_vec()))
            .transpose()
    }

    async fn delete_secret(&self, reference: &McpSecretReferenceId) -> CredentialStoreResult<bool> {
        validate_reference(reference)?;
        Ok(self
            .secrets
            .lock()
            .map_err(|_| CredentialStoreError::OperationFailed)?
            .remove(&reference.0)
            .is_some())
    }
}

fn validate_reference(reference: &McpSecretReferenceId) -> CredentialStoreResult<()> {
    validate_identifier(&reference.0)
}

fn validate_identifier(value: &str) -> CredentialStoreResult<()> {
    let length = value.chars().count();
    if value.trim().is_empty()
        || value.contains('\0')
        || value.contains('\n')
        || value.contains('\r')
        || length > MAX_REFERENCE_CHARS
    {
        return Err(CredentialStoreError::InvalidReference);
    }
    Ok(())
}

fn map_keyring_error(error: KeyringError) -> CredentialStoreError {
    match error {
        KeyringError::NoEntry => CredentialStoreError::OperationFailed,
        KeyringError::NoDefaultStore => CredentialStoreError::Unavailable,
        KeyringError::NoStorageAccess(_) => CredentialStoreError::AccessDenied,
        KeyringError::Ambiguous(_) => CredentialStoreError::Ambiguous,
        KeyringError::BadEncoding(_)
        | KeyringError::BadDataFormat(_, _)
        | KeyringError::BadStoreFormat(_) => CredentialStoreError::CorruptEntry,
        KeyringError::NotSupportedByStore(_) => CredentialStoreError::Unsupported,
        KeyringError::TooLong(_, _) | KeyringError::Invalid(_, _) => {
            CredentialStoreError::OperationFailed
        }
        KeyringError::PlatformFailure(_) => CredentialStoreError::OperationFailed,
        _ => CredentialStoreError::OperationFailed,
    }
}

#[cfg(test)]
mod tests {
    use dcc_core::ports::CredentialStore;

    use super::*;

    fn reference(value: &str) -> McpSecretReferenceId {
        McpSecretReferenceId(value.to_string())
    }

    #[tokio::test]
    async fn in_memory_store_creates_replaces_resolves_and_deletes() {
        let store = InMemoryCredentialStore::default();
        let reference = reference("credential:figma");

        assert!(store
            .resolve_secret(&reference)
            .await
            .expect("resolve missing")
            .is_none());
        store
            .store_secret(
                &reference,
                SecretValue::new(b"first".to_vec()).expect("first secret"),
            )
            .await
            .expect("create secret");
        let first = store
            .resolve_secret(&reference)
            .await
            .expect("resolve first")
            .expect("first exists");
        assert_eq!(first.expose_secret(), b"first");

        store
            .store_secret(
                &reference,
                SecretValue::new(b"replacement".to_vec()).expect("replacement secret"),
            )
            .await
            .expect("replace secret");
        let replacement = store
            .resolve_secret(&reference)
            .await
            .expect("resolve replacement")
            .expect("replacement exists");
        assert_eq!(replacement.expose_secret(), b"replacement");

        assert!(store
            .delete_secret(&reference)
            .await
            .expect("delete secret"));
        assert!(!store
            .delete_secret(&reference)
            .await
            .expect("delete missing"));
        assert!(store
            .resolve_secret(&reference)
            .await
            .expect("resolve deleted")
            .is_none());
    }

    #[tokio::test]
    async fn invalid_references_fail_without_exposing_input() {
        let store = InMemoryCredentialStore::default();
        let invalid = reference("credential\ninjected");
        let error = store
            .store_secret(
                &invalid,
                SecretValue::new(b"secret".to_vec()).expect("secret"),
            )
            .await
            .expect_err("invalid reference must fail");

        assert_eq!(error, CredentialStoreError::InvalidReference);
        assert!(!error.to_string().contains("injected"));
    }

    #[test]
    fn keyring_errors_are_reduced_to_audit_safe_categories() {
        assert_eq!(
            map_keyring_error(KeyringError::NoDefaultStore),
            CredentialStoreError::Unavailable
        );
        assert_eq!(
            map_keyring_error(KeyringError::BadEncoding(b"raw-secret".to_vec())),
            CredentialStoreError::CorruptEntry
        );
    }
}
