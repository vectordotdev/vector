//! Configuration for the `Clickhouse` sink.

use std::{collections::BTreeMap, fmt};

use http::Uri;
use http_1::{
    HeaderMap, HeaderName, HeaderValue, Request, StatusCode,
    header::{AUTHORIZATION, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE},
};
use snafu::{ResultExt, Snafu};
use vector_lib::codecs::encoding::ArrowStreamSerializerConfig;
use vector_lib::codecs::encoding::format::SchemaProvider;
use vector_lib::codecs::{
    BatchEncoder, EncoderKind, JsonSerializerConfig, NewlineDelimitedEncoderConfig,
    encoding::{BatchSerializerConfig, Framer},
};
use vector_lib::stream::BatcherSettings;

use super::{
    arrow,
    request_builder::ClickhouseRequestBuilder,
    service::{ClickhouseRetryLogic, ClickhouseServiceRequestBuilder},
    sink::{ClickhouseSink, PartitionKey},
};
use crate::{
    config::{SinkContext, ValidatedSink},
    http::{
        Auth, MaybeAuth,
        client_v1::{HttpClient, empty_body},
    },
    sinks::{
        prelude::*,
        util::{
            RealtimeSizeBasedDefaultBatchSettings, UriSerde, http::RequestConfig,
            http_v1::HttpService,
        },
    },
    template::{ConfinedTemplate, ConfinementConfig, Template},
};

/// Data format.
///
/// The format used to parse input/output data.
///
/// [formats]: https://clickhouse.com/docs/en/interfaces/formats
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
#[serde(rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)]
pub enum Format {
    #[default]
    /// `JSONEachRow`.
    JsonEachRow,

    /// `JSONAsObject`.
    JsonAsObject,

    /// `JSONAsString`.
    JsonAsString,

    /// `ArrowStream` (beta).
    #[configurable(metadata(status = "beta"))]
    ArrowStream,
}

/// Batch encoding configuration for the `clickhouse` sink.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(tag = "codec", rename_all = "snake_case")]
#[configurable(metadata(
    docs::enum_tag_description = "The codec to use for batch encoding events."
))]
pub enum ClickhouseBatchEncoding {
    /// Encodes events in [Apache Arrow][apache_arrow] IPC streaming format.
    ///
    /// This is the streaming variant of the Arrow IPC format, which writes
    /// a continuous stream of record batches.
    ///
    /// [apache_arrow]: https://arrow.apache.org/
    ArrowStream(ArrowStreamSerializerConfig),
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Format::JsonEachRow => write!(f, "JSONEachRow"),
            Format::JsonAsObject => write!(f, "JSONAsObject"),
            Format::JsonAsString => write!(f, "JSONAsString"),
            Format::ArrowStream => write!(f, "ArrowStream"),
        }
    }
}

/// Configuration for the `clickhouse` sink.
#[configurable_component(sink("clickhouse", "Deliver log data to a ClickHouse database."))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct ClickhouseConfig {
    /// The endpoint of the ClickHouse server.
    #[serde(alias = "host")]
    #[configurable(metadata(docs::examples = "http://localhost:8123"))]
    pub endpoint: UriSerde,

    /// The table that data is inserted into.
    #[configurable(metadata(docs::examples = "mytable"))]
    pub table: Template,

    /// The database that contains the table that data is inserted into.
    #[configurable(metadata(docs::examples = "mydatabase"))]
    pub database: Option<Template>,

    /// The format to parse input data.
    #[serde(default)]
    pub format: Format,

    /// Sets `input_format_skip_unknown_fields`, allowing ClickHouse to discard fields not present in the table schema.
    ///
    /// If left unspecified, use the default provided by the `ClickHouse` server.
    #[serde(default)]
    pub skip_unknown_fields: Option<bool>,

    /// Sets `date_time_input_format` to `best_effort`, allowing ClickHouse to properly parse RFC3339/ISO 8601.
    #[serde(default)]
    pub date_time_best_effort: bool,

    /// Sets `insert_distributed_one_random_shard`, allowing ClickHouse to insert data into a random shard when using Distributed Table Engine.
    #[serde(default)]
    pub insert_random_shard: bool,

    #[serde(default = "Compression::gzip_default")]
    pub compression: Compression,

    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    pub encoding: Transformer,

    /// The batch encoding configuration for encoding events in batches.
    ///
    /// When specified, events are encoded together as a single batch.
    /// This is mutually exclusive with per-event encoding based on the `format` field.
    #[serde(default)]
    pub batch_encoding: Option<ClickhouseBatchEncoding>,

    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    pub auth: Option<Auth>,

    /// HTTP request settings.
    ///
    /// Header values support event templates for inserts. Schema discovery and healthchecks use only static headers.
    #[serde(default)]
    pub request: RequestConfig,

    pub tls: Option<TlsConfig>,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    #[serde(default)]
    pub query_settings: QuerySettingsConfig,

    #[serde(flatten)]
    pub confinement: ConfinementConfig,
}

