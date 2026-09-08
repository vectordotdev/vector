use std::sync::Arc;

use indoc::indoc;
use tower::ServiceBuilder;
use vector_lib::{
    config::proxy::ProxyConfig, configurable::configurable_component, schema::meaning,
};
use vrl::value::Kind;

use hyper::{Body, client::connect::Connect};

use super::{service::LogApiRetry, sink::LogSinkBuilder};
use crate::config::ValidatedSink;
use crate::{
    common::datadog,
    http::HttpClient,
    schema,
    sinks::{
        datadog::{DatadogCommonConfig, LocalDatadogCommonConfig, logs::service::LogApiService},
        prelude::*,
        util::{
            HttpEndpoint,
            http::{RequestConfig, validate_headers},
        },
    },
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};

// The Datadog API has a hard limit of 5MB for uncompressed payloads. Above this
// threshold the API will toss results. We previously serialized Events as they
// came in -- a very CPU intensive process -- and to avoid that we only batch up
// to 750KB below the max and then build our payloads. This does mean that in
// some situations we'll kick out over-large payloads -- for instance, a string
// of escaped double-quotes -- but we believe this should be very rare in
// practice.
pub const MAX_PAYLOAD_BYTES: usize = 5_000_000;
pub const BATCH_GOAL_BYTES: usize = 4_250_000;
pub const BATCH_MAX_EVENTS: usize = 1_000;
pub const BATCH_DEFAULT_TIMEOUT_SECS: f64 = 5.0;

#[derive(Clone, Copy, Debug, Default)]
pub struct DatadogLogsDefaultBatchSettings;

impl SinkBatchSettings for DatadogLogsDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(BATCH_MAX_EVENTS);
    const MAX_BYTES: Option<usize> = Some(BATCH_GOAL_BYTES);
    const TIMEOUT_SECS: f64 = BATCH_DEFAULT_TIMEOUT_SECS;
}

/// Configuration for the `datadog_logs` sink.
#[configurable_component(sink("datadog_logs", "Publish log events to Datadog."))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct DatadogLogsConfig {
    #[serde(flatten)]
    pub local_dd_common: LocalDatadogCommonConfig,

    #[derivative(Default(value = "default_compression()"))]
    #[serde(default = "default_compression")]
    pub compression: Option<Compression>,

    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    pub encoding: Transformer,

    #[serde(default)]
    pub batch: BatchConfig<DatadogLogsDefaultBatchSettings>,

    #[serde(default)]
    pub request: RequestConfig,

    /// When enabled this sink will normalize events to conform to the Datadog Agent standard. This
    /// also sends requests to the logs backend with the `DD-PROTOCOL: agent-json` header. This bool
    /// will be overridden as `true` if this header has already been set in the request.headers
    /// configuration setting.
    #[serde(default)]
    pub conforms_as_agent: bool,
}

const fn default_compression() -> Option<Compression> {
    Some(Compression::zstd_default())
}

