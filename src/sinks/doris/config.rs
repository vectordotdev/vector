//! Configuration for the `Doris` sink.

use super::sink::DorisSink;

use crate::{
    codecs::EncodingConfigWithFraming,
    config::ValidatedSink,
    http::{Auth, HttpClient, MaybeAuth},
    sinks::{
        doris::{
            client::DorisSinkClient, common::DorisCommon, health::DorisHealthLogic,
            retry::DorisRetryLogic, service::DorisService,
        },
        prelude::*,
        util::{
            RealtimeSizeBasedDefaultBatchSettings, TowerRequestSettings, UriSerde,
            service::HealthConfig,
        },
    },
    template::ConfinementConfig,
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
    #[configurable(metadata(docs::examples = "http://127.0.0.1:8030"))]
    #[configurable(metadata(docs::required = true))]
    pub endpoints: Vec<UriSerde>,

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

    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    pub auth: Option<Auth>,

    #[serde(default)]
    pub request: TowerRequestConfig,

    pub tls: Option<TlsConfig>,

    /// Options for determining the health of Doris endpoints.
    #[serde(default)]
    #[serde(rename = "distribution")]
    pub endpoint_health: Option<HealthConfig>,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    #[serde(flatten)]
    pub confinement: ConfinementConfig,
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
            confinement: ConfinementConfig::default(),
        }
    }
}

impl_generate_config_from_default!(DorisConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "doris")]
impl SinkConfig for DorisConfig {
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

#[derive(Clone, Debug)]
pub struct ValidatedDoris {
    request_settings: TowerRequestSettings,
    health_config: HealthConfig,
    batch_settings: BatcherSettings,
    database: ConfinedTemplate,
    table: ConfinedTemplate,
}

#[async_trait::async_trait]
impl ValidatedSink for DorisConfig {
    type Validated = ValidatedDoris;

