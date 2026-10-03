//! Configuration for the `azure_data_explorer` sink.
//!
//! Supports two ingestion modes:
//!
//! - **`streaming`** (default): `POST /v1/rest/ingest/{db}/{table}?streamFormat=MultiJSON`
//!   Use the plain cluster URL (no `ingest-` prefix). Best for low-latency, small payloads.
//!   Requires [streaming ingestion policy] to be enabled on the table.
//!
//! - **`queued`**: Upload payload to Azure Blob Storage then enqueue an ingestion notification.
//!   Use the `ingest-` prefixed URL. Handles large payloads (up to 4 GB per blob).
//!
//! Events can be routed to different tables using:
//! - `table_field` - an event field whose value is the target table name (highest priority)
//! - `table` - a [Template] that is rendered per-event (supports `{{ field }}` syntax)
//! - `default_table` - a static fallback when routing cannot resolve a table name
//!
//! [streaming ingestion policy]: https://learn.microsoft.com/en-us/kusto/management/streaming-ingestion-policy

use std::time::Duration;

use futures::FutureExt;
use vector_lib::configurable::configurable_component;
use vrl::value::Kind;

use super::{
    auth::AzureDataExplorerAuth,
    encoder::AzureDataExplorerEncoder,
    request_builder::AzureDataExplorerRequestBuilder,
    resources::ResourceManager,
    service::{AzureDataExplorerService, IngestConfig},
    sink::{AdxPartitioner, AzureDataExplorerSink},
};
use crate::{
    http::HttpClient,
    sinks::{
        azure_common::config::AzureAuthentication,
        prelude::*,
        util::{BatchConfig, http::http_response_retry_logic},
    },
    template::Template,
};

// ---------------------------------------------------------------------------
// Ingestion method
// ---------------------------------------------------------------------------

/// Ingestion mode for Azure Data Explorer.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum IngestionMethod {
    /// Streaming ingestion via `POST /v1/rest/ingest/{db}/{table}?streamFormat=MultiJSON`.
    ///
    /// Requires the plain cluster URL (no `ingest-` prefix) in `ingestion_endpoint`.
    /// Requires [streaming ingestion policy] on the target table.
    ///
    /// [streaming ingestion policy]: https://learn.microsoft.com/en-us/kusto/management/streaming-ingestion-policy
    #[default]
    Streaming,

    /// Queued ingestion via Azure Blob Storage + Azure Queue Storage.
    ///
    /// Requires the `ingest-` prefixed URL in `ingestion_endpoint`.
    /// Supports payloads up to 4 GB per batch.
    Queued,
}

// ---------------------------------------------------------------------------
// Batch settings
// ---------------------------------------------------------------------------

/// Batch settings for streaming ingestion (low-latency, 4 MB max per Microsoft guidance).
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct StreamingBatchSettings;

impl SinkBatchSettings for StreamingBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(500);
    const MAX_BYTES: Option<usize> = Some(3_900_000);
    const TIMEOUT_SECS: f64 = 10.0;
}

/// Batch settings for queued ingestion (matching Fluent Bit defaults: 200 MB / 30 min).
///
/// Hard ceiling of 4 GB per blob is enforced at config validation time.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct QueuedBatchSettings;

impl SinkBatchSettings for QueuedBatchSettings {
    const MAX_EVENTS: Option<usize> = None;
    const MAX_BYTES: Option<usize> = Some(200_000_000); // 200 MB default (Fluent Bit default)
    const TIMEOUT_SECS: f64 = 1800.0; // 30 minutes (Fluent Bit default)
}

/// Maximum allowed blob size for queued ingestion (matches Fluent Bit's `MAX_FILE_SIZE`).
pub(super) const QUEUED_MAX_BYTES_HARD_LIMIT: usize = 4_000_000_000;

// ---------------------------------------------------------------------------
// Main config struct
// ---------------------------------------------------------------------------

