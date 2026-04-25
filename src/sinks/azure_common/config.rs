use std::path::PathBuf;
use std::sync::Arc;

#[cfg(test)]
use base64::prelude::*;

use azure_core::http::{ClientMethodOptions};

use azure_core::credentials::{TokenCredential, TokenRequestOptions};
use azure_core::{Error, error::ErrorKind};

use azure_identity::{
    AzureCliCredential, ClientAssertion, ClientAssertionCredential, ClientCertificateCredential,
    ClientCertificateCredentialOptions, ClientSecretCredential, ManagedIdentityCredential,
    ManagedIdentityCredentialOptions, UserAssignedId, WorkloadIdentityCredential,
    WorkloadIdentityCredentialOptions,
};

use vector_lib::{
    configurable::configurable_component,
    sensitive_string::SensitiveString,
};

/// TLS configuration.
#[configurable_component]
#[configurable(metadata(docs::advanced))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct AzureBlobTlsConfig {
    /// Absolute path to an additional CA certificate file.
    ///
    /// The certificate must be in PEM (X.509) format.
    #[serde(alias = "ca_path")]
    #[configurable(metadata(docs::examples = "/path/to/certificate_authority.crt"))]
    #[configurable(metadata(docs::human_name = "CA File Path"))]
    pub ca_file: Option<PathBuf>,
}

/// Azure service principal authentication.
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
#[serde(deny_unknown_fields, untagged)]
pub enum AzureAuthentication {
    #[configurable(metadata(docs::enum_tag_description = "The kind of Azure credential to use."))]
    Specific(SpecificAzureCredential),

    /// Mock credential for testing — returns a static fake token
    #[cfg(test)]
    #[serde(skip)]
    MockCredential,
}

impl Default for AzureAuthentication {
    // This should never be actually used.
    // This is only needed when using Default::default() (such as unit tests),
    // as serde requires `azure_credential_kind` to be specified.
    fn default() -> Self {
        Self::Specific(SpecificAzureCredential::ManagedIdentity {
            user_assigned_managed_identity_id: None,
            user_assigned_managed_identity_id_type: None,
        })
    }
}

#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
#[derive(Default)]
/// User Assigned Managed Identity Types.
pub enum UserAssignedManagedIdentityIdType {
    #[default]
    /// Client ID
    ClientId,
    /// Object ID
    ObjectId,
    /// Resource ID
    ResourceId,
}

