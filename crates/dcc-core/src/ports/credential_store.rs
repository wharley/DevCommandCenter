use std::fmt;

use async_trait::async_trait;
use thiserror::Error;
use zeroize::Zeroize;

use crate::domain::mcp::McpSecretReferenceId;

const MAX_SECRET_BYTES: usize = 64 * 1024;

/// Secret bytes that cannot be serialized and are redacted from `Debug`.
/// The owned buffer is zeroized when dropped.
pub struct SecretValue(Vec<u8>);

impl SecretValue {
    pub fn new(value: impl Into<Vec<u8>>) -> CredentialStoreResult<Self> {
        let value = value.into();
        if value.is_empty() {
            return Err(CredentialStoreError::EmptySecret);
        }
        if value.len() > MAX_SECRET_BYTES {
            return Err(CredentialStoreError::SecretTooLarge);
        }
        Ok(Self(value))
    }

    pub fn expose_secret(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretValue([REDACTED])")
    }
}

impl Drop for SecretValue {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum CredentialStoreError {
    #[error("credential reference is invalid")]
    InvalidReference,
    #[error("secret must not be empty")]
    EmptySecret,
    #[error("secret exceeds the supported size limit")]
    SecretTooLarge,
    #[error("operating-system credential store is unavailable")]
    Unavailable,
    #[error("operating-system credential store is locked or access was denied")]
    AccessDenied,
    #[error("credential reference is ambiguous")]
    Ambiguous,
    #[error("credential data is corrupt")]
    CorruptEntry,
    #[error("credential operation is not supported by this system store")]
    Unsupported,
    #[error("credential operation failed")]
    OperationFailed,
}

pub type CredentialStoreResult<T> = std::result::Result<T, CredentialStoreError>;

#[async_trait]
pub trait CredentialStore: Send + Sync {
    /// Creates or replaces a secret under an opaque reference.
    async fn store_secret(
        &self,
        reference: &McpSecretReferenceId,
        secret: SecretValue,
    ) -> CredentialStoreResult<()>;

    /// Resolves a secret for backend-only use. This value must never be
    /// returned through a renderer-facing contract.
    async fn resolve_secret(
        &self,
        reference: &McpSecretReferenceId,
    ) -> CredentialStoreResult<Option<SecretValue>>;

    /// Deletes a credential explicitly and returns whether it existed.
    async fn delete_secret(&self, reference: &McpSecretReferenceId) -> CredentialStoreResult<bool>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_value_debug_is_redacted() {
        let secret = SecretValue::new(b"payment-token".to_vec()).expect("create secret");
        assert_eq!(format!("{secret:?}"), "SecretValue([REDACTED])");
        assert!(!format!("{secret:?}").contains("payment-token"));
    }

    #[test]
    fn secret_value_rejects_empty_and_oversized_buffers() {
        assert!(matches!(
            SecretValue::new(Vec::new()),
            Err(CredentialStoreError::EmptySecret)
        ));
        assert!(matches!(
            SecretValue::new(vec![0; MAX_SECRET_BYTES + 1]),
            Err(CredentialStoreError::SecretTooLarge)
        ));
    }
}
