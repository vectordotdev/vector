use std::{path::PathBuf, sync::Arc};

use azure_core::cloud::CustomConfiguration;
use azure_core::credentials::{
    AccessToken as IdentityAccessToken, Secret as IdentitySecret,
    TokenCredential as IdentityTokenCredential, TokenRequestOptions,
};
use azure_core::error::{Error as AzureCoreError, ErrorKind as AzureCoreErrorKind};
use azure_core::http::{
    ClientMethodOptions, StatusCode, Url, policies::BearerTokenAuthorizationPolicy,
};
use azure_identity::{
    AzureCliCredential, ClientAssertion, ClientAssertionCredential,
    ClientAssertionCredentialOptions, ClientSecretCredential, ManagedIdentityCredential,
    ManagedIdentityCredentialOptions, UserAssignedId, WorkloadIdentityCredential,
    WorkloadIdentityCredentialOptions,
};
use azure_storage_blob::{BlobContainerClient, BlobContainerClientOptions};
use bytes::Bytes;
use futures::FutureExt;
use snafu::Snafu;
use vector_lib::configurable::configurable_component;
use vector_lib::sensitive_string::SensitiveString;
use vector_lib::{
    json_size::JsonSize,
    request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata},
    stream::DriverResponse,
};

use crate::sinks::azure_common::connection_string::{Auth, ParsedConnectionString};
use crate::sinks::azure_common::shared_key_policy::SharedKeyAuthorizationPolicy;
use crate::{
    event::{EventFinalizers, EventStatus, Finalizable},
    sinks::{Healthcheck, util::retries::RetryLogic},
};

/// Azure Blob Storage authentication strategies when using `storage_account`.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "strategy")]
#[configurable(metadata(
    docs::enum_tag_description = "The authentication strategy to use when `storage_account` is set."
))]
pub enum AzureBlobAuthConfig {
    /// Use the DefaultAzureCredential chain (Environment -> Managed Identity -> Azure CLI).
    Default,

    /// Use credentials from environment variables (includes workload identity and client secret).
    Environment,

    /// Use managed identity credentials from IMDS.
    ManagedIdentity {
        /// The user-assigned managed identity client ID.
        #[configurable(metadata(docs::examples = "${AZURE_CLIENT_ID}"))]
        #[configurable(metadata(docs::examples = "00000000-0000-0000-0000-000000000000"))]
        client_id: Option<String>,

        /// The user-assigned managed identity object ID.
        #[configurable(metadata(docs::examples = "00000000-0000-0000-0000-000000000000"))]
        object_id: Option<String>,

        /// The user-assigned managed identity ARM resource ID.
        #[configurable(metadata(
            docs::examples = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.ManagedIdentity/userAssignedIdentities/identity"
        ))]
        msi_res_id: Option<String>,
    },

    /// Use the Azure CLI credential.
    AzureCli,

    /// Use a workload identity token (federated credentials).
    WorkloadIdentity {
        /// The Azure Active Directory tenant ID.
        #[configurable(metadata(docs::examples = "${AZURE_TENANT_ID}"))]
        #[configurable(metadata(docs::examples = "00000000-0000-0000-0000-000000000000"))]
        tenant_id: String,

        /// The Azure Active Directory client (application) ID.
        #[configurable(metadata(docs::examples = "${AZURE_CLIENT_ID}"))]
        #[configurable(metadata(docs::examples = "00000000-0000-0000-0000-000000000000"))]
        client_id: String,

        /// The federated token value.
        #[configurable(metadata(docs::examples = "${AZURE_FEDERATED_TOKEN}"))]
        token: Option<SensitiveString>,

        /// Path to a federated token file.
        #[configurable(metadata(docs::examples = "${AZURE_FEDERATED_TOKEN_FILE}"))]
        token_file: Option<String>,

        /// Override the Azure authority host.
        #[configurable(metadata(docs::examples = "https://login.microsoftonline.com"))]
        authority_host: Option<String>,
    },
}