/// Specific Azure credential types.
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
#[serde(
    tag = "azure_credential_kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum SpecificAzureCredential {
    /// Use Azure CLI credentials
    #[cfg(not(target_arch = "wasm32"))]
    AzureCli {},

    /// Use certificate credentials
    ClientCertificateCredential {
        /// The [Azure Tenant ID][azure_tenant_id].
        ///
        /// [azure_tenant_id]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
        #[configurable(metadata(docs::examples = "00000000-0000-0000-0000-000000000000"))]
        #[configurable(metadata(docs::examples = "${AZURE_TENANT_ID:?err}"))]
        azure_tenant_id: String,

        /// The [Azure Client ID][azure_client_id].
        ///
        /// [azure_client_id]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
        #[configurable(metadata(docs::examples = "00000000-0000-0000-0000-000000000000"))]
        #[configurable(metadata(docs::examples = "${AZURE_CLIENT_ID:?err}"))]
        azure_client_id: String,

        /// PKCS12 certificate with RSA private key.
        #[configurable(metadata(docs::examples = "path/to/certificate.pfx"))]
        #[configurable(metadata(docs::examples = "${AZURE_CLIENT_CERTIFICATE_PATH:?err}"))]
        certificate_path: PathBuf,

        /// The password for the client certificate, if applicable.
        #[configurable(metadata(docs::examples = "${AZURE_CLIENT_CERTIFICATE_PASSWORD}"))]
        certificate_password: Option<SensitiveString>,
    },

    /// Use client ID/secret credentials
    ClientSecretCredential {
        /// The [Azure Tenant ID][azure_tenant_id].
        ///
        /// [azure_tenant_id]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
        #[configurable(metadata(docs::examples = "00000000-0000-0000-0000-000000000000"))]
        #[configurable(metadata(docs::examples = "${AZURE_TENANT_ID:?err}"))]
        azure_tenant_id: String,

        /// The [Azure Client ID][azure_client_id].
        ///
        /// [azure_client_id]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
        #[configurable(metadata(docs::examples = "00000000-0000-0000-0000-000000000000"))]
        #[configurable(metadata(docs::examples = "${AZURE_CLIENT_ID:?err}"))]
        azure_client_id: String,

        /// The [Azure Client Secret][azure_client_secret].
        ///
        /// [azure_client_secret]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
        #[configurable(metadata(docs::examples = "00-00~000000-0000000~0000000000000000000"))]
        #[configurable(metadata(docs::examples = "${AZURE_CLIENT_SECRET:?err}"))]
        azure_client_secret: SensitiveString,
    },

    /// Use Managed Identity credentials
    ManagedIdentity {
        /// The User Assigned Managed Identity to use.
        #[configurable(metadata(docs::examples = "00000000-0000-0000-0000-000000000000"))]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        user_assigned_managed_identity_id: Option<String>,

        /// The type of the User Assigned Managed Identity ID provided (Client ID, Object ID,
        /// or Resource ID). Defaults to Client ID.
        user_assigned_managed_identity_id_type: Option<UserAssignedManagedIdentityIdType>,
    },

    /// Use Managed Identity with Client Assertion credentials
    ManagedIdentityClientAssertion {
        /// The User Assigned Managed Identity to use for the managed identity.
        #[configurable(metadata(docs::examples = "00000000-0000-0000-0000-000000000000"))]
        #[configurable(metadata(
            docs::examples = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg-vector/providers/Microsoft.ManagedIdentity/userAssignedIdentities/id-vector-uami"
        ))]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        user_assigned_managed_identity_id: Option<String>,

        /// The type of the User Assigned Managed Identity ID provided (Client ID, Object ID, or Resource ID). Defaults to Client ID.
        user_assigned_managed_identity_id_type: Option<UserAssignedManagedIdentityIdType>,

        /// The target Tenant ID to use.
        #[configurable(metadata(docs::examples = "00000000-0000-0000-0000-000000000000"))]
        client_assertion_tenant_id: String,

        /// The target Client ID to use.
        #[configurable(metadata(docs::examples = "00000000-0000-0000-0000-000000000000"))]
        client_assertion_client_id: String,
    },

    /// Use Workload Identity credentials
    WorkloadIdentity {
        /// The [Azure Tenant ID][azure_tenant_id]. Defaults to the value of the environment variable `AZURE_TENANT_ID`.
        ///
        /// [azure_tenant_id]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
        #[configurable(metadata(docs::examples = "00000000-0000-0000-0000-000000000000"))]
        #[configurable(metadata(docs::examples = "${AZURE_TENANT_ID}"))]
        tenant_id: Option<String>,

        /// The [Azure Client ID][azure_client_id]. Defaults to the value of the environment variable `AZURE_CLIENT_ID`.
        ///
        /// [azure_client_id]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
        #[configurable(metadata(docs::examples = "00000000-0000-0000-0000-000000000000"))]
        #[configurable(metadata(docs::examples = "${AZURE_CLIENT_ID}"))]
        client_id: Option<String>,

        /// Path of a file containing a Kubernetes service account token. Defaults to the value of the environment variable `AZURE_FEDERATED_TOKEN_FILE`.
        #[configurable(metadata(
            docs::examples = "/var/run/secrets/azure/tokens/azure-identity-token"
        ))]
        #[configurable(metadata(docs::examples = "${AZURE_FEDERATED_TOKEN_FILE}"))]
        token_file_path: Option<PathBuf>,
    },
}

