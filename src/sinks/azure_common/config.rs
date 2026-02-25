use std::fs::File;
use std::io::Read;
use std::sync::Arc;

use azure_core::error::Error as AzureCoreError;
use tokio::runtime::Handle;
use tokio::task;

use crate::sinks::azure_common::connection_string::{Auth, ParsedConnectionString};
use crate::sinks::azure_common::shared_key_policy::SharedKeyAuthorizationPolicy;
use azure_core::http::{ClientMethodOptions, StatusCode, Url};

use azure_core::credentials::{TokenCredential, TokenRequestOptions};
use azure_core::{Error, error::ErrorKind};

use azure_identity::{
    AzureCliCredential, ClientAssertion, ClientAssertionCredential, ClientSecretCredential,
    ManagedIdentityCredential, ManagedIdentityCredentialOptions, UserAssignedId,
    WorkloadIdentityCredential,
};

use azure_storage_blob::{BlobContainerClient, BlobContainerClientOptions};

use bytes::Bytes;
use futures::FutureExt;
use snafu::Snafu;
use vector_lib::{
    configurable::configurable_component,
    json_size::JsonSize,
    request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata},
    sensitive_string::SensitiveString,
    stream::DriverResponse,
};

use crate::{
    event::{EventFinalizers, EventStatus, Finalizable},
    sinks::{Healthcheck, util::retries::RetryLogic},
    tls::TlsConfig,
};

/// Configuration of the authentication strategy for interacting with Azure services.
#[configurable_component]
#[derive(Clone, Debug, Derivative, Eq, PartialEq)]
#[derivative(Default)]
#[serde(deny_unknown_fields, untagged)]
pub enum AzureAuthentication {
    /// Use client credentials
    #[derivative(Default)]
    ClientSecretCredential {
        /// The [Azure Tenant ID][azure_tenant_id].
        ///
        /// [azure_tenant_id]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
        #[configurable(metadata(docs::examples = "00000000-0000-0000-0000-000000000000"))]
        azure_tenant_id: String,

        /// The [Azure Client ID][azure_client_id].
        ///
        /// [azure_client_id]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
        #[configurable(metadata(docs::examples = "00000000-0000-0000-0000-000000000000"))]
        azure_client_id: String,

        /// The [Azure Client Secret][azure_client_secret].
        ///
        /// [azure_client_secret]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
        #[configurable(metadata(docs::examples = "00-00~000000-0000000~0000000000000000000"))]
        azure_client_secret: SensitiveString,
    },

    /// Use credentials from environment variables
    #[configurable(metadata(docs::enum_tag_description = "The kind of Azure credential to use."))]
    Specific(SpecificAzureCredential),
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

    /// Use Managed Identity credentials
    ManagedIdentity {
        /// The User Assigned Managed Identity (Client ID) to use.
        #[configurable(metadata(docs::examples = "00000000-0000-0000-0000-000000000000"))]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        user_assigned_managed_identity_id: Option<String>,
    },

    /// Use Managed Identity with Client Assertion credentials
    ManagedIdentityClientAssertion {
        /// The User Assigned Managed Identity (Client ID) to use for the managed identity.
        #[configurable(metadata(docs::examples = "00000000-0000-0000-0000-000000000000"))]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        user_assigned_managed_identity_id: Option<String>,

        /// The target Tenant ID to use.
        #[configurable(metadata(docs::examples = "00000000-0000-0000-0000-000000000000"))]
        client_assertion_tenant_id: String,

        /// The target Client ID to use.
        #[configurable(metadata(docs::examples = "00000000-0000-0000-0000-000000000000"))]
        client_assertion_client_id: String,
    },

    /// Use Workload Identity credentials
    WorkloadIdentity {},
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
                let credential: Arc<dyn TokenCredential> = ClientSecretCredential::new(
                    &azure_tenant_id.clone(),
                    azure_client_id.clone(),
                    secret.into(),
                    None,
                )?;
                Ok(credential)
            }

            Self::Specific(specific) => specific.credential().await,
        }
    }
}

