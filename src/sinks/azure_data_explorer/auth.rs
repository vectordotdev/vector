//! Azure Entra ID authentication for Azure Data Explorer.
//!
//! Uses [`azure_identity::ClientSecretCredential`] (from the official Azure SDK
//! for Rust) for service-principal client-credentials authentication, rather
//! than a hand-rolled OAuth2 flow.

use std::sync::Arc;

use azure_core::credentials::{Secret, TokenCredential};
use azure_identity::ClientSecretCredential;
use vector_lib::sensitive_string::SensitiveString;

/// Scope for Azure Data Explorer / Kusto API access.
const KUSTO_SCOPE: &str = "https://kusto.kusto.windows.net/.default";

// ---------------------------------------------------------------------------
// Internal trait: allows swapping in a mock for tests without needing to
// construct `azure_core::credentials::AccessToken` (which requires the `time`
// crate's `OffsetDateTime`).
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
trait TokenProvider: Send + Sync {
    async fn get_bearer_token(&self) -> crate::Result<String>;
}

/// Production token provider backed by [`ClientSecretCredential`].
struct EntraTokenProvider {
    credential: Arc<ClientSecretCredential>,
}

#[async_trait::async_trait]
impl TokenProvider for EntraTokenProvider {
    async fn get_bearer_token(&self) -> crate::Result<String> {
        let access_token = self
            .credential
            .get_token(&[KUSTO_SCOPE], None)
            .await
            .map_err(|e| format!("Failed to acquire Azure Entra token: {e}"))?;

        Ok(access_token.token.secret().to_string())
    }
}

// ---------------------------------------------------------------------------
// Public auth wrapper
// ---------------------------------------------------------------------------

/// Azure Entra ID token provider for Azure Data Explorer.
///
/// Wraps [`azure_identity::ClientSecretCredential`] to acquire Bearer tokens
/// via the OAuth2 client-credentials flow.  Token caching and refresh are
/// handled internally by the Azure SDK.
#[derive(Clone)]
pub(super) struct AzureDataExplorerAuth {
    provider: Arc<dyn TokenProvider>,
}

impl AzureDataExplorerAuth {
    /// Creates a new auth provider backed by [`ClientSecretCredential`].
    pub(super) fn new(
        tenant_id: &str,
        client_id: String,
        client_secret: SensitiveString,
    ) -> crate::Result<Self> {
        let secret = Secret::from(client_secret.inner().to_string());
        let credential = ClientSecretCredential::new(tenant_id, client_id, secret, None)
            .map_err(|e| format!("Failed to create Azure credential: {e}"))?;

        Ok(Self {
            provider: Arc::new(EntraTokenProvider { credential }),
        })
    }

    /// Creates a mock auth provider that always returns the given token.
    /// For use in tests only.
    #[cfg(test)]
    pub(super) fn mock(token: impl Into<String>) -> Self {
        Self {
            provider: Arc::new(MockTokenProvider {
                token: token.into(),
            }),
        }
    }

    /// Returns a valid Bearer access token string.
    pub(super) async fn get_token(&self) -> crate::Result<String> {
        self.provider.get_bearer_token().await
    }
}

// ---------------------------------------------------------------------------
// Test-only mock
// ---------------------------------------------------------------------------

#[cfg(test)]
struct MockTokenProvider {
    token: String,
}

#[cfg(test)]
#[async_trait::async_trait]
impl TokenProvider for MockTokenProvider {
    async fn get_bearer_token(&self) -> crate::Result<String> {
        Ok(self.token.clone())
    }
}