#[derive(Debug)]
struct ManagedIdentityClientAssertion {
    credential: Arc<dyn TokenCredential>,
    scope: String,
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl ClientAssertion for ManagedIdentityClientAssertion {
    async fn secret(&self, options: Option<ClientMethodOptions<'_>>) -> azure_core::Result<String> {
        Ok(self
            .credential
            .get_token(
                &[&self.scope],
                Some(TokenRequestOptions {
                    method_options: options.unwrap_or_default(),
                }),
            )
            .await?
            .token
            .secret()
            .to_string())
    }
}

impl AzureAuthentication {
    /// Returns the provider for the credentials based on the authentication mechanism chosen.
    pub async fn credential(&self) -> azure_core::Result<Arc<dyn TokenCredential>> {
        match self {
            Self::Specific(specific) => specific.credential().await,

            #[cfg(test)]
            Self::MockCredential => Ok(Arc::new(MockTokenCredential) as Arc<dyn TokenCredential>),
        }
    }
}

impl SpecificAzureCredential {
    /// Returns the provider for the credentials based on the specific credential type.
    pub async fn credential(&self) -> azure_core::Result<Arc<dyn TokenCredential>> {
        let credential: Arc<dyn TokenCredential> = match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::AzureCli {} => AzureCliCredential::new(None)?,

            // requires azure_identity feature 'client_certificate'
            Self::ClientCertificateCredential {
                azure_tenant_id,
                azure_client_id,
                certificate_path,
                certificate_password,
            } => {
                let certificate_bytes: Vec<u8> = std::fs::read(certificate_path).map_err(|e| {
                    Error::with_message(
                        ErrorKind::Credential,
                        format!(
                            "Failed to read certificate file {}: {e}",
                            certificate_path.display()
                        ),
                    )
                })?;

                let mut options: ClientCertificateCredentialOptions =
                    ClientCertificateCredentialOptions::default();
                if let Some(password) = certificate_password {
                    options.password = Some(password.inner().to_string().into());
                }

                ClientCertificateCredential::new(
                    azure_tenant_id.clone(),
                    azure_client_id.clone(),
                    certificate_bytes.into(),
                    Some(options),
                )?
            }

            Self::ClientSecretCredential {
                azure_tenant_id,
                azure_client_id,
                azure_client_secret,
            } => {
                if azure_tenant_id.is_empty() {
                    return Err(Error::with_message(ErrorKind::Credential,
                        "`auth.azure_tenant_id` is blank; either use `auth.azure_credential_kind`, or provide tenant ID, client ID, and secret.".to_string()
                    ));
                }
                if azure_client_id.is_empty() {
                    return Err(Error::with_message(ErrorKind::Credential,
                        "`auth.azure_client_id` is blank; either use `auth.azure_credential_kind`, or provide tenant ID, client ID, and secret.".to_string()
                    ));
                }
                if azure_client_secret.inner().is_empty() {
                    return Err(Error::with_message(ErrorKind::Credential,
                        "`auth.azure_client_secret` is blank; either use `auth.azure_credential_kind`, or provide tenant ID, client ID, and secret.".to_string()
                    ));
                }

                let secret: String = azure_client_secret.inner().into();
                ClientSecretCredential::new(
                    &azure_tenant_id.clone(),
                    azure_client_id.clone(),
                    secret.into(),
                    None,
                )?
            }

            Self::ManagedIdentity {
                user_assigned_managed_identity_id,
                user_assigned_managed_identity_id_type,
            } => {
                let mut options = ManagedIdentityCredentialOptions::default();
                if let Some(id) = user_assigned_managed_identity_id {
                    options.user_assigned_id = match user_assigned_managed_identity_id_type
                        .as_ref()
                        .unwrap_or(&Default::default())
                    {
                        UserAssignedManagedIdentityIdType::ClientId => {
                            Some(UserAssignedId::ClientId(id.clone()))
                        }
                        UserAssignedManagedIdentityIdType::ObjectId => {
                            Some(UserAssignedId::ObjectId(id.clone()))
                        }
                        UserAssignedManagedIdentityIdType::ResourceId => {
                            Some(UserAssignedId::ResourceId(id.clone()))
                        }
                    };
                }
                ManagedIdentityCredential::new(Some(options))?
            }

            Self::ManagedIdentityClientAssertion {
                user_assigned_managed_identity_id,
                user_assigned_managed_identity_id_type,
                client_assertion_tenant_id,
                client_assertion_client_id,
            } => {
                let mut options = ManagedIdentityCredentialOptions::default();
                if let Some(id) = user_assigned_managed_identity_id {
                    options.user_assigned_id = match user_assigned_managed_identity_id_type
                        .as_ref()
                        .unwrap_or(&Default::default())
                    {
                        UserAssignedManagedIdentityIdType::ClientId => {
                            Some(UserAssignedId::ClientId(id.clone()))
                        }
                        UserAssignedManagedIdentityIdType::ObjectId => {
                            Some(UserAssignedId::ObjectId(id.clone()))
                        }
                        UserAssignedManagedIdentityIdType::ResourceId => {
                            Some(UserAssignedId::ResourceId(id.clone()))
                        }
                    };
                }
                let msi: Arc<dyn TokenCredential> = ManagedIdentityCredential::new(Some(options))?;
                let assertion = ManagedIdentityClientAssertion {
                    credential: msi,
                    // Future: make this configurable for sovereign clouds? (no way to test...)
                    scope: "api://AzureADTokenExchange/.default".to_string(),
                };

                ClientAssertionCredential::new(
                    client_assertion_tenant_id.clone(),
                    client_assertion_client_id.clone(),
                    assertion,
                    None,
                )?
            }

            Self::WorkloadIdentity {
                tenant_id,
                client_id,
                token_file_path,
            } => {
                let options = WorkloadIdentityCredentialOptions {
                    tenant_id: tenant_id.clone(),
                    client_id: client_id.clone(),
                    token_file_path: token_file_path.clone(),
                    ..Default::default()
                };

                WorkloadIdentityCredential::new(Some(options))?
            }
        };
        Ok(credential)
    }
}

