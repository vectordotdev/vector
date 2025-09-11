//! Configuration for the `Doris` sink.

use super::{progress::ProgressReporter, sink::DorisSink};

use crate::{
    http::{Auth, HttpClient},
    sinks::{
        doris::{
            client::DorisSinkClient, common::DorisCommon, health::DorisHealthLogic,
            retry::DorisRetryLogic, service::DorisService,
        },
        prelude::*,
        util::{RealtimeSizeBasedDefaultBatchSettings, service::HealthConfig},
    },
};
use futures;
use futures_util::TryFutureExt;
use std::{collections::HashMap, sync::Arc};
use vector_lib::codecs::JsonSerializerConfig;

/// Configuration for the `doris` sink.
#[configurable_component(sink("doris", "Deliver log data to an Apache Doris database."))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct DorisConfig {
    /// A list of Doris endpoints to send logs to.
    ///
    /// The endpoint must contain an HTTP scheme, and may specify a
    /// hostname or IP address and port.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "http://127.0.0.1:8030"))]
    pub endpoints: Vec<String>,

    /// The database that contains the table data will be inserted into.
    #[configurable(metadata(docs::examples = "mydatabase"))]
    pub database: Template,

    /// The table data is inserted into.
    #[configurable(metadata(docs::examples = "mytable"))]
    pub table: Template,

    /// The prefix for Stream Load label.
    /// The final label will be in format: `{label_prefix}_{database}_{table}_{timestamp}_{uuid}`.
    #[configurable(metadata(docs::examples = "vector"))]
    #[serde(default = "default_label_prefix")]
    pub label_prefix: String,

    /// The line delimiter for batch data.
    #[configurable(metadata(docs::examples = "\\n"))]
    #[serde(default = "default_line_delimiter")]
    pub line_delimiter: String,

    /// Enable request logging.
    #[serde(default = "default_log_request")]
    pub log_request: bool,

    /// Progress reporting interval in seconds.
    /// Set to 0 to disable progress reporting.
    #[serde(default = "default_log_progress_interval")]
    pub log_progress_interval: u64,

    /// Custom HTTP headers to add to the request.
    #[serde(default)]
    #[configurable(metadata(docs::additional_props_description = "An HTTP header value."))]
    pub headers: HashMap<String, String>,

    /// The codec configuration. This configures how events are encoded before being sent to Doris.
    #[serde(default)]
    pub codec: JsonSerializerConfig,

    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    pub encoding: Transformer,

    /// Compression algorithm to use for HTTP requests.
    #[serde(default)]
    pub compression: Compression,

    /// Number of retries that will be attempted before give up.
    #[serde(default = "default_max_retries")]
    pub max_retries: isize,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    /// Controls the buffer size for requests sent to Doris endpoints.
    ///
    /// This sets the maximum number of stream load requests that can be queued for sending to
    /// Doris endpoints before backpressure is applied. A value of 1 ensures requests are sent
    /// sequentially to the endpoint.
    #[configurable(metadata(docs::examples = 1))]
    #[configurable(metadata(docs::human_name = "Request Buffer Size"))]
    #[serde(default = "default_buffer_bound")]
    pub buffer_bound: usize,

    #[configurable(derived)]
    pub auth: Option<Auth>,

    #[serde(default)]
    #[configurable(derived)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    /// Options for determining the health of Doris endpoints.
    #[serde(default)]
    #[configurable(derived)]
    #[serde(rename = "distribution")]
    pub endpoint_health: Option<HealthConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

fn default_label_prefix() -> String {
    "vector".to_string()
}

fn default_line_delimiter() -> String {
    "\n".to_string()
}

fn default_log_request() -> bool {
    true
}

fn default_log_progress_interval() -> u64 {
    10
}

fn default_max_retries() -> isize {
    -1
}

fn default_buffer_bound() -> usize {
    1
}

