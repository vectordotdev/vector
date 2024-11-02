
use std::sync::Arc;

use azure_core::auth::TokenCredential;
use vector_lib::configurable::configurable_component;
use vector_lib::schema;
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

// Log Ingestion API version
// const API_VERSION: &str = "2023-01-01";

/// Configuration for the `azure_logs_ingestion` sink.
#[configurable_component(sink(
    "azure_logs_ingestion",
    "Publish log events to the Azure Logs Ingestion API."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct AzureLogsIngestionConfig {
    /// The [Data collection endpoint URI][endpoint] associated with the Log Analytics workspace.
    ///
    /// [endpoint]: https://learn.microsoft.com/en-us/azure/azure-monitor/logs/logs-ingestion-api-overview
    #[configurable(metadata(docs::examples = "https://my-dce-5kyl.eastus-1.ingest.monitor.azure.com"))]
    pub endpoint: String,

    /// The [Data collection rule][dcr] for the Data collection endpoint.
    ///
    /// [dcr]: https://learn.microsoft.com/en-us/azure/azure-monitor/logs/logs-ingestion-api-overview
    #[configurable(metadata(docs::examples = "dcr-000a00a000a00000a000000aa000a0aa"))]
    pub dcr: String,

    /// The [Stream name][stream_name] for the Data collection rule.
    ///
    /// [stream_name]: https://learn.microsoft.com/en-us/azure/azure-monitor/logs/logs-ingestion-api-overview
    #[configurable(metadata(docs::examples = "Custom-MyTable"))]
    pub stream_name: String,

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
            dcr: Default::default(),
            stream_name: Default::default(),
            encoding: Default::default(),
            batch: Default::default(),
            request: Default::default(),
            tls: None,
            acknowledgements: Default::default(),
        }
    }
}

impl AzureLogsIngestionConfig {

    pub(super) async fn build_inner(
        &self,
        cx: SinkContext,
        endpoint: UriSerde,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let endpoint = endpoint.with_default_parts().uri;
        let protocol = get_http_scheme_from_uri(&endpoint).to_string();

        let batch_settings = self
            .batch
            .validate()?
            .limit_max_bytes(MAX_BATCH_SIZE)?
            .into_batcher_settings()?;

        // TODO will need to change this as part of upstream 0.20.0
        // https://github.com/Azure/azure-sdk-for-rust/blob/main/sdk/identity/azure_identity/CHANGELOG.md
        let credential: Arc<dyn TokenCredential> = azure_identity::create_credential()?;

        let tls_settings = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(Some(tls_settings), &cx.proxy)?;

        let service = AzureLogsIngestionService::new(
            client,
            endpoint,
            credential,
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
        // https://my-dce-5kyl.eastus-1.ingest.monitor.azure.com/dataCollectionRules/dcr-000a00a000a00000a000000aa000a0aa/streams/Custom-MyTable?api-version=2023-01-01
        // let endpoint = format!("{}/dataCollectionRules/{}/streams/{}", self.endpoint, self.dcr, self.stream_name).parse()?;
        let endpoint = self.endpoint.parse()?;
        self.build_inner(cx, endpoint).await
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