#[cfg(test)]
#[derive(Debug)]
struct MockTokenCredential;

#[cfg(test)]
#[async_trait::async_trait]
impl TokenCredential for MockTokenCredential {
    async fn get_token(
        &self,
        scopes: &[&str],
        _options: Option<azure_core::credentials::TokenRequestOptions<'_>>,
    ) -> azure_core::Result<azure_core::credentials::AccessToken> {
        let Some(scope) = scopes.first() else {
            return Err(Error::with_message(
                ErrorKind::Credential,
                "no scopes were provided",
            ));
        };

        // serde_json sometimes does and sometimes doesn't preserve order, be careful to sort
        // the claims in alphabetical order to ensure a consistent base64 encoding for testing
        let jwt = serde_json::json!({
            "aud": scope.strip_suffix("/.default").unwrap_or(*scope),
            "exp": 2147483647,
            "iat": 0,
            "iss": "https://sts.windows.net/",
            "nbf": 0,
        });

        // JWTs do not include standard base64 padding.
        // this seemed cleaner than importing a new crates just for this function
        let jwt_base64 = format!(
            "e30.{}.",
            BASE64_STANDARD
                .encode(serde_json::to_string(&jwt).unwrap())
                .trim_end_matches("=")
        )
        .to_string();

        warn!(
            "Using mock token credential, JWT: {}, base64: {}",
            serde_json::to_string(&jwt).unwrap(),
            jwt_base64
        );

        Ok(azure_core::credentials::AccessToken::new(
            jwt_base64,
            azure_core::time::OffsetDateTime::now_utc() + std::time::Duration::from_secs(3600),
        ))
    }
}

#[cfg(test)]
#[tokio::test]
async fn azure_mock_token_credential_test() {
    let credential = MockTokenCredential;
    let access_token = credential
        .get_token(&["https://example.com/.default"], None)
        .await
        .expect("valid credential should return a token");
    assert_eq!(
        access_token.token.secret(),
        "e30.eyJhdWQiOiJodHRwczovL2V4YW1wbGUuY29tIiwiZXhwIjoyMTQ3NDgzNjQ3LCJpYXQiOjAsImlzcyI6Imh0dHBzOi8vc3RzLndpbmRvd3MubmV0LyIsIm5iZiI6MH0."
    );
}
