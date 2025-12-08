use std::sync::Arc;

use azure_core::credentials::TokenCredential;
use azure_core::{
    error::ErrorKind,
    Error,
};

use azure_identity::{
    AzureCliCredential,
    ClientSecretCredential,
    ManagedIdentityCredential,
    WorkloadIdentityCredential,
};
use vector_lib::{
    schema,
    configurable::configurable_component,
    sensitive_string::SensitiveString,
};
use vrl::value::Kind;

use crate::{
    http::{get_http_scheme_from_uri, HttpClient},
    sinks::{
        prelude::*,
        util::{http::HttpStatusRetryLogic, RealtimeSizeBasedDefaultBatchSettings, UriSerde},
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

mod azure_credential_kinds {
    #[cfg(not(target_arch = "wasm32"))]
    pub const AZURE_CLI: &str = "azurecli";
    pub const MANAGED_IDENTITY: &str = "managedidentity";
    pub const WORKLOAD_IDENTITY: &str = "workloadidentity";
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
        #[configurable(metadata(docs::examples = "00000000-0000-0000-0000-000000000000"))]
        azure_tenant_id: String,

        /// The [Azure Client ID][azure_client_id].
        #[configurable(metadata(docs::examples = "00000000-0000-0000-0000-000000000000"))]
        azure_client_id: String,

        /// The [Azure Client Secret][azure_client_secret].
        #[configurable(metadata(docs::examples = "00-00~000000-0000000~0000000000000000000"))]
        azure_client_secret: SensitiveString,
    },

    /// Use credentials from environment variables
    SpecificAzureCredential {
        /// The kind of Azure credential to use.
        #[configurable(metadata(docs::examples = "azurecli"))]
        #[configurable(metadata(docs::examples = "managedidentity"))]
        #[configurable(metadata(docs::examples = "workloadidentity"))]
        azure_credential_kind: String,
    }
}

impl AzureAuthentication {
    /// Returns the provider for the credentials based on the authentication mechanism chosen.
    pub async fn credential(
        &self,
    ) -> azure_core::Result<Arc<dyn TokenCredential>> {
        match self {
            Self::ClientSecretCredential {
                azure_tenant_id,
                azure_client_id,
                azure_client_secret,
            } => {
                let secret: String = azure_client_secret.inner().into();
                let credential = ClientSecretCredential::new(
                    &azure_tenant_id.clone(),
                    azure_client_id.clone(),
                    secret.into(),
                    None,
                )?;
                Ok(credential)
            }

            Self::SpecificAzureCredential {
                azure_credential_kind,
            } => {
                let credential: Arc<dyn TokenCredential> = match azure_credential_kind.replace(' ', "").to_lowercase().as_str() {
                    #[cfg(not(target_arch = "wasm32"))]
                    azure_credential_kinds::AZURE_CLI => AzureCliCredential::new(None)?,
                    azure_credential_kinds::MANAGED_IDENTITY => ManagedIdentityCredential::new(None)?,
                    azure_credential_kinds::WORKLOAD_IDENTITY => WorkloadIdentityCredential::new(None)?,
                    _ => {
                        return Err(Error::with_message(ErrorKind::Credential, || {
                            format!("unknown/unsupported azure_credential_kind `{}`", azure_credential_kind)
                        }))
                    }
                };
                Ok(credential)
            }
        }
    }
}

impl AzureLogsIngestionConfig {
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

        let credential: Arc<dyn TokenCredential> = self.auth.credential().await
            .expect("Failed to create credential");

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
