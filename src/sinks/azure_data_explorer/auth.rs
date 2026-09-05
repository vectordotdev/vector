//! Azure authentication for Azure Data Explorer.
//!
//! Delegates to [`crate::sinks::azure_common::config::AzureAuthentication`] to obtain an
//! [`azure_core::credentials::TokenCredential`], then acquires Bearer tokens scoped to the
//! Kusto API (`https://kusto.kusto.windows.net/.default`).
//!
//! Supports all credential kinds provided by `azure_common`:
//! `client_secret_credential`, `managed_identity`, `workload_identity`,
//! `azure_cli`, `client_certificate_credential`, `managed_identity_client_assertion`.

use std::sync::Arc;

use azure_core::credentials::TokenCredential;

use crate::sinks::azure_common::config::AzureAuthentication;

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

/// Production token provider backed by any `azure_common::AzureAuthentication` variant.
struct AzureCommonTokenProvider {
    credential: Arc<dyn TokenCredential>,
}

#[async_trait::async_trait]
impl TokenProvider for AzureCommonTokenProvider {
    async fn get_bearer_token(&self) -> crate::Result<String> {
        let access_token = self
            .credential
            .get_token(&[KUSTO_SCOPE], None)
            .await
            .map_err(|e| format!("Failed to acquire Azure token for Kusto: {e}"))?;

        Ok(access_token.token.secret().to_string())
    }
}

// ---------------------------------------------------------------------------
// Public auth wrapper
// ---------------------------------------------------------------------------

/// Azure token provider for Azure Data Explorer.
///
/// Wraps any [`AzureAuthentication`] credential variant to acquire Bearer tokens
/// scoped to the Kusto API. Token caching and refresh are handled internally
/// by the Azure SDK.
#[derive(Clone)]
pub(super) struct AzureDataExplorerAuth {
    provider: Arc<dyn TokenProvider>,
}

impl AzureDataExplorerAuth {
    /// Creates a new auth provider from an [`AzureAuthentication`] config value.
    pub(super) async fn new(auth: &AzureAuthentication) -> crate::Result<Self> {
        let credential = auth
            .credential()
            .await
            .map_err(|e| format!("Failed to create Azure credential for Kusto: {e}"))?;

        Ok(Self {
            provider: Arc::new(AzureCommonTokenProvider { credential }),
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