impl SpecificAzureCredential {
    /// Returns the provider for the credentials based on the specific credential type.
    pub async fn credential(&self) -> azure_core::Result<Arc<dyn TokenCredential>> {
        let credential: Arc<dyn TokenCredential> = match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::AzureCli {} => AzureCliCredential::new(None)?,

            Self::ManagedIdentity {
                user_assigned_managed_identity_id,
            } => {
                let mut options = ManagedIdentityCredentialOptions::default();
                if let Some(id) = user_assigned_managed_identity_id {
                    options.user_assigned_id = Some(UserAssignedId::ClientId(id.clone()));
                }
                ManagedIdentityCredential::new(Some(options))?
            }

            Self::ManagedIdentityClientAssertion {
                user_assigned_managed_identity_id,
                client_assertion_tenant_id,
                client_assertion_client_id,
            } => {
                let mut options = ManagedIdentityCredentialOptions::default();
                if let Some(id) = user_assigned_managed_identity_id {
                    options.user_assigned_id = Some(UserAssignedId::ClientId(id.clone()));
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

            Self::WorkloadIdentity {} => WorkloadIdentityCredential::new(None)?,
        };
        Ok(credential)
    }
}

#[derive(Debug, Clone)]
pub struct AzureBlobRequest {
    pub blob_data: Bytes,
    pub content_encoding: Option<&'static str>,
    pub content_type: &'static str,
    pub metadata: AzureBlobMetadata,
    pub request_metadata: RequestMetadata,
}

impl Finalizable for AzureBlobRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.metadata.finalizers)
    }
}

impl MetaDescriptive for AzureBlobRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.request_metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.request_metadata
    }
}

#[derive(Clone, Debug)]
pub struct AzureBlobMetadata {
    pub partition_key: String,
    pub count: usize,
    pub byte_size: JsonSize,
    pub finalizers: EventFinalizers,
}

#[derive(Debug, Clone)]
pub struct AzureBlobRetryLogic;

impl RetryLogic for AzureBlobRetryLogic {
    type Error = AzureCoreError;
    type Request = AzureBlobRequest;
    type Response = AzureBlobResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error.http_status() {
            Some(code) => code.is_server_error() || code == StatusCode::TooManyRequests,
            None => false,
        }
    }
}

#[derive(Debug)]
pub struct AzureBlobResponse {
    pub events_byte_size: GroupedCountByteSize,
    pub byte_size: usize,
}

impl DriverResponse for AzureBlobResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.events_byte_size
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.byte_size)
    }
}

#[derive(Debug, Snafu)]
pub enum HealthcheckError {
    #[snafu(display("Invalid connection string specified"))]
    InvalidCredentials,
    #[snafu(display("Container: {:?} not found", container))]
    UnknownContainer { container: String },
    #[snafu(display("Unknown status code: {}", status))]
    Unknown { status: StatusCode },
}

pub fn build_healthcheck(
    container_name: String,
    client: Arc<BlobContainerClient>,
) -> crate::Result<Healthcheck> {
    let healthcheck = async move {
        let resp: crate::Result<()> = match client.get_properties(None).await {
            Ok(_) => Ok(()),
            Err(error) => {
                let code = error.http_status();
                Err(match code {
                    Some(StatusCode::Forbidden) => Box::new(HealthcheckError::InvalidCredentials),
                    Some(StatusCode::NotFound) => Box::new(HealthcheckError::UnknownContainer {
                        container: container_name,
                    }),
                    Some(status) => Box::new(HealthcheckError::Unknown { status }),
                    None => "unknown status code".into(),
                })
            }
        };
        resp
    };

    Ok(healthcheck.boxed())
}

