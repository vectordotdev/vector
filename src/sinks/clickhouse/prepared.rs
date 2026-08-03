//! Prepared/validated ClickHouse sink implementation.
//!
//! This module provides `ValidatedClickhouse`, which captures all pure validation
//! results that can be computed without network/filesystem access. The build method
//! consumes these validated values without recomputing them.

use http::{Request, StatusCode, Uri};
use hyper::Body;
use vector_lib::config::AcknowledgementsConfig;
use vector_lib::stream::BatcherSettings;

use super::{
    config::{ClickhouseBatchEncoding, ClickhouseConfig, Format},
    request_builder::ClickhouseRequestBuilder,
    service::{ClickhouseRetryLogic, ClickhouseServiceRequestBuilder},
    sink::{ClickhouseSink, PartitionKey},
};
use crate::{
    config::{Input, SinkContext, ValidateSink},
    http::{Auth, HttpClient, MaybeAuth},
    sinks::{prelude::*, util::http::HttpService},
    template::{ConfinedTemplate, ConfinementConfig, Template},
};

/// Purely validated ClickHouse sink configuration.
///
/// This type captures all validation results that can be computed purely from
/// configuration without network/filesystem/credentials/async operations.
/// The actual sink building consumes these values without recomputing them.
#[derive(Clone, Debug)]
pub struct ValidatedClickhouse {
    /// Validated configuration derived from the original config.
    config: ClickhouseConfig,
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

impl ValidatedClickhouse {
    /// Creates a new validated ClickHouse sink.
    ///
    /// This method performs pure validation without side effects.
    pub fn from_config(config: &ClickhouseConfig) -> crate::Result<Self> {
        // Validate templates can be parsed (this is pure)
        let database = config.database.clone().unwrap_or_else(|| {
            "default"
                .try_into()
                .expect("'default' should be a valid template")
        });

        // For batch_encoding with ArrowStream, validate compatibility (pure check)
        if let Some(batch_encoding) = &config.batch_encoding {
            if config.format != Format::ArrowStream {
                return Err(format!(
                    "'batch_encoding' is only compatible with 'format: arrow_stream'. Found 'format: {}'.",
                    config.format
                )
                .into());
            }
            let ClickhouseBatchEncoding::ArrowStream(_) = batch_encoding;
        }

        // Resolve auth choice (pure validation)
        let auth = config.auth.choose_one(&config.endpoint.auth)?;

        // Compute batch settings (pure validation)
        let batch_settings = config.batch.into_batcher_settings()?;

        // Confine templates (pure validation)
        let confined_table =
            config
                .table
                .clone()
                .confine(&config.confinement, ClickhouseConfig::NAME, "table")?;
        let confined_database =
            database
                .clone()
                .confine(&config.confinement, ClickhouseConfig::NAME, "database")?;

        Ok(Self {
            config: config.clone(),
            database,
            auth,
            batch_settings,
            confined_table,
            confined_database,
        })
    }
}

#[async_trait::async_trait]
impl crate::config::PreparedSink for ValidatedClickhouse {
    fn get_type_name(&self) -> &'static str {
        "clickhouse"
    }

    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let endpoint = self.config.endpoint.with_default_parts().uri;
        let tls_settings = TlsSettings::from_options(self.config.tls.as_ref())?;
        let client = HttpClient::new(tls_settings, &cx.proxy)?;

        let clickhouse_service_request_builder = ClickhouseServiceRequestBuilder {
            auth: self.auth.clone(),
            endpoint: endpoint.clone(),
            skip_unknown_fields: self.config.skip_unknown_fields,
            date_time_best_effort: self.config.date_time_best_effort,
            insert_random_shard: self.config.insert_random_shard,
            compression: self.config.compression,
            query_settings: self.config.query_settings,
        };

        let service: HttpService<ClickhouseServiceRequestBuilder, PartitionKey> =
            HttpService::new(client.clone(), clickhouse_service_request_builder);

        let request_limits = self.config.request.into_settings();

