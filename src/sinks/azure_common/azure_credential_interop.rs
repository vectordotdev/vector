/*
 * This file is derived from:
 * https://github.com/Metaswitch/apt-transport-blob/blob/0d2818400300a73a45b8af79f69489f567ae3bc4/src/azure_credential_interop.rs
 *
 * Originally licensed under the MIT License by Alianza, Inc.
 *
 *  MIT License
 *
 *  Copyright (c) Alianza, Inc
 *
 *  Permission is hereby granted, free of charge, to any person obtaining a copy
 *  of this software and associated documentation files (the "Software"), to deal
 *  in the Software without restriction, including without limitation the rights
 *  to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 *  copies of the Software, and to permit persons to whom the Software is
 *  furnished to do so, subject to the following conditions:
 *
 *  The above copyright notice and this permission notice shall be included in all
 *  copies or substantial portions of the Software.
 *
 *  THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 *  IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 *  FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 *  AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 *  LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 *  OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 *  SOFTWARE
 */

// Interop module for Azure Credentials

use azure_core::credentials::TokenCredential;
use azure_core_for_storage::error::{Error, ErrorKind};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(crate) struct TokenCredentialInterop {
    // Credential
    credential: Arc<dyn TokenCredential>,
}

impl TokenCredentialInterop {
    /// Create a new `TokenCredentialInterop` from a `DefaultAzureCredential`
    pub fn new(credential: Arc<dyn TokenCredential>) -> Self {
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