impl Default for AzureBlobAuthConfig {
    fn default() -> Self {
        Self::Default
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
    connection_string: Option<String>,
    storage_account: Option<String>,
    container_name: String,
    endpoint: Option<String>,
    auth: &AzureBlobAuthConfig,
) -> crate::Result<Arc<BlobContainerClient>> {
    match (connection_string, storage_account) {
        (Some(connection_string), None) => {
            let parsed = ParsedConnectionString::parse(&connection_string)
                .map_err(|e| format!("Invalid connection string: {e}"))?;
            let container_url = parsed
                .container_url(&container_name)
                .map_err(|e| format!("Failed to build container URL: {e}"))?;
            let url =
                Url::parse(&container_url).map_err(|e| format!("Invalid container URL: {e}"))?;

            let mut options = BlobContainerClientOptions::default();
            match parsed.auth() {
                Auth::Sas { .. } | Auth::None => {}
                Auth::SharedKey {
                    account_name,
                    account_key,
                } => {
                    let policy = SharedKeyAuthorizationPolicy::new(
                        account_name,
                        account_key,
                        // Use an Azurite-supported storage service version.
                        String::from("2025-11-05"),
                    )
                    .map_err(|e| format!("Failed to create SharedKey policy: {e}"))?;
                    options
                        .client_options
                        .per_call_policies
                        .push(Arc::new(policy));
                }
            }

            build_container_client(url, options)
        }
        (None, Some(storage_account)) => {
            let container_url = match endpoint {
                Some(endpoint) => format!("{}/{}", endpoint.trim_end_matches('/'), container_name),
                None => format!(
                    "https://{}.blob.core.windows.net/{}",
                    storage_account, container_name
                ),
            };

            let url =
                Url::parse(&container_url).map_err(|e| format!("Invalid container URL: {e}"))?;
            let mut options = BlobContainerClientOptions::default();
            let bearer_policy = BearerTokenAuthorizationPolicy::new(
                build_token_credential(auth)?,
                ["https://storage.azure.com/.default"],
            );
            options
                .client_options
                .per_call_policies
                .push(Arc::new(bearer_policy));

            build_container_client(url, options)
        }
        (Some(_), Some(_)) => {
            Err("only one of `connection_string` or `storage_account` may be set".into())
        }
        (None, None) => Err("either `connection_string` or `storage_account` must be set".into()),
    }
}

fn build_container_client(
    url: Url,
    mut options: BlobContainerClientOptions,
) -> crate::Result<Arc<BlobContainerClient>> {
    // Azure SDK requires a reqwest 0.12 transport implementation.
    options.client_options.transport = Some(azure_core::http::Transport::new(Arc::new(
        reqwest_12::ClientBuilder::new()
            // Avoid macOS system proxy discovery panics in restricted runtimes.
            .no_proxy()
            .build()
            .map_err(|e| format!("Failed to build reqwest client: {e}"))?,
    )));

    let client = BlobContainerClient::from_url(url, None, Some(options))
        .map_err(|e| format!("Failed to create blob container client: {e}"))?;
    Ok(Arc::new(client))
}

#[derive(Debug)]
struct ChainedTokenCredential {
    credentials: Vec<Arc<dyn IdentityTokenCredential>>,
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl IdentityTokenCredential for ChainedTokenCredential {
    async fn get_token(
        &self,
        scopes: &[&str],
        _options: Option<TokenRequestOptions<'_>>,
    ) -> azure_core::Result<IdentityAccessToken> {
        let mut errors = Vec::new();

        for credential in &self.credentials {
            match credential.get_token(scopes, None).await {
                Ok(token) => return Ok(token),
                Err(error) => errors.push(error.to_string()),
            }
        }

        let detail = if errors.is_empty() {
            "no credential sources configured".to_string()
        } else {
            errors.join("\n")
        };

        Err(AzureCoreError::with_message(
            AzureCoreErrorKind::Credential,
            format!(
                "Multiple errors were encountered while attempting to authenticate:\n{}",
                detail
            ),
        ))
    }
}

#[derive(Debug)]
struct StaticAssertion {
    token: String,
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl ClientAssertion for StaticAssertion {
    async fn secret(
        &self,
        _options: Option<ClientMethodOptions<'_>>,
    ) -> azure_core::Result<String> {
        Ok(self.token.clone())
    }
}

fn apply_authority_host(
    client_options: &mut azure_core::http::ClientOptions,
    authority_host: &str,
) {
    let mut cloud = CustomConfiguration::default();
    cloud.authority_host = authority_host.to_string();
    client_options.cloud = Some(Arc::new(cloud.into()));
}

#[derive(Debug, Default)]
struct EnvironmentIdentityVars {
    tenant_id: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
    federated_token_file: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum EnvironmentIdentitySource {
    ServicePrincipal {
        tenant_id: String,
        client_id: String,
        client_secret: String,
    },
    WorkloadIdentity {
        tenant_id: String,
        client_id: String,
        federated_token_file: String,
    },
}

impl EnvironmentIdentityVars {
    fn from_process_env() -> Self {
        Self {
            tenant_id: std::env::var("AZURE_TENANT_ID").ok(),
            client_id: std::env::var("AZURE_CLIENT_ID").ok(),
            client_secret: std::env::var("AZURE_CLIENT_SECRET").ok(),
            federated_token_file: std::env::var("AZURE_FEDERATED_TOKEN_FILE").ok(),
        }
    }
}

fn build_environment_identity_credential(
    required: bool,
) -> crate::Result<Option<Arc<dyn IdentityTokenCredential>>> {
    build_environment_identity_credential_with_vars(
        EnvironmentIdentityVars::from_process_env(),
        required,
    )
}

fn build_environment_identity_credential_with_vars(
    vars: EnvironmentIdentityVars,
    required: bool,
) -> crate::Result<Option<Arc<dyn IdentityTokenCredential>>> {
    let source = resolve_environment_identity_source(vars, required)?;

    match source {
        Some(EnvironmentIdentitySource::ServicePrincipal {
            tenant_id,
            client_id,
            client_secret,
        }) => {
            let credential = ClientSecretCredential::new(
                &tenant_id,
                client_id,
                IdentitySecret::new(client_secret),
                None,
            )?;
            Ok(Some(credential))
        }
        Some(EnvironmentIdentitySource::WorkloadIdentity {
            tenant_id,
            client_id,
            federated_token_file,
        }) => {
            let mut options = WorkloadIdentityCredentialOptions::default();
            options.client_id = Some(client_id);
            options.tenant_id = Some(tenant_id);
            options.token_file_path = Some(PathBuf::from(federated_token_file));
            let credential = WorkloadIdentityCredential::new(Some(options))?;
            Ok(Some(credential))
        }
        None => Ok(None),
    }
}

fn resolve_environment_identity_source(
    vars: EnvironmentIdentityVars,
    required: bool,
) -> crate::Result<Option<EnvironmentIdentitySource>> {
    let EnvironmentIdentityVars {
        tenant_id,
        client_id,
        client_secret,
        federated_token_file,
    } = vars;

    if tenant_id.is_none()
        && client_id.is_none()
        && client_secret.is_none()
        && federated_token_file.is_none()
    {
        if required {
            return Err("environment auth requires AZURE_TENANT_ID and AZURE_CLIENT_ID".into());
        }
        return Ok(None);
    }

    let tenant_id = match tenant_id {
        Some(value) => value,
        None => {
            if required {
                return Err("environment auth requires AZURE_TENANT_ID".into());
            }
            return Ok(None);
        }
    };

    let client_id = match client_id {
        Some(value) => value,
        None => {
            if required {
                return Err("environment auth requires AZURE_CLIENT_ID".into());
            }
            return Ok(None);
        }
    };

    if let Some(secret) = client_secret {
        return Ok(Some(EnvironmentIdentitySource::ServicePrincipal {
            tenant_id,
            client_id,
            client_secret: secret,
        }));
    }

    if let Some(token_file) = federated_token_file {
        if token_file.trim().is_empty() {
            if required {
                return Err(
                    "environment auth requires a non-empty AZURE_FEDERATED_TOKEN_FILE".into(),
                );
            }
            return Ok(None);
        }

        return Ok(Some(EnvironmentIdentitySource::WorkloadIdentity {
            tenant_id,
            client_id,
            federated_token_file: token_file,
        }));
    }

    if required {
        return Err(
            "environment auth requires AZURE_CLIENT_SECRET or AZURE_FEDERATED_TOKEN_FILE".into(),
        );
    }

    Ok(None)
}

fn build_token_credential(
    auth: &AzureBlobAuthConfig,
) -> crate::Result<Arc<dyn IdentityTokenCredential>> {
    let credentials: Vec<Arc<dyn IdentityTokenCredential>> = match auth {
        AzureBlobAuthConfig::Default => {
            let mut credentials = Vec::new();
            if let Some(credential) = build_environment_identity_credential(false)? {
                credentials.push(credential);
            }
            credentials.push(ManagedIdentityCredential::new(None)?);
            credentials.push(AzureCliCredential::new(None)?);
            credentials
        }
        AzureBlobAuthConfig::Environment => {
            let credential = build_environment_identity_credential(true)?
                .ok_or("environment auth requires configured credentials")?;
            vec![credential]
        }
        AzureBlobAuthConfig::ManagedIdentity {
            client_id,
            object_id,
            msi_res_id,
        } => {
            let mut user_assigned_id = None;
            if let Some(client_id) = client_id {
                user_assigned_id = Some(UserAssignedId::ClientId(client_id.clone()));
            }
            if let Some(object_id) = object_id {
                if user_assigned_id.is_some() {
                    return Err(
                        "managed_identity auth only supports one of `client_id`, `object_id`, or `msi_res_id`".into(),
                    );
                }
                user_assigned_id = Some(UserAssignedId::ObjectId(object_id.clone()));
            }
            if let Some(msi_res_id) = msi_res_id {
                if user_assigned_id.is_some() {
                    return Err(
                        "managed_identity auth only supports one of `client_id`, `object_id`, or `msi_res_id`".into(),
                    );
                }
                user_assigned_id = Some(UserAssignedId::ResourceId(msi_res_id.clone()));
            }

            let options = ManagedIdentityCredentialOptions {
                user_assigned_id,
                ..Default::default()
            };
            vec![ManagedIdentityCredential::new(Some(options))?]
        }
        AzureBlobAuthConfig::AzureCli => vec![AzureCliCredential::new(None)?],
        AzureBlobAuthConfig::WorkloadIdentity {
            tenant_id,
            client_id,
            token,
            token_file,
            authority_host,
        } => {
            if token.is_some() && token_file.is_some() {
                return Err(
                    "workload_identity auth supports only one of `token` or `token_file`".into(),
                );
            }

            match (token, token_file) {
                (Some(token), None) => {
                    let token_value = token.inner().to_owned();
                    if token_value.trim().is_empty() {
                        return Err("workload_identity auth requires a non-empty token".into());
                    }

                    let mut options = ClientAssertionCredentialOptions::default();
                    if let Some(authority_host) = authority_host {
                        apply_authority_host(&mut options.client_options, authority_host);
                    }
                    let assertion = StaticAssertion { token: token_value };
                    let credential = ClientAssertionCredential::new(
                        tenant_id.clone(),
                        client_id.clone(),
                        assertion,
                        Some(options),
                    )?;
                    vec![credential]
                }
                (None, Some(token_file)) => {
                    if token_file.trim().is_empty() {
                        return Err("workload_identity auth requires a non-empty token_file".into());
                    }

                    let mut options = WorkloadIdentityCredentialOptions::default();
                    if let Some(authority_host) = authority_host {
                        apply_authority_host(
                            &mut options.credential_options.client_options,
                            authority_host,
                        );
                    }
                    options.tenant_id = Some(tenant_id.clone());
                    options.client_id = Some(client_id.clone());
                    options.token_file_path = Some(PathBuf::from(token_file.clone()));
                    let credential = WorkloadIdentityCredential::new(Some(options))?;
                    vec![credential]
                }
                _ => {
                    return Err(
                        "workload_identity auth requires either `token` or `token_file`".into(),
                    );
                }
            }
        }
    };

    if credentials.is_empty() {
        return Err("no credential sources available for authentication".into());
    }

    Ok(Arc::new(ChainedTokenCredential { credentials }))
}

#[cfg(test)]
mod tests {
    use super::{
        EnvironmentIdentitySource, EnvironmentIdentityVars, resolve_environment_identity_source,
    };

