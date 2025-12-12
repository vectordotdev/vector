use std::path::PathBuf;

use azure_identity::{ClientAssertionCredentialOptions, ClientSecretCredential, ClientSecretCredentialOptions, TokenCredentialOptions, WorkloadIdentityCredential, WorkloadIdentityCredentialOptions};
use azure_core::credentials::{AccessToken, TokenCredential, TokenRequestOptions};
use azure_core::error::{Error, ErrorKind, ResultExt};

const AZURE_TENANT_ID_ENV_KEY: &str = "AZURE_TENANT_ID";
const AZURE_CLIENT_ID_ENV_KEY: &str = "AZURE_CLIENT_ID";
const AZURE_FEDERATED_TOKEN_FILE: &str = "AZURE_FEDERATED_TOKEN_FILE";
const AZURE_CLIENT_SECRET_ENV_KEY: &str = "AZURE_CLIENT_SECRET";

/// Enables authentication with Workflows Identity if either `AZURE_FEDERATED_TOKEN` or `AZURE_FEDERATED_TOKEN_FILE` is set,
/// otherwise enables authentication to Azure Active Directory using client secret, or a username and password.
///
///
/// Details configured in the following environment variables:
///
/// | Variable                            | Description                                      |
/// |-------------------------------------|--------------------------------------------------|
/// | `AZURE_TENANT_ID`                   | The Azure Active Directory tenant(directory) ID. |
/// | `AZURE_CLIENT_ID`                   | The client(application) ID of an App Registration in the tenant. |
/// | `AZURE_CLIENT_SECRET`               | A client secret that was generated for the App Registration. |
/// | `AZURE_FEDERATED_TOKEN_FILE`        | Path to an federated token file. Variable is present in pods with aks workload identities. |
/// | `AZURE_AUTHORITY_HOST`              | Url for the identity provider to exchange to federated token for an `access_token`. Variable is present in pods with aks workload identities. |
///
/// This credential ultimately uses a or `WorkloadIdentityCredential` a`ClientSecretCredential` to perform the authentication using
/// these details.
/// Please consult the documentation of that class for more details.
#[derive(Clone, Debug)]
pub struct EnvironmentCredential {
    options: TokenCredentialOptions,
}

impl Default for EnvironmentCredential {
    /// Creates an instance of the `EnvironmentCredential` using the default `HttpClient`.
    fn default() -> Self {
        Self::new(TokenCredentialOptions::default())
    }
}

impl EnvironmentCredential {
    /// Creates a new `EnvironmentCredential`.
    pub fn new(options: TokenCredentialOptions) -> Self {
        Self {
            options,
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl TokenCredential for EnvironmentCredential {
    async fn get_token(&self,
            scopes: &[&str],
            options: Option<TokenRequestOptions>,) -> azure_core::Result<AccessToken> {
        let tenant_id = std::env::var(AZURE_TENANT_ID_ENV_KEY)
            .with_context(ErrorKind::Credential, || {
                format!("missing tenant id set in {AZURE_TENANT_ID_ENV_KEY} environment variable")
            })?;
        let client_id = std::env::var(AZURE_CLIENT_ID_ENV_KEY)
            .with_context(ErrorKind::Credential, || {
                format!("missing client id set in {AZURE_CLIENT_ID_ENV_KEY} environment variable")
            })?;

        let federated_token_file = std::env::var(AZURE_FEDERATED_TOKEN_FILE);
        let client_secret = std::env::var(AZURE_CLIENT_SECRET_ENV_KEY);

        /*
        pub struct WorkloadIdentityCredentialOptions {
    /// Options for the [`ClientAssertionCredential`] used by the [`WorkloadIdentityCredential`].
    pub credential_options: ClientAssertionCredentialOptions,

    /// Client ID of the Entra identity. Defaults to the value of the environment variable `AZURE_CLIENT_ID`.
    pub client_id: Option<String>,

    /// Tenant ID of the Entra identity. Defaults to the value of the environment variable `AZURE_TENANT_ID`.
    pub tenant_id: Option<String>,

    /// Path of a file containing a Kubernetes service account token. Defaults to the value of the environment
    /// variable `AZURE_FEDERATED_TOKEN_FILE`.
    pub token_file_path: Option<PathBuf>,
} */

       if let Ok(file) = federated_token_file {
            if let Ok(credential) = WorkloadIdentityCredential::new(
                Some(WorkloadIdentityCredentialOptions {
                    credential_options: ClientAssertionCredentialOptions { credential_options: self.options.clone(), ..Default::default() },
                    client_id: Some(client_id),
                    tenant_id: Some(tenant_id),
                    token_file_path: Some(PathBuf::from(file)),
                })
            ) {
                return credential.get_token(scopes, options).await;
            }
        } else if let Ok(client_secret) = client_secret {
            if let Ok(credential) = ClientSecretCredential::new(
                &tenant_id,
                client_id,
                client_secret.into(),
                Some(ClientSecretCredentialOptions { credential_options: self.options.clone() })
            ) {
                return credential.get_token(scopes, options).await;
            }
        }

        Err(Error::message(
            ErrorKind::Credential,
            "no valid environment credential providers",
        ))
    }
}
