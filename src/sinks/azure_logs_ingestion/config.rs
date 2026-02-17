use std::sync::Arc;

use azure_core::credentials::{TokenCredential, TokenRequestOptions};
use azure_core::http::ClientMethodOptions;
use azure_core::{Error, error::ErrorKind};

use azure_identity::{
    AzureCliCredential, ClientAssertion, ClientAssertionCredential, ClientSecretCredential,
    ManagedIdentityCredential, ManagedIdentityCredentialOptions, UserAssignedId,
    WorkloadIdentityCredential,
};
use vector_lib::{configurable::configurable_component, schema, sensitive_string::SensitiveString};
use vrl::value::Kind;

use crate::{
    http::{HttpClient, get_http_scheme_from_uri},
    sinks::{
        prelude::*,
        util::{RealtimeSizeBasedDefaultBatchSettings, UriSerde, http::HttpStatusRetryLogic},
    },
};

use super::{
    service::{AzureLogsIngestionResponse, AzureLogsIngestionService},
    sink::AzureLogsIngestionSink,
};

/// Max number of bytes in request body
const MAX_BATCH_SIZE: usize = 30 * 1024 * 1024;

pub(super) fn default_scope() -> String {
    "https://monitor.azure.com/.default".into()
}

pub(super) fn default_timestamp_field() -> String {
    "TimeGenerated".into()
}

/// Configuration for the `azure_logs_ingestion` sink.
#[configurable_component(sink(
    "azure_logs_ingestion",
    "Publish log events to the Azure Monitor Logs Ingestion API."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct AzureLogsIngestionConfig {
    /// The [Data collection endpoint URI][endpoint] associated with the Log Analytics workspace.
    ///
    /// [endpoint]: https://learn.microsoft.com/en-us/azure/azure-monitor/logs/logs-ingestion-api-overview
    #[configurable(metadata(
        docs::examples = "https://my-dce-5kyl.eastus-1.ingest.monitor.azure.com"
    ))]
    pub endpoint: String,

    /// The [Data collection rule immutable ID][dcr_immutable_id] for the Data collection endpoint.
    ///
    /// [dcr_immutable_id]: https://learn.microsoft.com/en-us/azure/azure-monitor/logs/logs-ingestion-api-overview
    #[configurable(metadata(docs::examples = "dcr-000a00a000a00000a000000aa000a0aa"))]
    pub dcr_immutable_id: String,

    /// The [Stream name][stream_name] for the Data collection rule.
    ///
    /// [stream_name]: https://learn.microsoft.com/en-us/azure/azure-monitor/logs/logs-ingestion-api-overview
    #[configurable(metadata(docs::examples = "Custom-MyTable"))]
    pub stream_name: String,

    #[configurable(derived)]
    #[serde(default)]
    pub auth: AzureAuthentication,

    /// [Token scope][token_scope] for dedicated Azure regions.
    ///
    /// [token_scope]: https://learn.microsoft.com/en-us/azure/azure-monitor/logs/logs-ingestion-api-overview
    #[configurable(metadata(docs::examples = "https://monitor.azure.us/.default"))]
    #[configurable(metadata(docs::examples = "https://monitor.azure.cn/.default"))]
    #[serde(default = "default_scope")]
    pub(super) token_scope: String,

    /// The destination field (column) for the timestamp.
    ///
    /// The setting of `log_schema.timestamp_key`, usually `timestamp`, is used as the source.
    /// Most schemas use `TimeGenerated`, but some use `Timestamp` (legacy) or `EventStartTime` (ASIM) [std_columns].
    ///
    /// [std_columns]: https://learn.microsoft.com/en-us/azure/azure-monitor/logs/log-standard-columns#timegenerated
    #[configurable(metadata(docs::examples = "EventStartTime"))]
    #[configurable(metadata(docs::examples = "Timestamp"))]
    #[serde(default = "default_timestamp_field")]
    pub timestamp_field: String,

    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    pub encoding: Transformer,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl Default for AzureLogsIngestionConfig {
    fn default() -> Self {
        Self {
            endpoint: Default::default(),
            dcr_immutable_id: Default::default(),
            stream_name: Default::default(),
            auth: Default::default(),
            token_scope: default_scope(),
            timestamp_field: default_timestamp_field(),
            encoding: Default::default(),
            batch: Default::default(),
            request: Default::default(),
            tls: None,
            acknowledgements: Default::default(),
        }
    }
}

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

impl AzureLogsIngestionConfig {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn build_inner(
        &self,
        cx: SinkContext,
        endpoint: UriSerde,
        dcr_immutable_id: String,
        stream_name: String,
        credential: Arc<dyn TokenCredential>,
        token_scope: String,
        timestamp_field: String,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let endpoint = endpoint.with_default_parts().uri;
        let protocol = get_http_scheme_from_uri(&endpoint).to_string();

        let batch_settings = self
            .batch
            .validate()?
            .limit_max_bytes(MAX_BATCH_SIZE)?
            .into_batcher_settings()?;

        let tls_settings = TlsSettings::from_options(self.tls.as_ref())?;
        let client = HttpClient::new(Some(tls_settings), &cx.proxy)?;

        let service = AzureLogsIngestionService::new(
            client,
            endpoint,
            dcr_immutable_id,
            stream_name,
            credential,
            token_scope,
        )?;
        let healthcheck = service.healthcheck();

        let retry_logic =
            HttpStatusRetryLogic::new(|res: &AzureLogsIngestionResponse| res.http_status);
        let request_settings = self.request.into_settings();
        let service = ServiceBuilder::new()
            .settings(request_settings, retry_logic)
            .service(service);

        let sink = AzureLogsIngestionSink::new(
            batch_settings,
            self.encoding.clone(),
            service,
            timestamp_field,
            protocol,
        );

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }
}

impl_generate_config_from_default!(AzureLogsIngestionConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "azure_logs_ingestion")]
impl SinkConfig for AzureLogsIngestionConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let endpoint: UriSerde = self.endpoint.parse()?;

        let credential: Arc<dyn TokenCredential> = self.auth.credential().await?;

        self.build_inner(
            cx,
            endpoint,
            self.dcr_immutable_id.clone(),
            self.stream_name.clone(),
            credential,
            self.token_scope.clone(),
            self.timestamp_field.clone(),
        )
        .await
    }

    fn input(&self) -> Input {
        let requirements =
            schema::Requirement::empty().optional_meaning("timestamp", Kind::timestamp());

        Input::log().with_schema_requirement(requirements)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}