    #[test]
    fn environment_identity_returns_none_when_unset_and_not_required() {
        let result = resolve_environment_identity_source(EnvironmentIdentityVars::default(), false)
            .expect("should not fail when not required");

        assert!(result.is_none());
    }

    #[test]
    fn environment_identity_returns_none_when_partial_and_not_required() {
        let result = resolve_environment_identity_source(
            EnvironmentIdentityVars {
                tenant_id: Some("00000000-0000-0000-0000-000000000000".to_string()),
                client_id: Some("11111111-1111-1111-1111-111111111111".to_string()),
                client_secret: None,
                federated_token_file: None,
            },
            false,
        )
        .expect("should not fail when not required");

        assert!(result.is_none());
    }

    #[test]
    fn environment_identity_requires_tenant_and_client_when_required() {
        let error = resolve_environment_identity_source(EnvironmentIdentityVars::default(), true)
            .expect_err("missing env vars should fail");

        assert_eq!(
            error.to_string(),
            "environment auth requires AZURE_TENANT_ID and AZURE_CLIENT_ID"
        );
    }

    #[test]
    fn environment_identity_accepts_service_principal_credentials() {
        let result = resolve_environment_identity_source(
            EnvironmentIdentityVars {
                tenant_id: Some("00000000-0000-0000-0000-000000000000".to_string()),
                client_id: Some("11111111-1111-1111-1111-111111111111".to_string()),
                client_secret: Some("test-secret".to_string()),
                federated_token_file: None,
            },
            true,
        )
        .expect("service principal env vars should resolve");

        assert_eq!(
            result,
            Some(EnvironmentIdentitySource::ServicePrincipal {
                tenant_id: "00000000-0000-0000-0000-000000000000".to_string(),
                client_id: "11111111-1111-1111-1111-111111111111".to_string(),
                client_secret: "test-secret".to_string(),
            })
        );
    }