/// Configuration for the `azure_data_explorer` sink.
#[configurable_component(sink(
    "azure_data_explorer",
    "Deliver log events to Azure Data Explorer (Kusto) via streaming or queued ingestion."
))]
#[derive(Clone, Debug)]
pub struct AzureDataExplorerConfig {
    /// The Kusto cluster endpoint URL.
    ///
    /// For **streaming** ingestion: the plain cluster URL without the `ingest-` prefix,
    /// e.g. `https://mycluster.eastus.kusto.windows.net`.
    ///
    /// For **queued** ingestion: the `ingest-` prefixed URL,
    /// e.g. `https://ingest-mycluster.eastus.kusto.windows.net`.
    #[configurable(metadata(
        docs::examples = "https://mycluster.eastus.kusto.windows.net",
        docs::examples = "https://ingest-mycluster.eastus.kusto.windows.net",
    ))]
    #[configurable(validation(format = "uri"))]
    pub(super) ingestion_endpoint: String,

    /// The name of the target database.
    #[configurable(metadata(docs::examples = "my_database"))]
    pub(super) database: String,

    /// The ingestion mode: `streaming` (default) or `queued`.
    #[configurable(derived)]
    #[serde(default)]
    pub(super) ingestion_method: IngestionMethod,

    // ---- Table routing ----

    /// The target table. Supports [template syntax][template] for dynamic routing,
    /// e.g. `{{ kubernetes.namespace }}_logs` or a static `my_table`.
    ///
    /// At least one of `table`, `table_field`, or `default_table` must be set.
    ///
    /// [template]: https://vector.dev/docs/reference/configuration/template-syntax/
    #[configurable(metadata(
        docs::examples = "my_table",
        docs::examples = "{{ kubernetes.namespace }}_logs",
    ))]
    #[serde(default)]
    pub(super) table: Option<Template>,

    /// An event field whose string value is used as the target ADX table name.
    ///
    /// Takes precedence over `table` when the field is present in the event.
    /// Commonly set to `adx_table`.
    #[configurable(metadata(docs::examples = "adx_table"))]
    #[serde(default)]
    pub(super) table_field: Option<String>,

    /// Default table name used when `table_field` is absent or `table` template
    /// rendering fails.
    ///
    /// When set alongside `table_field` or a dynamic `table` template, this acts
    /// as the fallback so events are never silently dropped.
    #[configurable(metadata(docs::examples = "default_logs"))]
    #[serde(default)]
    pub(super) default_table: Option<String>,

    // ---- Auth ----

    /// Azure authentication configuration.
    ///
    /// Supports `client_secret_credential`, `managed_identity`, `workload_identity`,
    /// `azure_cli`, `client_certificate_credential`, and `managed_identity_client_assertion`.
    #[configurable(derived)]
    pub(super) auth: AzureAuthentication,

    // ---- Ingestion options ----

    /// Optional ingestion mapping reference name (`mappingName` query parameter for streaming,
    /// `jsonMappingReference` in the queued ingestion message).
    ///
    /// The named [JSON mapping] must already exist on the table in ADX.
    ///
    /// [JSON mapping]: https://learn.microsoft.com/en-us/kusto/management/mappings?view=azure-data-explorer
    #[serde(default)]
    #[configurable(metadata(docs::examples = "my_mapping"))]
    pub(super) mapping_reference: Option<String>,

    /// How often (in seconds) to refresh the ingestion resources (blob/queue SAS URIs and
    /// identity token) from the ADX cluster. Only applicable for `queued` ingestion.
    ///
    /// Defaults to 3600 (1 hour), matching Fluent Bit's default.
    #[serde(default = "default_ingestion_resources_refresh_interval")]
    pub(super) ingestion_resources_refresh_interval_secs: u64,

    // ---- Batching, encoding, compression ----

    /// Batch configuration.
    ///
    /// For streaming ingestion the defaults are 500 events / 3.9 MB / 10 s.
    ///
    /// For queued ingestion, the recommended settings to match Fluent Bit defaults are
    /// `max_bytes = 200_000_000` (200 MB) and `timeout_secs = 1800` (30 min), with no
    /// event count cap. The hard maximum for queued ingestion is 4,000,000,000 bytes (4 GB).
    #[configurable(derived)]
    #[serde(default)]
    pub(super) batch: BatchConfig<StreamingBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub(super) request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    pub(super) encoding: Transformer,

    /// Compression algorithm.
    ///
    /// For streaming ingestion, gzip sets `Content-Encoding: gzip`.
    /// For queued ingestion, gzip compresses the blob (`.multijson.gz` extension).
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

fn default_ingestion_resources_refresh_interval() -> u64 {
    3600
}

impl GenerateConfig for AzureDataExplorerConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"ingestion_endpoint = "https://mycluster.eastus.kusto.windows.net"
            database = "my_database"
            table = "my_table"

            [auth]
            azure_credential_kind = "client_secret_credential"
            azure_tenant_id = "${AZURE_TENANT_ID}"
            azure_client_id = "${AZURE_CLIENT_ID}"
            azure_client_secret = "${AZURE_CLIENT_SECRET}"
            "#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "azure_data_explorer")]
