//! Configuration for the `azure_data_explorer` sink.
//!
//! Uses **queued ingestion** (blob upload + queue notification), matching the
//! Fluent Bit `out_azure_kusto` plugin.

use futures::FutureExt;
use vector_lib::{configurable::configurable_component, sensitive_string::SensitiveString};
use vrl::value::Kind;

use super::{
    auth::AzureDataExplorerAuth,
    encoder::AzureDataExplorerEncoder,
    request_builder::AzureDataExplorerRequestBuilder,
    resources::ResourceManager,
    service::{AzureDataExplorerService, QueuedIngestConfig},
    sink::AzureDataExplorerSink,
};
use crate::{
    http::HttpClient,
    sinks::{
        prelude::*,
        util::{BatchConfig, http::http_response_retry_logic},
    },
};

/// Configuration for the `azure_data_explorer` sink.
#[configurable_component(sink(
    "azure_data_explorer",
    "Deliver log events to Azure Data Explorer via queued ingestion."
))]
#[derive(Clone, Debug)]
pub struct AzureDataExplorerConfig {
    /// The Kusto cluster's **ingestion** endpoint URL.
    ///
    /// This is the `ingest-` prefixed URL, e.g.
    /// `https://ingest-mycluster.eastus.kusto.windows.net`.
    #[configurable(metadata(
        docs::examples = "https://ingest-mycluster.eastus.kusto.windows.net",
    ))]
    #[configurable(validation(format = "uri"))]
    pub(super) ingestion_endpoint: String,

    /// The name of the target database.
    #[configurable(metadata(docs::examples = "my_database"))]
    pub(super) database: String,

    /// The name of the target table inside the database.
    #[configurable(metadata(docs::examples = "my_table"))]
    pub(super) table: String,

    /// Azure Entra ID (Azure AD) tenant ID for service-principal authentication.
    #[configurable(metadata(docs::examples = "${AZURE_TENANT_ID}"))]
    pub(super) tenant_id: String,

    /// Azure Entra ID application (client) ID.
    #[configurable(metadata(docs::examples = "${AZURE_CLIENT_ID}"))]
    pub(super) client_id: String,

    /// Azure Entra ID application client secret.
    #[configurable(metadata(docs::examples = "${AZURE_CLIENT_SECRET}"))]
    pub(super) client_secret: SensitiveString,

    /// Optional ingestion mapping reference name.
    ///
    /// When set, the value is passed in the ingestion message's
    /// `jsonMappingReference` property.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "my_mapping"))]
    pub(super) mapping_reference: Option<String>,

    #[configurable(derived)]
    #[serde(default)]
    pub(super) batch: BatchConfig<AzureDataExplorerDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub(super) request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    pub(super) encoding: Transformer,

    /// The compression algorithm to use.
    ///
    /// When enabled, the JSONL payload is gzip-compressed before blob upload
    /// and the blob name ends with `.multijson.gz`.
    #[configurable(derived)]
    #[serde(default = "Compression::gzip_default")]
    pub(super) compression: Compression,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub(super) acknowledgements: AcknowledgementsConfig,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct AzureDataExplorerDefaultBatchSettings;

impl SinkBatchSettings for AzureDataExplorerDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(1_000);
    const MAX_BYTES: Option<usize> = Some(4_000_000); // 4 MB
    const TIMEOUT_SECS: f64 = 30.0;
}

impl GenerateConfig for AzureDataExplorerConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"ingestion_endpoint = "https://ingest-mycluster.eastus.kusto.windows.net"
            database = "my_database"
            table = "my_table"
            tenant_id = "${AZURE_TENANT_ID}"
            client_id = "${AZURE_CLIENT_ID}"
            client_secret = "${AZURE_CLIENT_SECRET}""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "azure_data_explorer")]
impl SinkConfig for AzureDataExplorerConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let batch_settings = self.batch.validate()?.into_batcher_settings()?;

        let request_builder = AzureDataExplorerRequestBuilder {
            encoder: AzureDataExplorerEncoder {
                transformer: self.encoding.clone(),
            },
            compression: self.compression,
        };

        let client = HttpClient::new(None, cx.proxy())?;

        let auth = AzureDataExplorerAuth::new(
            &self.tenant_id,
            self.client_id.clone(),
            self.client_secret.clone(),
        )?;

        // Resource manager handles .get ingestion resources + identity token caching
        let resource_manager = ResourceManager::new(
            auth.clone(),
            client.clone(),
            self.ingestion_endpoint.clone(),
        );

        let queued_config = QueuedIngestConfig {
            database: self.database.clone(),
            table: self.table.clone(),
            mapping_reference: self.mapping_reference.clone(),
            compression: self.compression,
        };

        let service =
            AzureDataExplorerService::new(client.clone(), resource_manager.clone(), queued_config);

        let request_limits = self.request.into_settings();

        let service = ServiceBuilder::new()
            .settings(request_limits, http_response_retry_logic())
            .service(service);

        let sink = AzureDataExplorerSink::new(service, batch_settings, request_builder);

        let healthcheck = healthcheck(self.ingestion_endpoint.clone(), auth).boxed();

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        let requirement = Requirement::empty().optional_meaning("timestamp", Kind::timestamp());
        Input::log().with_schema_requirement(requirement)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

/// Validates credentials and ingestion endpoint reachability by:
/// 1. Acquiring an Entra token (validates service-principal credentials)
/// 2. Executing a lightweight `.show version` management command
async fn healthcheck(
    ingestion_endpoint: String,
    auth: AzureDataExplorerAuth,
) -> crate::Result<()> {
    let token = auth.get_token().await?;

    let mgmt_uri = format!(
        "{}/v1/rest/mgmt",
        ingestion_endpoint.trim_end_matches('/')
    );

    let body = serde_json::json!({
        "csl": ".show version",
        "db": "NetDefaultDB"
    });
    let body_bytes = bytes::Bytes::from(serde_json::to_vec(&body)?);

    let request = http::Request::post(&mgmt_uri)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(hyper::Body::from(body_bytes))?;

    let client = HttpClient::new(None, &Default::default())?;
    let response = client.send(request).await?;
    let status = response.status();

    if status.is_success() {
        Ok(())
    } else if status == http::StatusCode::UNAUTHORIZED || status == http::StatusCode::FORBIDDEN {
        Err(format!(
            "Azure Data Explorer authentication failed (HTTP {}). \
             Verify tenant_id, client_id, and client_secret.",
            status
        )
        .into())
    } else {
        let body = http_body::Body::collect(response.into_body())
            .await?
            .to_bytes();
        let body_str = String::from_utf8_lossy(&body);
        Err(format!(
            "Azure Data Explorer healthcheck failed: HTTP {} - {}",
            status, body_str
        )
        .into())
    }
}