    fn validate(&self) -> crate::Result<ValidatedDoris> {
        if self.endpoints.is_empty() {
            return Err("No endpoints configured.'.".into());
        }
        // Pure endpoint checks only — `DorisCommon`/TLS loading reads
        // certificate files from disk, so it is deferred to `build` to keep
        // `vector validate --no-environment` filesystem-free.
        for endpoint in &self.endpoints {
            if !matches!(endpoint.uri.scheme_str(), Some("http" | "https")) {
                return Err(format!(
                    "Invalid scheme: {}, endpoint must use http or https",
                    endpoint.uri
                )
                .into());
            }
            if endpoint.uri.host().is_none() {
                return Err(
                    format!("Invalid host: {}, host must include hostname", endpoint.uri).into(),
                );
            }
            self.auth.choose_one(&endpoint.auth)?;
        }
        let request_settings = self.request.into_settings();
        let health_config = self.endpoint_health.clone().unwrap_or_default();
        let batch_settings = self.batch.into_batcher_settings()?;
        let database = self
            .database
            .clone()
            .confine(&self.confinement, Self::NAME, "database")?;
        let table = self
            .table
            .clone()
            .confine(&self.confinement, Self::NAME, "table")?;

        Ok(ValidatedDoris {
            request_settings,
            health_config,
            batch_settings,
            database,
            table,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedDoris,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let ValidatedDoris {
            request_settings,
            health_config,
            batch_settings,
            database,
            table,
        } = validated.clone();
        // `DorisCommon` parsing performs environment-dependent work (TLS
        // certificate loading from disk), so it happens here at build time
        // rather than during `validate`.
        let commons = DorisCommon::parse_many(self)?;
        let common = &commons[0];

        let client = HttpClient::new(common.tls_settings.clone(), &cx.proxy)?;

        let services_futures = commons
            .iter()
            .map(|common| {
                let client_clone = client.clone();
                let compression = self.compression;
                let label_prefix = self.label_prefix.clone();
                let headers = self.headers.clone();
                let log_request = self.log_request;
                let base_url = common.base_url.clone();
                let auth = common.auth.clone();

                async move {
                    let endpoint = base_url.to_string();

                    let doris_client = DorisSinkClient::new(
                        client_clone,
                        base_url,
                        auth,
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

        // Create DorisSink with the pre-validated settings
        let sink = DorisSink::new(
            service,
            batch_settings,
            database,
            table,
            common.request_builder.clone(),
        );

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
        let healthcheck = futures::future::select_ok(commons.iter().cloned().map(move |common| {
            let client = Arc::clone(&healthcheck_doris_client);
            async move { common.healthcheck(client).await }.boxed()
        }))
        .map_ok(|((), _)| ())
        .boxed();
        Ok((sink, healthcheck))
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
    fn validate_produces_usable_values() {
        use crate::config::ValidatedSink;
        let config = DorisConfig {
            endpoints: vec![
                "http://127.0.0.1:8030"
                    .parse::<http::Uri>()
                    .unwrap()
                    .try_into()
                    .unwrap(),
            ],
            database: Template::try_from("mydatabase").unwrap(),
            table: Template::try_from("mytable").unwrap(),
            ..Default::default()
        };
        let validated = config.validate().expect("validation should succeed");
        assert_eq!(validated.database.to_string(), "mydatabase");
        assert_eq!(validated.table.to_string(), "mytable");
    }

    #[test]
    fn validate_rejects_auth_conflict_with_endpoint_credentials() {
        use crate::config::ValidatedSink;
        let config = DorisConfig {
            endpoints: vec![
                "http://user:pass@127.0.0.1:8030"
                    .parse::<http::Uri>()
                    .unwrap()
                    .try_into()
                    .unwrap(),
            ],
            database: Template::try_from("mydatabase").unwrap(),
            table: Template::try_from("mytable").unwrap(),
            auth: Some(crate::http::Auth::Basic {
                user: "config_user".to_string(),
                password: vector_common::sensitive_string::SensitiveString::from(
                    "config_pass".to_string(),
                ),
            }),
            ..Default::default()
        };
        assert!(
            config.validate().is_err(),
            "top-level auth combined with endpoint-embedded credentials must be rejected"
        );
    }

    #[test]
    fn validate_rejects_non_http_scheme() {
        use crate::config::ValidatedSink;
        let config = DorisConfig {
            endpoints: vec![
                "ftp://doris.example.com:8030"
                    .parse::<http::Uri>()
                    .unwrap()
                    .try_into()
                    .unwrap(),
            ],
            database: Template::try_from("mydatabase").unwrap(),
            table: Template::try_from("mytable").unwrap(),
            ..Default::default()
        };
        assert!(
            config.validate().is_err(),
            "a non-http endpoint scheme must be rejected during validation"
        );
    }

    #[test]
    fn test_default_values() {
        assert_eq!(default_label_prefix(), "vector");
        assert_eq!(default_max_retries(), -1);
    }

    #[test]
    fn confinement_rejects_unconfined_database_template() {
        let template = Template::try_from("{{ tenant }}").unwrap();
        let config = ConfinementConfig::default();
        let result = template.confine(&config, "doris", "database");
        assert!(
            result.is_err(),
            "bare template with no literal prefix must be rejected"
        );
    }

    #[test]
    fn confinement_opt_out_allows_unconfined_database_template() {
        let template = Template::try_from("{{ tenant }}").unwrap();
        let config = ConfinementConfig {
            dangerously_allow_unconfined_template_resolution: true,
        };
        let result = template.confine(&config, "doris", "database");
        assert!(result.is_ok(), "opt-out must allow bare template");
    }

    #[test]
    fn confinement_blocks_dotdot_escape_at_render() {
        use crate::event::LogEvent;
        use vrl::event_path;
        let template = Template::try_from("mydb_{{ tenant }}").unwrap();
        let config = ConfinementConfig::default();
        let confined = template.confine(&config, "doris", "database").unwrap();
        let mut event = LogEvent::default();
        event.insert(event_path!("tenant"), "/../evil");
        let result = confined.render_string(&crate::event::Event::Log(event));
        assert!(
            result.is_err(),
            "dotdot escape in rendered value must be rejected by prefix check"
        );
    }
}