pub fn build_client(
    auth: Option<AzureAuthentication>,
    connection_string: String,
    container_name: String,
    proxy: &crate::config::ProxyConfig,
    tls: Option<TlsConfig>,
) -> crate::Result<Arc<BlobContainerClient>> {
    // Parse connection string without legacy SDK
    let parsed = ParsedConnectionString::parse(&connection_string)
        .map_err(|e| format!("Invalid connection string: {e}"))?;
    // Compose container URL (SAS appended if present)
    let container_url = parsed
        .container_url(&container_name)
        .map_err(|e| format!("Failed to build container URL: {e}"))?;
    let url = Url::parse(&container_url).map_err(|e| format!("Invalid container URL: {e}"))?;

    let mut credential: Option<Arc<dyn TokenCredential>> = None;

    // Prepare options; attach Shared Key policy if needed
    let mut options = BlobContainerClientOptions::default();
    match (parsed.auth(), &auth) {
        (Auth::None, None) => {
            warn!("No authentication method provided, requests will be anonymous");
        }
        (Auth::Sas { .. }, None) => {
            info!("Using SAS token authentication");
        }
        (
            Auth::SharedKey {
                account_name,
                account_key,
            },
            None,
        ) => {
            info!("Using Shared Key authentication");

            let policy = SharedKeyAuthorizationPolicy::new(
                account_name,
                account_key,
                // Use an Azurite-supported storage service version
                String::from("2025-11-05"),
            )
            .map_err(|e| format!("Failed to create SharedKey policy: {e}"))?;
            options
                .client_options
                .per_call_policies
                .push(Arc::new(policy));
        }
        (Auth::None, Some(AzureAuthentication::ClientSecretCredential { .. })) => {
            info!("Using Client Secret authentication");
            let async_credential_result = task::block_in_place(|| {
                Handle::current().block_on(async { auth.unwrap().credential().await.unwrap() })
            });
            credential = Some(async_credential_result);
        }
        (Auth::None, Some(AzureAuthentication::Specific(..))) => {
            info!("Using specific Azure Authentication method");
            let async_credential_result = task::block_in_place(|| {
                Handle::current().block_on(async { auth.unwrap().credential().await.unwrap() })
            });
            credential = Some(async_credential_result);
        }
        (Auth::Sas { .. }, Some(AzureAuthentication::ClientSecretCredential { .. })) => {
            panic!("Cannot use both SAS token and Client ID/Secret at the same time");
        }
        (Auth::SharedKey { .. }, Some(AzureAuthentication::ClientSecretCredential { .. })) => {
            panic!("Cannot use both Shared Key and Client ID/Secret at the same time");
        }
        (Auth::Sas { .. }, Some(AzureAuthentication::Specific(..))) => {
            panic!(
                "Cannot use both SAS token and another Azure Authentication method at the same time"
            );
        }
        (Auth::SharedKey { .. }, Some(AzureAuthentication::Specific(..))) => {
            panic!(
                "Cannot use both Shared Key and another Azure Authentication method at the same time"
            );
        }
    }

    // Use reqwest v0.12 since Azure SDK only implements HttpClient for reqwest::Client v0.12
    let mut reqwest_builder = reqwest_12::ClientBuilder::new();
    let bypass_proxy = {
        let host = url.host_str().unwrap_or("");
        let port = url.port();
        proxy.no_proxy.matches(host)
            || port
                .map(|p| proxy.no_proxy.matches(&format!("{}:{}", host, p)))
                .unwrap_or(false)
    };
    if bypass_proxy || !proxy.enabled {
        // Ensure no proxy (and disable any potential system proxy auto-detection)
        reqwest_builder = reqwest_builder.no_proxy();
    } else {
        if let Some(http) = &proxy.http {
            let p = reqwest_12::Proxy::http(http)
                .map_err(|e| format!("Invalid HTTP proxy URL: {e}"))?;
            // If credentials are embedded in the proxy URL, reqwest will handle them.
            reqwest_builder = reqwest_builder.proxy(p);
        }
        if let Some(https) = &proxy.https {
            let p = reqwest_12::Proxy::https(https)
                .map_err(|e| format!("Invalid HTTPS proxy URL: {e}"))?;
            // If credentials are embedded in the proxy URL, reqwest will handle them.
            reqwest_builder = reqwest_builder.proxy(p);
        }
    }

    if let Some(tls_config) = tls
        && let Some(ca_file) = tls_config.ca_file
    {
        let mut buf = Vec::new();
        File::open(&ca_file)?.read_to_end(&mut buf)?;
        let cert = reqwest_12::Certificate::from_pem(&buf)?;

        warn!("Adding TLS root certificate from {}", ca_file.display());
        reqwest_builder = reqwest_builder.add_root_certificate(cert);
    }

    options.client_options.transport = Some(azure_core::http::Transport::new(std::sync::Arc::new(
        reqwest_builder
            .build()
            .map_err(|e| format!("Failed to build reqwest client: {e}"))?,
    )));
    let client = BlobContainerClient::from_url(url, credential, Some(options))
        .map_err(|e| format!("{e}"))?;
    Ok(Arc::new(client))
}