impl_generate_config_from_default!(DorisConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "doris")]
impl SinkConfig for DorisConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let endpoints = self.endpoints.clone();

        if endpoints.is_empty() {
            return Err("No endpoints configured.'.".into());
        }
        let commons = DorisCommon::parse_many(self).await?;
        let common = commons[0].clone();

        let client = HttpClient::new(common.tls_settings.clone(), &cx.proxy)?;

        // Create and start the progress reporter
        let reporter = ProgressReporter::new(self.log_progress_interval);
        let reporter_clone = reporter.clone();
        // Create a new noop shutdown signal - it will be automatically closed when the Vector process shuts down
        let shutdown = vector_lib::shutdown::ShutdownSignal::noop();
        tokio::spawn(async move {
            reporter_clone.report(Some(shutdown)).await;
        });

        // Setup retry logic using the configured request settings
        let request_settings = self.request.into_settings();

        let health_config = self.endpoint_health.clone().unwrap_or_default();

        // Wrap reporter in Arc for sharing
        let reporter_arc = Arc::new(reporter);

        let services_futures = commons
            .iter()
            .cloned()
            .map(|common| {
                let client_clone = client.clone();
                let reporter_arc_clone = Arc::clone(&reporter_arc);
                let compression = self.compression.clone();
                let label_prefix = self.label_prefix.clone();
                let headers = self.headers.clone();
                let log_request = self.log_request;

                async move {
                    let endpoint = common.base_url.clone();

                    let doris_client = DorisSinkClient::new(
                        client_clone,
                        common.base_url.clone(),
                        common.auth.clone(),
                        compression,
                        label_prefix,
                        headers,
                    )
                    .await;

                    let doris_client_safe = doris_client.into_thread_safe();

                    let service =
                        DorisService::new(doris_client_safe, log_request, reporter_arc_clone);

                    Ok::<_, crate::Error>((endpoint, service))
                }
            })
            .collect::<Vec<_>>();

        // Wait for all futures to complete
        let services_results = futures::future::join_all(services_futures).await;

        // Filter out successful results
        let services = services_results
            .into_iter()
            .filter_map(Result::ok)
            .collect::<Vec<_>>();

        let service = request_settings.distributed_service(
            DorisRetryLogic {},
            services,
            health_config,
            DorisHealthLogic,
            self.buffer_bound,
        );

        // Create DorisSink with the configured service
        let sink = DorisSink::new(service, self, &common)?;

        let sink = VectorSink::from_event_streamsink(sink);

        // Create a shared client instance to avoid repeated creation
        let healthcheck_doris_client = {
            let doris_client = DorisSinkClient::new(
                client.clone(),
                common.base_url.clone(),
                common.auth.clone(),
                self.compression.clone(),
                self.label_prefix.clone(),
                self.headers.clone(),
            )
            .await;
            doris_client.into_thread_safe()
        };

        // Use the previously saved client for health check, no need to create a new instance
        let healthcheck = futures::future::select_ok(commons.into_iter().map(move |common| {
            let client = healthcheck_doris_client.clone();
            async move { common.healthcheck(client).await }.boxed()
        }))
        .map_ok(|((), _)| ())
        .boxed();

        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DorisConfig>();
    }

    #[test]
    fn test_default_values() {
        assert_eq!(default_label_prefix(), "vector");
        assert_eq!(default_line_delimiter(), "\n");
        assert_eq!(default_log_request(), true);
        assert_eq!(default_log_progress_interval(), 10);
        assert_eq!(default_max_retries(), -1);
    }

    #[test]
    fn parse_config_with_defaults() {
        let config: DorisConfig = toml::from_str(
            r#"
            endpoints = ["http://localhost:8030"]
            database = "test_db"
            table = "test_table"
            "#,
        )
        .unwrap();

        assert_eq!(config.endpoints, vec!["http://localhost:8030"]);
        assert_eq!(config.database.to_string(), "test_db");
        assert_eq!(config.table.to_string(), "test_table");
        assert_eq!(config.label_prefix, "vector");
        assert_eq!(config.line_delimiter, "\n");
        assert!(config.log_request);
        assert_eq!(config.log_progress_interval, 10);
        assert_eq!(config.max_retries, -1);
    }

    #[test]
    fn parse_config_with_custom_values() {
        let config: DorisConfig = toml::from_str(
            r#"
            endpoints = ["http://doris1:8030", "http://doris2:8030"]
            database = "custom_db"
            table = "custom_table"
            label_prefix = "custom_prefix"
            line_delimiter = "\r\n"
            log_request = false
            log_progress_interval = 30
            max_retries = 5
            "#,
        )
        .unwrap();

        assert_eq!(
            config.endpoints,
            vec!["http://doris1:8030", "http://doris2:8030"]
        );
        assert_eq!(config.database.to_string(), "custom_db");
        assert_eq!(config.table.to_string(), "custom_table");
        assert_eq!(config.label_prefix, "custom_prefix");
        assert_eq!(config.line_delimiter, "\r\n");
        assert!(!config.log_request);
        assert_eq!(config.log_progress_interval, 30);
        assert_eq!(config.max_retries, 5);
    }

    #[test]
    fn parse_config_with_auth() {
        let config: DorisConfig = toml::from_str(
            r#"
            endpoints = ["http://localhost:8030"]
            database = "test_db"
            table = "test_table"
            auth.strategy = "basic"
            auth.user = "admin"
            auth.password = "password"
            "#,
        )
        .unwrap();

        assert!(config.auth.is_some());
        if let Some(Auth::Basic { user, password }) = &config.auth {
            assert_eq!(user, "admin");
            assert_eq!(password.inner(), "password");
        } else {
            panic!("Expected Basic auth");
        }
    }

    #[test]
    fn parse_config_with_custom_headers() {
        let config: DorisConfig = toml::from_str(
            r#"
            endpoints = ["http://localhost:8030"]
            database = "test_db"
            table = "test_table"
            [headers]
            "X-Custom-Header" = "custom_value"
            "Content-Type" = "application/json"
            "#,
        )
        .unwrap();

        assert_eq!(config.headers.len(), 2);
        assert_eq!(
            config.headers.get("X-Custom-Header").unwrap(),
            "custom_value"
        );
        assert_eq!(
            config.headers.get("Content-Type").unwrap(),
            "application/json"
        );
    }

    #[test]
    fn parse_distribution() {
        toml::from_str::<DorisConfig>(
            r#"
            endpoints = ["", ""]
            database = "test_db"
            table = "test_table"
            distribution.retry_initial_backoff_secs = 10
        "#,
        )
        .unwrap();
    }
}