impl SinkConfig for AzureDataExplorerConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        self.validate()?;

        let client = HttpClient::new(None, cx.proxy())?;
        let auth = AzureDataExplorerAuth::new(&self.auth).await?;

        let partitioner = AdxPartitioner {
            table_field: self.table_field.clone(),
            table: self.table.clone(),
            default_table: self.default_table.clone(),
        };

        let request_builder = AzureDataExplorerRequestBuilder {
            encoder: AzureDataExplorerEncoder {
                transformer: self.encoding.clone(),
            },
            compression: self.compression,
        };

        let ingest_config = IngestConfig {
            ingestion_endpoint: self.ingestion_endpoint.clone(),
            database: self.database.clone(),
            mapping_reference: self.mapping_reference.clone(),
            compression: self.compression,
        };

        let request_limits = self.request.into_settings();

        let (sink, healthcheck) = match self.ingestion_method {
            IngestionMethod::Streaming => {
                let batch_settings = self.batch.validate()?.into_batcher_settings()?;

                let service = AzureDataExplorerService::new_streaming(
                    client.clone(),
                    auth.clone(),
                    ingest_config.clone(),
                );
                let service = ServiceBuilder::new()
                    .settings(request_limits, http_response_retry_logic())
                    .service(service);

                let sink =
                    AzureDataExplorerSink::new(service, batch_settings, request_builder, partitioner);

                let healthcheck = healthcheck_streaming(self.ingestion_endpoint.clone(), auth).boxed();
                (VectorSink::from_event_streamsink(sink), healthcheck)
            }

            IngestionMethod::Queued => {
                // Validate queued-specific constraints
                if let Some(max_bytes) = self.batch.max_bytes {
                    if max_bytes > QUEUED_MAX_BYTES_HARD_LIMIT {
                        return Err(format!(
                            "batch.max_bytes ({max_bytes}) exceeds the queued ingestion hard \
                             limit of {QUEUED_MAX_BYTES_HARD_LIMIT} bytes (~4 GB)"
                        )
                        .into());
                    }
                }

                // When user has not configured any batch field, apply queued defaults
                // (200 MB / 30 min / no event cap) matching Fluent Bit behavior.
                // Otherwise, honour the user's explicit settings.
                let batch_settings = if self.batch.max_bytes.is_none()
                    && self.batch.max_events.is_none()
                    && self.batch.timeout_secs.is_none()
                {
                    BatchConfig::<QueuedBatchSettings>::default()
                        .validate()?
                        .into_batcher_settings()?
                } else {
                    self.batch.validate()?.into_batcher_settings()?
                };

                let resource_manager = ResourceManager::new(
                    auth.clone(),
                    client.clone(),
                    self.ingestion_endpoint.clone(),
                    Duration::from_secs(self.ingestion_resources_refresh_interval_secs),
                );

                let service = AzureDataExplorerService::new_queued(
                    client.clone(),
                    auth.clone(),
                    ingest_config.clone(),
                    resource_manager.clone(),
                );
                let service = ServiceBuilder::new()
                    .settings(request_limits, http_response_retry_logic())
                    .service(service);

                let sink =
                    AzureDataExplorerSink::new(service, batch_settings, request_builder, partitioner);

                let healthcheck =
                    healthcheck_queued(self.ingestion_endpoint.clone(), auth).boxed();
                (VectorSink::from_event_streamsink(sink), healthcheck)
            }
        };

        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        let requirement = Requirement::empty().optional_meaning("timestamp", Kind::timestamp());
        Input::log().with_schema_requirement(requirement)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl AzureDataExplorerConfig {
    /// Validates the configuration at build time.
    fn validate(&self) -> crate::Result<()> {
        if self.table.is_none() && self.table_field.is_none() && self.default_table.is_none() {
            return Err(
                "At least one of `table`, `table_field`, or `default_table` must be set".into(),
            );
        }

        if self.table_field.is_some() && self.default_table.is_none() {
            warn!(
                message = "No `default_table` is configured. Events without the `table_field` \
                           field will be dropped.",
            );
        }

        Ok(())
    }

}

// ---------------------------------------------------------------------------
// Healthchecks
// ---------------------------------------------------------------------------

/// Streaming healthcheck: acquires a token and calls `.show version` on the cluster.
async fn healthcheck_streaming(
    ingestion_endpoint: String,
    auth: AzureDataExplorerAuth,
) -> crate::Result<()> {
    let token = auth.get_token().await?;
    run_show_version(&ingestion_endpoint, &token).await
}

/// Queued healthcheck: acquires a token and calls `.show version` on the ingest endpoint.
async fn healthcheck_queued(
    ingestion_endpoint: String,
    auth: AzureDataExplorerAuth,
) -> crate::Result<()> {
    let token = auth.get_token().await?;
    run_show_version(&ingestion_endpoint, &token).await
}

async fn run_show_version(endpoint: &str, token: &str) -> crate::Result<()> {
    let mgmt_uri = format!("{}/v1/rest/mgmt", endpoint.trim_end_matches('/'));

    let body = serde_json::json!({
        "csl": ".show version",
        "db": "NetDefaultDB"
    });
    let body_bytes = bytes::Bytes::from(serde_json::to_vec(&body)?);

    let request = http::Request::post(&mgmt_uri)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .body(hyper::Body::from(body_bytes))?;

    let client = HttpClient::new(None, &Default::default())?;
    let response = client.send(request).await?;
    let status = response.status();

    if status.is_success() {
        Ok(())
    } else if status == http::StatusCode::UNAUTHORIZED || status == http::StatusCode::FORBIDDEN {
        Err(format!(
            "Azure Data Explorer authentication failed (HTTP {}). \
             Verify your `auth` configuration.",
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
