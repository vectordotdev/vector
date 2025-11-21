// Interop module for Azure Credentials

use azure_core::credentials::TokenCredential;
use azure_core_for_storage::error::{Error, ErrorKind};
use azure_identity::DefaultAzureCredential;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(crate) struct TokenCredentialInterop {
    // Credential
    credential: Arc<DefaultAzureCredential>,
}

impl TokenCredentialInterop {
    /// Create a new `TokenCredentialInterop` from a `DefaultAzureCredential`
    pub fn new(credential: Arc<DefaultAzureCredential>) -> Self {
        Self { credential }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl azure_core_for_storage::auth::TokenCredential for TokenCredentialInterop {
    async fn get_token(
        &self,
        scopes: &[&str],
    ) -> azure_core_for_storage::Result<azure_core_for_storage::auth::AccessToken> {
        let access_token = self
            .credential
            .get_token(scopes, None)
            .await
            .map_err(|err| Error::new(ErrorKind::Credential, err))?;

        // Construct an old AccessToken from the information in the new AccessToken.
        let secret = access_token.token.secret().to_string();
        let access_token = azure_core_for_storage::auth::AccessToken {
            token: secret.into(),
            expires_on: access_token.expires_on,
        };

        // Return the new AccessToken
        Ok(access_token)
    }

    async fn clear_cache(&self) -> azure_core_for_storage::Result<()> {
        Ok(())
    }
}