    #[test]
    fn environment_identity_accepts_workload_identity_credentials() {
        let result = resolve_environment_identity_source(
            EnvironmentIdentityVars {
                tenant_id: Some("00000000-0000-0000-0000-000000000000".to_string()),
                client_id: Some("11111111-1111-1111-1111-111111111111".to_string()),
                client_secret: None,
                federated_token_file: Some("/var/run/secrets/azure/tokens/token".to_string()),
            },
            true,
        )
        .expect("workload identity env vars should resolve");

        assert_eq!(
            result,
            Some(EnvironmentIdentitySource::WorkloadIdentity {
                tenant_id: "00000000-0000-0000-0000-000000000000".to_string(),
                client_id: "11111111-1111-1111-1111-111111111111".to_string(),
                federated_token_file: "/var/run/secrets/azure/tokens/token".to_string(),
            })
        );
    }

    #[test]
    fn environment_identity_requires_secret_or_token_file_when_required() {
        let error = resolve_environment_identity_source(
            EnvironmentIdentityVars {
                tenant_id: Some("00000000-0000-0000-0000-000000000000".to_string()),
                client_id: Some("11111111-1111-1111-1111-111111111111".to_string()),
                client_secret: None,
                federated_token_file: None,
            },
            true,
        )
        .expect_err("missing secret/token file should fail");

        assert_eq!(
            error.to_string(),
            "environment auth requires AZURE_CLIENT_SECRET or AZURE_FEDERATED_TOKEN_FILE"
        );
    }
}