#[derive(Debug, Snafu)]
pub(super) enum HeaderValidationError {
    #[snafu(display("Authorization header cannot be used with configured authentication"))]
    AuthorizationConflict,
    #[snafu(display(
        "Request header `{name}` is controlled by the ClickHouse sink and cannot be configured"
    ))]
    ReservedHeader { name: String },
    #[snafu(display("Invalid request header name '{name}': {source}"))]
    InvalidHeaderName {
        name: String,
        source: http_1::header::InvalidHeaderName,
    },
    #[snafu(display("Invalid value for request header '{name}': {source}"))]
    InvalidHeaderValue {
        name: String,
        source: http_1::header::InvalidHeaderValue,
    },
}

pub(super) fn validate_headers(headers: &BTreeMap<String, String>) -> crate::Result<HeaderMap> {
    let mut validated = HeaderMap::new();
    for (name, value) in headers {
        let header_name = HeaderName::from_bytes(name.as_bytes())
            .with_context(|_| InvalidHeaderNameSnafu { name })?;
        let header_value =
            HeaderValue::from_str(value).with_context(|_| InvalidHeaderValueSnafu { name })?;
        validated.insert(header_name, header_value);
    }
    Ok(validated)
}

/// Query settings for the `clickhouse` sink.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct QuerySettingsConfig {
    /// Async insert-related settings.
    #[serde(default)]
    pub async_insert_settings: AsyncInsertSettingsConfig,
}

/// Async insert related settings for the `clickhouse` sink.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct AsyncInsertSettingsConfig {
    /// Sets `async_insert`, allowing ClickHouse to queue the inserted data and later flush to table in the background.
    ///
    /// If left unspecified, use the default provided by the `ClickHouse` server.
    #[serde(default)]
    pub enabled: Option<bool>,

    /// Sets `wait_for`, allowing ClickHouse to wait for processing of asynchronous insertion.
    ///
    /// If left unspecified, use the default provided by the `ClickHouse` server.
    #[serde(default)]
    pub wait_for_processing: Option<bool>,

    /// Sets `wait_for_processing_timeout`, to control the timeout for waiting for processing asynchronous insertion.
    ///
    /// If left unspecified, use the default provided by the `ClickHouse` server.
    #[serde(default)]
    pub wait_for_processing_timeout: Option<u64>,

    /// Sets `async_insert_deduplicate`, allowing ClickHouse to perform deduplication when inserting blocks in the replicated table.
    ///
    /// If left unspecified, use the default provided by the `ClickHouse` server.
    #[serde(default)]
    pub deduplicate: Option<bool>,

    /// Sets `async_insert_max_data_size`, the maximum size in bytes of unparsed data collected per query before being inserted.
    ///
    /// If left unspecified, use the default provided by the `ClickHouse` server.
    #[serde(default)]
    pub max_data_size: Option<u64>,

    /// Sets `async_insert_max_query_number`, the maximum number of insert queries before being inserted
    ///
    /// If left unspecified, use the default provided by the `ClickHouse` server.
    #[serde(default)]
    pub max_query_number: Option<u64>,
}

