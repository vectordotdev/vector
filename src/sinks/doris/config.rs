//! Configuration for the `Doris` sink.

use super::sink::DorisSink;

use crate::{
    codecs::EncodingConfigWithFraming,
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
use std::collections::HashMap;
use std::sync::Arc;

/// Configuration for the `doris` sink.
#[configurable_component(sink("doris", "Deliver log data to an Apache Doris database."))]
#[derive(Clone, Debug)]
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

    /// Enable request logging.
    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    pub log_request: bool,

    /// Custom HTTP headers to add to the request.
    ///
    /// These headers can be used to set Doris-specific Stream Load parameters:
    /// - `format`: Data format (json, csv.)
    /// - `read_json_by_line`: Whether to read JSON line by line
    /// - `strip_outer_array`: Whether to strip outer array brackets
    /// - Column mappings and transformations
    ///
    /// See [Doris Stream Load documentation](https://doris.apache.org/docs/data-operate/import/import-way/stream-load-manual)
    /// for all available parameters.
    #[serde(default)]
    #[configurable(metadata(docs::additional_props_description = "An HTTP header value."))]
    pub headers: HashMap<String, String>,

    #[serde(flatten)]
    pub encoding: EncodingConfigWithFraming,

    /// Compression algorithm to use for HTTP requests.
    #[serde(default)]
    pub compression: Compression,

    /// Number of retries attempted before failing.
    #[serde(default = "default_max_retries")]
    pub max_retries: isize,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

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

const fn default_max_retries() -> isize {
    -1
}

impl Default for DorisConfig {
    fn default() -> Self {
        Self {
            endpoints: Vec::new(),
            database: Template::try_from("").unwrap(),
            table: Template::try_from("").unwrap(),
            label_prefix: default_label_prefix(),
            log_request: false,
            headers: HashMap::new(),
            encoding: (
                Some(vector_lib::codecs::encoding::FramingConfig::NewlineDelimited),
                vector_lib::codecs::JsonSerializerConfig::default(),
            )
                .into(),
            compression: Compression::default(),
            max_retries: default_max_retries(),
            batch: BatchConfig::default(),
            auth: None,
            request: TowerRequestConfig::default(),
            tls: None,
            endpoint_health: None,
            acknowledgements: AcknowledgementsConfig::default(),
        }
    }
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

        // Setup retry logic using the configured request settings
        let request_settings = self.request.into_settings();

        let health_config = self.endpoint_health.clone().unwrap_or_default();

        let services_futures = commons
            .iter()
            .cloned()
            .map(|common| {
                let client_clone = client.clone();
                let compression = self.compression;
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

                    let service = DorisService::new(doris_client_safe, log_request);

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
            1, // Buffer bound is hardcoded to 1 for sinks
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
                self.compression,
                self.label_prefix.clone(),
                self.headers.clone(),
            )
            .await;
            doris_client.into_thread_safe()
        };

        // Use the previously saved client for health check, no need to create a new instance
        let healthcheck = futures::future::select_ok(commons.into_iter().map(move |common| {
            let client = Arc::clone(&healthcheck_doris_client);
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
        assert_eq!(default_max_retries(), -1);
    }
}