impl GenerateConfig for DatadogLogsConfig {
    fn generate_config() -> serde_json::Value {
        serde_yaml::from_str(indoc! {r#"
            default_api_key: ${DATADOG_API_KEY_ENV_VAR}
        "#})
        .unwrap()
    }
}

impl DatadogLogsConfig {
    // TODO: We should probably hoist this type of base URI generation so that all DD sinks can
    // utilize it, since it all follows the same pattern.
    /// Resolve the logs API endpoint from the given endpoint/site.
    fn logs_endpoint(endpoint: Option<&str>, site: &str) -> crate::Result<HttpEndpoint> {
        let base_url = endpoint.map_or_else(
            || format!("https://http-intake.logs.{site}"),
            |endpoint| endpoint.to_string(),
        );

        Ok(HttpEndpoint::parse(&base_url)?.append_path("/api/v2/logs")?)
    }

    fn get_uri(&self, dd_common: &DatadogCommonConfig) -> crate::Result<HttpEndpoint> {
        Self::logs_endpoint(dd_common.endpoint.as_deref(), &dd_common.site)
    }

    pub fn build_processor<C>(
        &self,
        dd_common: &DatadogCommonConfig,
        client: HttpClient<Body, C>,
        dd_evp_origin: String,
        batch: BatcherSettings,
    ) -> crate::Result<VectorSink>
    where
        C: Connect + Clone + Send + Sync + 'static,
    {
        let default_api_key: Arc<str> = Arc::from(dd_common.default_api_key.inner());
        let request_limits = self.request.tower.into_settings();

        let headers = {
            let mut request_headers = self.request.headers.clone();
            if self.conforms_as_agent {
                request_headers.insert(String::from("DD-PROTOCOL"), String::from("agent-json"));
            }
            request_headers
        };

        // conforms_as_agent is true if either the user supplied configuration parameter is enabled
        // or the DD-PROTOCOL: agent-json header had already been manually set
        let conforms_as_agent = if let Some(value) = headers.get("DD-PROTOCOL") {
            value == "agent-json"
        } else {
            false
        };

        let endpoint = self.get_uri(dd_common)?;
        let protocol = endpoint.protocol().to_string();

        let service = ServiceBuilder::new()
            .settings(request_limits, LogApiRetry)
            .service(LogApiService::new(
                client,
                endpoint.into_uri(),
                headers,
                dd_evp_origin,
            )?);

        let encoding = self.encoding.clone();

        let sink = LogSinkBuilder::new(
            encoding,
            service,
            default_api_key,
            batch,
            protocol,
            conforms_as_agent,
        )
        .compression(self.compression.or_else(default_compression).unwrap())
        .build();

        Ok(VectorSink::from_event_streamsink(sink))
    }

    pub fn create_client(&self, proxy: &ProxyConfig) -> crate::Result<HttpClient> {
        let default_tls_config;

        let tls_settings = MaybeTlsSettings::from_config(
            Some(match self.local_dd_common.tls.as_ref() {
                Some(config) => config,
                None => {
                    default_tls_config = TlsEnableableConfig::enabled();
                    &default_tls_config
                }
            }),
            false,
        )?;
        Ok(HttpClient::new(tls_settings, proxy)?)
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "datadog_logs")]
impl SinkConfig for DatadogLogsConfig {
    fn input(&self) -> Input {
        let requirement = schema::Requirement::empty()
            .optional_meaning(meaning::MESSAGE, Kind::bytes())
            .optional_meaning(meaning::TIMESTAMP, Kind::timestamp())
            .optional_meaning(meaning::HOST, Kind::bytes())
            .optional_meaning(meaning::SOURCE, Kind::bytes())
            .optional_meaning(meaning::SEVERITY, Kind::bytes())
            .optional_meaning(meaning::SERVICE, Kind::bytes())
            .optional_meaning(meaning::TRACE_ID, Kind::bytes());

        Input::log().with_schema_requirement(requirement)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.local_dd_common.acknowledgements
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedLogs {
    batch: BatcherSettings,
}

#[async_trait::async_trait]
impl ValidatedSink for DatadogLogsConfig {
    type Validated = ValidatedLogs;

    fn validate(&self) -> crate::Result<ValidatedLogs> {
        let site = self
            .local_dd_common
            .site
            .clone()
            .unwrap_or_else(|| datadog::DD_US_SITE.to_owned());
        Self::logs_endpoint(self.local_dd_common.endpoint.as_deref(), &site)?;

        let request_headers = {
            let mut request_headers = self.request.headers.clone();
            if self.conforms_as_agent {
                request_headers.insert(String::from("DD-PROTOCOL"), String::from("agent-json"));
            }
            request_headers
        };
        validate_headers(&request_headers)?;

        let batch = self
            .batch
            .validate()?
            .limit_max_bytes(BATCH_GOAL_BYTES)?
            .limit_max_events(BATCH_MAX_EVENTS)?
            .into_batcher_settings()?;

        Ok(ValidatedLogs { batch })
    }

    async fn build(
        &self,
        validated: &ValidatedLogs,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = self.create_client(&cx.proxy)?;
        let global = cx.extra_context.get_or_default::<datadog::Options>();
        let dd_common = self.local_dd_common.with_globals(global)?;

        let healthcheck = dd_common.build_healthcheck(client.clone())?;

        let sink = self.build_processor(&dd_common, client, cx.app_name_slug, validated.batch)?;

        Ok((sink, healthcheck))
    }
}

#[cfg(test)]
mod test {
    use vector_lib::{
        codecs::{JsonSerializerConfig, MetricTagValues, encoding::format::JsonSerializerOptions},
        config::LogNamespace,
        sensitive_string::SensitiveString,
    };

    use super::*;
    use crate::{
        assert_downcast_matches, codecs::EncodingConfigWithFraming,
        components::validation::prelude::*, config::ValidatedSink,
        sinks::util::http::HeaderValidationError,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DatadogLogsConfig>();
    }

    #[test]
    fn validate_produces_usable_batch_settings() {
        let config = DatadogLogsConfig::default();
        let validated = config.validate().expect("validation should succeed");
        assert_eq!(validated.batch.size_limit, BATCH_GOAL_BYTES);
        assert_eq!(validated.batch.item_limit, BATCH_MAX_EVENTS);
    }

    #[test]
    fn validate_rejects_malformed_endpoint() {
        let config = DatadogLogsConfig {
            local_dd_common: LocalDatadogCommonConfig::new(
                Some("not a uri".to_string()),
                None,
                None,
            ),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_defaults_missing_scheme_to_https() {
        let config = DatadogLogsConfig {
            local_dd_common: LocalDatadogCommonConfig::new(
                Some("localhost:8080".to_string()),
                None,
                None,
            ),
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn validate_accepts_valid_headers() {
        let config = indoc::indoc! {r#"
            default_api_key: "test_key"
            request:
              headers:
                Auth: "token:thing_and-stuff"
                X-Custom-Nonsense: "_%_{}_-_&_._`_|_~_!_#_&_$_"
        "#};
        let config: DatadogLogsConfig = serde_yaml::from_str(config).unwrap();

        assert!(config.validate().is_ok());
    }

    #[test]
    fn validate_catches_bad_header_names() {
        let config = indoc::indoc! {r#"
            default_api_key: "test_key"
            request:
              headers:
                "\x01": "bad"
        "#};
        let config: DatadogLogsConfig = serde_yaml::from_str(config).unwrap();

        assert_downcast_matches!(
            config.validate().unwrap_err(),
            HeaderValidationError,
            HeaderValidationError::InvalidHeaderName { .. }
        );
    }

    #[test]
    fn validate_catches_bad_header_values() {
        let config = indoc::indoc! {r#"
            default_api_key: "test_key"
            request:
              headers:
                "X-Custom-Nonsense": "a\nb"
        "#};
        let config: DatadogLogsConfig = serde_yaml::from_str(config).unwrap();

        assert_downcast_matches!(
            config.validate().unwrap_err(),
            HeaderValidationError,
            HeaderValidationError::InvalidHeaderValue { .. }
        );
    }

    #[test]
    fn get_uri_defaults_missing_scheme_to_https() {
        let config = DatadogLogsConfig::default();
        let custom = DatadogCommonConfig {
            endpoint: Some("localhost:8080".to_string()),
            site: "datadoghq.com".to_string(),
            default_api_key: SensitiveString::from("key".to_string()),
            acknowledgements: Default::default(),
        };
        assert_eq!(
            config.get_uri(&custom).unwrap().to_string(),
            "https://localhost:8080/api/v2/logs"
        );
        // The default site-based endpoint keeps its scheme.
        let default = DatadogCommonConfig {
            endpoint: None,
            site: "datadoghq.com".to_string(),
            default_api_key: SensitiveString::from("key".to_string()),
            acknowledgements: Default::default(),
        };
        assert_eq!(
            config.get_uri(&default).unwrap().to_string(),
            "https://http-intake.logs.datadoghq.com/api/v2/logs"
        );
    }

    #[test]
    fn compression_config_field() {
        // Verify the default compression function returns zstd
        assert_eq!(default_compression(), Some(Compression::zstd_default()));

        // Test 1: Config deserialized without compression field gets zstd default
        // (due to #[serde(default = "default_compression")])
        let config_yaml = indoc! {r#"
            default_api_key: "test_key"
        "#};

        let config: DatadogLogsConfig = serde_yaml::from_str(config_yaml).unwrap();
        // The serde default applies immediately during deserialization
        assert!(matches!(config.compression, Some(Compression::Zstd(_))));

        // Test 2: When explicitly set to "none", it should be Some(Compression::None)
        let config_yaml_with_none = indoc! {r#"
            default_api_key: "test_key"
            compression: "none"
        "#};

        let config_no_compression: DatadogLogsConfig =
            serde_yaml::from_str(config_yaml_with_none).unwrap();
        assert_eq!(config_no_compression.compression, Some(Compression::None));

        // Test 3: When explicitly set to "zstd", it should be Some(Compression::Zstd)
        let config_yaml_with_zstd = indoc! {r#"
            default_api_key: "test_key"
            compression: "zstd"
        "#};

        let config_zstd: DatadogLogsConfig = serde_yaml::from_str(config_yaml_with_zstd).unwrap();
        assert!(matches!(
            config_zstd.compression,
            Some(Compression::Zstd(_))
        ));

        // Test 4: When explicitly set to "gzip", it should be Some(Compression::Gzip)
        let config_yaml_with_gzip = indoc! {r#"
            default_api_key: "test_key"
            compression: "gzip"
        "#};

        let config_gzip: DatadogLogsConfig = serde_yaml::from_str(config_yaml_with_gzip).unwrap();
        assert!(matches!(
            config_gzip.compression,
            Some(Compression::Gzip(_))
        ));
    }

    impl ValidatableComponent for DatadogLogsConfig {
        fn validation_configuration() -> ValidationConfiguration {
            let endpoint = "http://127.0.0.1:9005".to_string();
            let config = Self {
                local_dd_common: LocalDatadogCommonConfig {
                    endpoint: Some(endpoint.clone()),
                    default_api_key: Some("unused".to_string().into()),
                    ..Default::default()
                },
                // Disable compression for validation tests to ensure byte counting is accurate
                compression: Some(Compression::None),
                ..Default::default()
            };

            let encoding = EncodingConfigWithFraming::new(
                None,
                JsonSerializerConfig::new(MetricTagValues::Full, JsonSerializerOptions::default())
                    .into(),
                config.encoding.clone(),
            );

            let logs_endpoint = format!("{endpoint}/api/v2/logs");

            let external_resource = ExternalResource::new(
                ResourceDirection::Push,
                HttpResourceConfig::from_parts(
                    http::Uri::try_from(&logs_endpoint).expect("should not fail to parse URI"),
                    None,
                ),
                encoding,
            );

            ValidationConfiguration::from_sink(
                Self::NAME,
                LogNamespace::Legacy,
                vec![ComponentTestCaseConfig::from_sink(
                    config,
                    None,
                    Some(external_resource),
                )],
            )
        }
    }

    register_validatable_component!(DatadogLogsConfig);
}