impl_generate_config_from_default!(ClickhouseConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "clickhouse")]
impl SinkConfig for ClickhouseConfig {
    fn confinement_config(&self) -> Option<&crate::template::ConfinementConfig> {
        Some(&self.confinement)
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[async_trait::async_trait]
impl ValidatedSink for ClickhouseConfig {
    type Validated = ValidatedClickhouse;

    fn validate(&self) -> crate::Result<ValidatedClickhouse> {
        // Validate templates can be parsed
        let database = self.database.clone().unwrap_or_else(|| {
            "default"
                .try_into()
                .expect("'default' should be a valid template")
        });

        // For batch_encoding with ArrowStream, validate compatibility
        if let Some(batch_encoding) = &self.batch_encoding {
            if self.format != Format::ArrowStream {
                return Err(format!(
                    "'batch_encoding' is only compatible with 'format: arrow_stream'. Found 'format: {}'.",
                    self.format
                )
                .into());
            }
            let ClickhouseBatchEncoding::ArrowStream(_) = batch_encoding;

            // ArrowStream requires static table and database.
            if self.table.is_dynamic() || database.is_dynamic() {
                return Err(
                    "Arrow codec requires a static table and database. Dynamic schema inference is not supported."
                        .into(),
                );
            }
        }

        // Resolve auth choice
        let auth = self.auth.choose_one(&self.endpoint.auth)?;

        let (headers, template_headers) = self.resolve_headers(auth.as_ref())?;

        // Compute batch settings
        let batch_settings = self.batch.into_batcher_settings()?;

        // Confine templates
        let confined_table =
            self.table
                .clone()
                .confine(&self.confinement, ClickhouseConfig::NAME, "table")?;
        let confined_database =
            database
                .clone()
                .confine(&self.confinement, ClickhouseConfig::NAME, "database")?;

        Ok(ValidatedClickhouse {
            template_headers,
            headers,
            database,
            auth,
            batch_settings,
            confined_table,
            confined_database,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedClickhouse,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let ValidatedClickhouse {
            template_headers,
            headers,
            database,
            auth,
            batch_settings,
            confined_table,
            confined_database,
        } = validated;
        let endpoint = self.endpoint.with_default_parts().uri;
        let tls_settings = TlsSettings::from_options(self.tls.as_ref())?;
        let client = HttpClient::new(tls_settings.into(), &cx.proxy)?;

        let clickhouse_service_request_builder = ClickhouseServiceRequestBuilder {
            headers: headers.clone(),
            auth: auth.clone(),
            endpoint: endpoint.clone(),
            skip_unknown_fields: self.skip_unknown_fields,
            date_time_best_effort: self.date_time_best_effort,
            insert_random_shard: self.insert_random_shard,
            compression: self.compression,
            query_settings: self.query_settings,
        };

        let service: HttpService<ClickhouseServiceRequestBuilder, PartitionKey> =
            HttpService::new(client.clone(), clickhouse_service_request_builder);

        let request_limits = self.request.tower.into_settings();

        let service = ServiceBuilder::new()
            .settings(request_limits, ClickhouseRetryLogic)
            .service(service);

        // Resolve the encoding strategy (format + encoder) based on configuration.
        // This happens here in build because Arrow schema fetching requires network access.
        let (format, encoder_kind) = self
            .resolve_strategy(&client, &endpoint, database, auth.as_ref(), headers)
            .await?;

        let request_builder = ClickhouseRequestBuilder {
            compression: self.compression,
            encoder: (self.encoding.clone(), encoder_kind),
        };

        // Use pre-computed batch settings and confined templates
        let sink = ClickhouseSink::new(
            *batch_settings,
            service,
            confined_database.clone(),
            confined_table.clone(),
            template_headers.clone(),
            format,
            request_builder,
        );

        let healthcheck = Box::pin(healthcheck(client, endpoint, auth.clone(), headers.clone()));
        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }
}

impl ClickhouseConfig {
    /// Resolves the encoding strategy (format + encoder) based on configuration.
    ///
    /// This method determines the appropriate ClickHouse format and Vector encoder
    /// based on the user's configuration, ensuring they are consistent.
    async fn resolve_strategy(
        &self,
        client: &HttpClient,
        endpoint: &Uri,
        database: &Template,
        auth: Option<&Auth>,
        headers: &HeaderMap,
    ) -> crate::Result<(Format, vector_lib::codecs::EncoderKind)> {
        if let Some(batch_encoding) = &self.batch_encoding {
            // Validate that batch_encoding is only compatible with ArrowStream format
            if self.format != Format::ArrowStream {
                return Err(format!(
                    "'batch_encoding' is only compatible with 'format: arrow_stream'. Found 'format: {}'.",
                    self.format
                )
                .into());
            }

            let ClickhouseBatchEncoding::ArrowStream(arrow_config) = batch_encoding;
            let mut arrow_config = arrow_config.clone();

            self.resolve_arrow_schema(
                client,
                endpoint.to_string(),
                database,
                auth,
                headers,
                &mut arrow_config,
            )
            .await?;

            let resolved_batch_config = BatchSerializerConfig::ArrowStream(arrow_config);
            let batch_serializer = resolved_batch_config.build_batch_serializer()?;
            let encoder = EncoderKind::Batch(BatchEncoder::new(batch_serializer));

            return Ok((Format::ArrowStream, encoder));
        }

        let encoder = EncoderKind::Framed(Box::new(Encoder::<Framer>::new(
            NewlineDelimitedEncoderConfig.build().into(),
            JsonSerializerConfig::default().build().into(),
        )));

        Ok((self.format, encoder))
    }

    async fn resolve_arrow_schema(
        &self,
        client: &HttpClient,
        endpoint: String,
        database: &Template,
        auth: Option<&Auth>,
        headers: &HeaderMap,
        config: &mut ArrowStreamSerializerConfig,
    ) -> crate::Result<()> {
        if self.table.is_dynamic() || database.is_dynamic() {
            return Err(
                "Arrow codec requires a static table and database. Dynamic schema inference is not supported."
                    .into(),
            );
        }

        let table_str = self.table.get_ref();
        let database_str = database.get_ref();

        debug!("Fetching schema for table {database_str}.{table_str} at startup.");

        let provider = arrow::ClickHouseSchemaProvider::new(
            client.clone(),
            endpoint,
            database_str.to_string(),
            table_str.to_string(),
            auth.cloned(),
            headers.clone(),
        );

        let schema = provider
            .get_schema()
            .await
            .map_err(|e| format!("Failed to fetch schema for {database_str}.{table_str}: {e}."))?;

        config.schema = Some(schema);

        debug!(
            "Successfully fetched Arrow schema with {} fields.",
            config.schema.as_ref().map_or(0, |s| s.fields().len())
        );

        Ok(())
    }

    fn resolve_headers(
        &self,
        auth: Option<&Auth>,
    ) -> crate::Result<(HeaderMap, BTreeMap<String, ConfinedTemplate>)> {
        // Validate names and configured values before splitting out event templates.
        let configured_headers = validate_headers(&self.request.headers)?;
        snafu::ensure!(
            auth.is_none() || !configured_headers.contains_key(AUTHORIZATION),
            AuthorizationConflictSnafu
        );

        // ensure headers don't have any reserved headers set by the clickhouse sink
        for name in configured_headers.keys() {
            snafu::ensure!(
                name != CONTENT_TYPE && name != CONTENT_LENGTH && name != CONTENT_ENCODING,
                ReservedHeaderSnafu {
                    name: name.as_str()
                }
            );
        }
        let (static_headers, template_headers) = self.request.split_headers();

        let headers = validate_headers(&static_headers)?;
        let template_headers = template_headers
            .into_iter()
            .map(|(name, template)| {
                template
                    .confine(&self.confinement, Self::NAME, "request.headers")
                    .map(|template| (name, template))
            })
            .collect::<crate::Result<BTreeMap<_, _>>>()?;

        Ok((headers, template_headers))
    }
}

/// Purely validated ClickHouse sink configuration.
///
/// This type captures all validation results that can be computed purely from
/// configuration without network/filesystem/credentials/async operations.
/// The actual sink building consumes these values without recomputing them.
#[derive(Clone, Debug)]
pub struct ValidatedClickhouse {
    /// Confined event templates for insert headers.
    template_headers: BTreeMap<String, ConfinedTemplate>,
    /// Parsed static HTTP headers.
    headers: HeaderMap,
    /// The database template, validated.
    database: Template,
    /// Resolved auth (pure validation without network).
    auth: Option<Auth>,
    /// Batch settings computed during preparation.
    batch_settings: BatcherSettings,
    /// Confined table template.
    confined_table: ConfinedTemplate,
    /// Confined database template.
    confined_database: ConfinedTemplate,
}

fn get_healthcheck_uri(endpoint: &Uri) -> String {
    let mut uri = endpoint.to_string();
    if !uri.ends_with('/') {
        uri.push('/');
    }
    uri.push_str("?query=SELECT%201");
    uri
}

async fn healthcheck(
    client: HttpClient,
    endpoint: Uri,
    auth: Option<Auth>,
    headers: HeaderMap,
) -> crate::Result<()> {
    let uri = get_healthcheck_uri(&endpoint);
    let mut request = Request::get(uri).body(empty_body())?;
    request.headers_mut().extend(headers);

    if let Some(auth) = auth {
        auth.apply_v1(&mut request);
    }

    let response = client.send(request).await?;

    match response.status() {
        StatusCode::OK => Ok(()),
        status => Err(HealthcheckError::unexpected_status(status).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ValidatedSink;
    use crate::template::ConfinementConfig;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<ClickhouseConfig>();
    }

    #[test]
    fn resolves_static_and_template_headers() {
        let config: ClickhouseConfig = serde_yaml::from_str(
            r"
            endpoint: http://localhost:8123
            table: events
            request:
              retry_attempts: 3
              headers:
                X-Static: static-value
                X-Custom: prefix-{{ dynamic_value }}
            ",
        )
        .unwrap();
        let validated = config.validate().unwrap();
        assert_eq!(config.request.tower.retry_attempts, 3);
        assert_eq!(validated.headers["x-static"], "static-value");
        assert!(!validated.headers.contains_key("x-custom"));
        assert!(validated.template_headers.contains_key("X-Custom"));
    }

    #[test]
    fn rejects_invalid_and_conflicting_headers() {
        for (name, value) in [
            // bad header
            ("bad header", "value"),
            // bad header value
            ("X-Custom", "secret\r\nvalue"),
            // reserved headers
            ("Content-Type", "custom/type"),
            ("content-length", "123"),
            ("CONTENT-ENCODING", "gzip"),
            // unconfined template
            ("X-Custom", "{{ custom_value }}"),
        ] {
            let mut config = create_test_config(Format::JsonEachRow, None);
            config.request.headers.insert(name.into(), value.into());
            assert!(config.validate().is_err(), "accepted header {name}");
        }

        // make sure both auth and the custom authorization header can't be set at the same time
        // while the auth can be configured through both the auth config value or the endpoint
        for endpoint_auth in [false, true] {
            let mut config = create_test_config(Format::JsonEachRow, None);
            config
                .request
                .headers
                .insert("authorization".into(), "Bearer {{ token }}".into());
            assert!(config.validate().is_ok());

            if endpoint_auth {
                config.endpoint = "http://user:pass@localhost:8123".parse().unwrap();
            } else {
                config.auth = Some(Auth::Bearer {
                    token: "configured".to_owned().into(),
                });
            }

            assert!(matches!(
                config
                    .validate()
                    .unwrap_err()
                    .downcast_ref::<HeaderValidationError>(),
                Some(HeaderValidationError::AuthorizationConflict)
            ));
        }
    }

    #[test]
    fn confinement_rejects_unconfined_table() {
        let template = Template::try_from("{{ table }}").unwrap();
        let config = ConfinementConfig::default();
        let result = template.confine(&config, "clickhouse", "table");
        assert!(result.is_err());
    }

    #[test]
    fn confinement_opt_out_allows_unconfined_table() {
        let template = Template::try_from("{{ table }}").unwrap();
        let config = ConfinementConfig {
            dangerously_allow_unconfined_template_resolution: true,
        };
        let result = template.confine(&config, "clickhouse", "table");
        assert!(result.is_ok());
    }

    #[test]
    fn confinement_allows_prefixed_table() {
        let template = Template::try_from("events-{{ env }}").unwrap();
        let config = ConfinementConfig::default();
        let result = template.confine(&config, "clickhouse", "table");
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_healthcheck_uri() {
        assert_eq!(
            get_healthcheck_uri(&"http://localhost:8123".parse().unwrap()),
            "http://localhost:8123/?query=SELECT%201"
        );
        assert_eq!(
            get_healthcheck_uri(&"http://localhost:8123/".parse().unwrap()),
            "http://localhost:8123/?query=SELECT%201"
        );
        assert_eq!(
            get_healthcheck_uri(&"http://localhost:8123/path/".parse().unwrap()),
            "http://localhost:8123/path/?query=SELECT%201"
        );
    }

    /// Codecs other than `arrow_stream` must be rejected at parse time, since
    /// `ClickhouseBatchEncoding` only exposes the `arrow_stream` variant.
    #[cfg(feature = "codecs-parquet")]
    #[test]
    fn batch_encoding_rejects_unsupported_codec() {
        let err = serde_yaml::from_str::<ClickhouseConfig>(
            r"
            endpoint: http://localhost:8123
            table: test_table
            batch_encoding:
              codec: parquet
            ",
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("parquet"),
            "expected error to mention the offending codec, got: {err}"
        );
    }

    /// Helper to create a minimal `ClickhouseConfig` for testing
    fn create_test_config(
        format: Format,
        batch_encoding: Option<ClickhouseBatchEncoding>,
    ) -> ClickhouseConfig {
        ClickhouseConfig {
            endpoint: "http://localhost:8123"
                .parse::<http::Uri>()
                .unwrap()
                .try_into()
                .unwrap(),
            table: "test_table".try_into().unwrap(),
            database: Some("test_db".try_into().unwrap()),
            format,
            batch_encoding,
            ..Default::default()
        }
    }

    #[test]
    fn test_format_selection_without_batch_encoding() {
        // When batch_encoding is None, the configured format should be used
        let configs = vec![
            Format::JsonEachRow,
            Format::JsonAsObject,
            Format::JsonAsString,
            Format::ArrowStream,
        ];

        for format in configs {
            let config = create_test_config(format, None);

            assert!(
                config.batch_encoding.is_none(),
                "batch_encoding should be None for format {format:?}"
            );
            assert_eq!(
                config.format, format,
                "format should match configured value"
            );
        }
    }

    #[test]
    fn preparation_error_on_invalid_batch_encoding_combination() {
        let config = ClickhouseConfig {
            endpoint: "http://localhost:8123"
                .parse::<http::Uri>()
                .unwrap()
                .try_into()
                .unwrap(),
            table: "test_table".try_into().unwrap(),
            format: Format::JsonEachRow, // Incompatible with batch_encoding
            batch_encoding: Some(ClickhouseBatchEncoding::ArrowStream(
                vector_lib::codecs::encoding::ArrowStreamSerializerConfig::default(),
            )),
            ..Default::default()
        };

        let result = config.validate();
        assert!(
            result.is_err(),
            "Preparation should fail for incompatible format/batch_encoding"
        );
        let error = result.unwrap_err().to_string();
        assert!(
            error.contains("'batch_encoding' is only compatible"),
            "Error message should mention incompatibility: {error}"
        );
    }

    #[test]
    fn prepares_valid_config() {
        let config = ClickhouseConfig {
            endpoint: "http://localhost:8123"
                .parse::<http::Uri>()
                .unwrap()
                .try_into()
                .unwrap(),
            table: "test_table".try_into().unwrap(),
            database: Some("test_db".try_into().unwrap()),
            format: Format::JsonEachRow,
            ..Default::default()
        };

        let validated = config.validate().expect("preparation should succeed");
        assert_eq!(validated.database.get_ref(), "test_db");
        assert!(validated.auth.is_none()); // Default has no auth
        // Verify the confined templates retained the validated values.
        assert_eq!(validated.confined_table.to_string(), "test_table");
        assert_eq!(validated.confined_database.to_string(), "test_db");
    }

    #[test]
    fn rejects_incompatible_batch_encoding() {
        let config = ClickhouseConfig {
            endpoint: "http://localhost:8123"
                .parse::<http::Uri>()
                .unwrap()
                .try_into()
                .unwrap(),
            table: "test_table".try_into().unwrap(),
            format: Format::JsonEachRow, // Incompatible with batch_encoding
            batch_encoding: Some(ClickhouseBatchEncoding::ArrowStream(
                vector_lib::codecs::encoding::ArrowStreamSerializerConfig::default(),
            )),
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("'batch_encoding' is only compatible"));
    }

    #[test]
    fn rejects_dynamic_targets_with_arrow_batch_encoding() {
        // Dynamic table (with a static prefix, so it passes confinement)
        let config = ClickhouseConfig {
            endpoint: "http://localhost:8123"
                .parse::<http::Uri>()
                .unwrap()
                .try_into()
                .unwrap(),
            table: "events-{{ tenant }}".try_into().unwrap(),
            format: Format::ArrowStream,
            batch_encoding: Some(ClickhouseBatchEncoding::ArrowStream(
                vector_lib::codecs::encoding::ArrowStreamSerializerConfig::default(),
            )),
            ..Default::default()
        };

        let result = config.validate();
        assert!(
            result.is_err(),
            "Expected validation to reject dynamic table with ArrowStream"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("static table and database"),
            "Error should mention static requirement: {err}"
        );

        // Dynamic database
        let config = ClickhouseConfig {
            endpoint: "http://localhost:8123"
                .parse::<http::Uri>()
                .unwrap()
                .try_into()
                .unwrap(),
            table: "test_table".try_into().unwrap(),
            database: Some("events-{{ tenant }}".try_into().unwrap()),
            format: Format::ArrowStream,
            batch_encoding: Some(ClickhouseBatchEncoding::ArrowStream(
                vector_lib::codecs::encoding::ArrowStreamSerializerConfig::default(),
            )),
            ..Default::default()
        };

        let result = config.validate();
        assert!(
            result.is_err(),
            "Expected validation to reject dynamic database with ArrowStream"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("static table and database"),
            "Error should mention static requirement: {err}"
        );
    }

    #[test]
    fn accepts_static_targets_with_arrow_batch_encoding() {
        let config = ClickhouseConfig {
            endpoint: "http://localhost:8123"
                .parse::<http::Uri>()
                .unwrap()
                .try_into()
                .unwrap(),
            table: "test_table".try_into().unwrap(),
            database: Some("test_db".try_into().unwrap()),
            format: Format::ArrowStream,
            batch_encoding: Some(ClickhouseBatchEncoding::ArrowStream(
                vector_lib::codecs::encoding::ArrowStreamSerializerConfig::default(),
            )),
            ..Default::default()
        };

        config.validate().expect("static targets should validate");
    }

    #[test]
    fn rejects_unconfined_template() {
        let config = ClickhouseConfig {
            endpoint: "http://localhost:8123"
                .parse::<http::Uri>()
                .unwrap()
                .try_into()
                .unwrap(),
            table: "{{ table }}".try_into().unwrap(), // No static prefix
            format: Format::JsonEachRow,
            ..Default::default()
        };

        let result = config.validate();
        assert!(
            result.is_err(),
            "Expected preparation to reject unconfined template"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("confinement") || err.contains("prefix"),
            "Error should mention confinement/prefix: {err}"
        );
    }
}