        let service = ServiceBuilder::new()
            .settings(request_limits, ClickhouseRetryLogic::default())
            .service(service);

        // Resolve the encoding strategy (format + encoder) based on configuration.
        // This happens here in build because Arrow schema fetching requires network access.
        let (format, encoder_kind) = self
            .config
            .resolve_strategy(&client, &endpoint, &self.database, self.auth.as_ref())
            .await?;

        let request_builder = ClickhouseRequestBuilder {
            compression: self.config.compression,
            encoder: (self.config.encoding.clone(), encoder_kind),
        };

        // Use pre-computed batch settings and confined templates
        let sink = ClickhouseSink::new(
            self.batch_settings,
            service,
            self.confined_database.clone(),
            self.confined_table.clone(),
            format,
            request_builder,
        );

        let healthcheck = Box::pin(healthcheck(client, endpoint, self.auth.clone()));
        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn confinement_config(&self) -> Option<&ConfinementConfig> {
        Some(&self.config.confinement)
    }

    fn acknowledgements(&self) -> AcknowledgementsConfig {
        self.config.acknowledgements
    }
}

impl ValidateSink for ClickhouseConfig {
    type Validated = ValidatedClickhouse;

    fn prepare(&self) -> crate::Result<Self::Validated> {
        ValidatedClickhouse::from_config(self)
    }
}

async fn healthcheck(client: HttpClient, endpoint: Uri, auth: Option<Auth>) -> crate::Result<()> {
    let uri = get_healthcheck_uri(&endpoint);
    let mut request = Request::get(uri).body(Body::empty()).unwrap();

    if let Some(auth) = auth {
        auth.apply(&mut request);
    }

    let response = client.send(request).await?;

    match response.status() {
        StatusCode::OK => Ok(()),
        status => Err(HealthcheckError::UnexpectedStatus { status }.into()),
    }
}

fn get_healthcheck_uri(endpoint: &Uri) -> String {
    let mut uri = endpoint.to_string();
    if !uri.ends_with('/') {
        uri.push('/');
    }
    uri.push_str("?query=SELECT%201");
    uri
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinks::clickhouse::config::Format;

    #[test]
    fn prepares_valid_config() {
        let config = ClickhouseConfig {
            endpoint: "http://localhost:8123".parse::<http::Uri>().unwrap().into(),
            table: "test_table".try_into().unwrap(),
            database: Some("test_db".try_into().unwrap()),
            format: Format::JsonEachRow,
            ..Default::default()
        };

        let validated = config.prepare().expect("preparation should succeed");
        assert_eq!(validated.database.get_ref(), "test_db");
        assert!(validated.auth.is_none()); // Default has no auth
        // Verify the confined templates retained the validated values.
        assert_eq!(validated.confined_table.to_string(), "test_table");
        assert_eq!(validated.confined_database.to_string(), "test_db");
    }

    #[test]
    fn rejects_incompatible_batch_encoding() {
        let config = ClickhouseConfig {
            endpoint: "http://localhost:8123".parse::<http::Uri>().unwrap().into(),
            table: "test_table".try_into().unwrap(),
            format: Format::JsonEachRow, // Incompatible with batch_encoding
            batch_encoding: Some(ClickhouseBatchEncoding::ArrowStream(
                vector_lib::codecs::encoding::ArrowStreamSerializerConfig::default(),
            )),
            ..Default::default()
        };

        let result = config.prepare();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("'batch_encoding' is only compatible"));
    }

    #[test]
    fn rejects_unconfined_template() {
        let config = ClickhouseConfig {
            endpoint: "http://localhost:8123".parse::<http::Uri>().unwrap().into(),
            table: "{{ table }}".try_into().unwrap(), // No static prefix
            format: Format::JsonEachRow,
            ..Default::default()
        };

        let result = config.prepare();
        assert!(
            result.is_err(),
            "Expected preparation to reject unconfined template"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("confinement") || err.contains("prefix"),
            "Error should mention confinement/prefix: {}",
            err
        );
    }
}
