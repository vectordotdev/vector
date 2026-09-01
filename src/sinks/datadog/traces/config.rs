use std::sync::{Arc, Mutex};

use indoc::indoc;
use tokio::sync::oneshot::{Sender, channel};
use tower::ServiceBuilder;
use vector_lib::{
    config::{AcknowledgementsConfig, proxy::ProxyConfig},
    configurable::configurable_component,
    stream::BatcherSettings,
};

use super::{
    apm_stats::{Aggregator, flush_apm_stats_thread},
    service::TraceApiRetry,
};
use crate::{
    common::datadog,
    config::{GenerateConfig, Input, SinkConfig, SinkContext, ValidatedSink},
    http::HttpClient,
    sinks::{
        Healthcheck, VectorSink,
        datadog::{
            DatadogCommonConfig, LocalDatadogCommonConfig,
            traces::{
                request_builder::DatadogTracesRequestBuilder, service::TraceApiService,
                sink::TracesSink,
            },
        },
        util::{
            BatchConfig, Compression, HttpEndpoint, SinkBatchSettings, TowerRequestConfig,
            service::ServiceBuilderExt,
        },
    },
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};

// The Datadog API has a hard limit of 3.2MB for uncompressed payloads.
// Beyond this limit the payload will be ignored, enforcing a slight lower
// limit as a safety margin.
pub const BATCH_GOAL_BYTES: usize = 3_000_000;
pub const BATCH_MAX_EVENTS: usize = 1_000;
pub const BATCH_DEFAULT_TIMEOUT_SECS: f64 = 10.0;

pub const PAYLOAD_LIMIT: usize = 3_200_000;

#[derive(Clone, Copy, Debug, Default)]
pub struct DatadogTracesDefaultBatchSettings;

impl SinkBatchSettings for DatadogTracesDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(BATCH_MAX_EVENTS);
    const MAX_BYTES: Option<usize> = Some(BATCH_GOAL_BYTES);
    const TIMEOUT_SECS: f64 = BATCH_DEFAULT_TIMEOUT_SECS;
}

/// Configuration for the `datadog_traces` sink.
#[configurable_component(sink("datadog_traces", "Publish trace events to Datadog."))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct DatadogTracesConfig {
    #[serde(flatten)]
    pub local_dd_common: LocalDatadogCommonConfig,

    #[serde(default)]
    pub compression: Option<Compression>,

    #[serde(default)]
    pub batch: BatchConfig<DatadogTracesDefaultBatchSettings>,

    #[serde(default)]
    pub request: TowerRequestConfig,
}

impl GenerateConfig for DatadogTracesConfig {
    fn generate_config() -> serde_json::Value {
        serde_yaml::from_str(indoc! {r#"
            default_api_key: ${DATADOG_API_KEY_ENV_VAR}
        "#})
        .unwrap()
    }
}

/// Datadog traces API has two routes: one for traces and another one for stats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DatadogTracesEndpoint {
    Traces,
    #[allow(dead_code)] // This will be used when APM stats will be generated
    APMStats,
}

/// Store traces & APM stats endpoints actual URIs.
#[derive(Clone)]
pub struct DatadogTracesEndpointConfiguration {
    traces_endpoint: HttpEndpoint,
    stats_endpoint: HttpEndpoint,
}

impl DatadogTracesEndpointConfiguration {
    pub fn get_uri_for_endpoint(&self, endpoint: DatadogTracesEndpoint) -> HttpEndpoint {
        match endpoint {
            DatadogTracesEndpoint::Traces => self.traces_endpoint.clone(),
            DatadogTracesEndpoint::APMStats => self.stats_endpoint.clone(),
        }
    }
}

impl DatadogTracesConfig {
    fn traces_base_endpoint(endpoint: Option<&str>, site: &str) -> String {
        endpoint.map_or_else(
            || format!("https://trace.agent.{site}"),
            |endpoint| endpoint.to_string(),
        )
    }

    fn generate_traces_endpoint_configuration(
        &self,
        dd_common: &DatadogCommonConfig,
    ) -> crate::Result<DatadogTracesEndpointConfiguration> {
        let base_uri = Self::traces_base_endpoint(dd_common.endpoint.as_deref(), &dd_common.site);
        let traces_endpoint = build_uri(&base_uri, "/api/v0.2/traces")?;
        let stats_endpoint = build_uri(&base_uri, "/api/v0.2/stats")?;

        Ok(DatadogTracesEndpointConfiguration {
            traces_endpoint,
            stats_endpoint,
        })
    }

    pub fn build_sink(
        &self,
        dd_common: &DatadogCommonConfig,
        client: HttpClient,
        batcher_settings: BatcherSettings,
    ) -> crate::Result<VectorSink> {
        let default_api_key: Arc<str> = Arc::from(dd_common.default_api_key.inner());
        let request_limits = self.request.into_settings();
        let endpoints = self.generate_traces_endpoint_configuration(dd_common)?;

        let service = ServiceBuilder::new()
            .settings(request_limits, TraceApiRetry)
            .service(TraceApiService::new(client.clone()));

        // Object responsible for caching/processing APM stats from incoming trace events.
        let apm_stats_aggregator =
            Arc::new(Mutex::new(Aggregator::new(Arc::clone(&default_api_key))));

        let compression = self.compression.unwrap_or_else(Compression::gzip_default);

        let request_builder = DatadogTracesRequestBuilder::new(
            Arc::clone(&default_api_key),
            endpoints.clone(),
            compression,
            PAYLOAD_LIMIT,
            Arc::clone(&apm_stats_aggregator),
        )?;

        // shutdown= Sender that the sink signals when input stream is exhausted.
        // tripwire= Receiver that APM stats flush thread listens for exit signal on.
        let (shutdown, tripwire) = channel::<Sender<()>>();

        let sink = TracesSink::new(
            service,
            request_builder,
            batcher_settings,
            shutdown,
            endpoints.traces_endpoint.protocol().to_string(),
        );

        // Send the APM stats payloads independently of the sink framework.
        // This is necessary to comply with what the APM stats backend of Datadog expects with
        // respect to receiving stats payloads.
        crate::spawn_in_current_span(flush_apm_stats_thread(
            tripwire,
            client,
            compression,
            endpoints,
            Arc::clone(&apm_stats_aggregator),
        ));

        Ok(VectorSink::from_event_streamsink(sink))
    }

    pub fn build_client(&self, proxy: &ProxyConfig) -> crate::Result<HttpClient> {
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
#[typetag::serde(name = "datadog_traces")]
impl SinkConfig for DatadogTracesConfig {
    fn input(&self) -> Input {
        Input::trace()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.local_dd_common.acknowledgements
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedTraces {
    batcher_settings: BatcherSettings,
}

#[async_trait::async_trait]
impl ValidatedSink for DatadogTracesConfig {
    type Validated = ValidatedTraces;

    fn validate(&self) -> crate::Result<ValidatedTraces> {
        let batcher_settings = self
            .batch
            .validate()?
            .limit_max_bytes(BATCH_GOAL_BYTES)?
            .limit_max_events(BATCH_MAX_EVENTS)?
            .into_batcher_settings()?;

        let site = self
            .local_dd_common
            .site
            .clone()
            .unwrap_or_else(|| datadog::DD_US_SITE.to_owned());
        let base = Self::traces_base_endpoint(self.local_dd_common.endpoint.as_deref(), &site);
        HttpEndpoint::parse(&base)?;

        Ok(ValidatedTraces { batcher_settings })
    }

    async fn build(
        &self,
        validated: &ValidatedTraces,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = self.build_client(&cx.proxy)?;
        let global = cx.extra_context.get_or_default::<datadog::Options>();
        let dd_common = self.local_dd_common.with_globals(global)?;
        let healthcheck = dd_common.build_healthcheck(client.clone())?;
        let sink = self.build_sink(&dd_common, client, validated.batcher_settings)?;

        Ok((sink, healthcheck))
    }
}

fn build_uri(host: &str, endpoint: &str) -> crate::Result<HttpEndpoint> {
    Ok(HttpEndpoint::parse(host)?.append_path(endpoint)?)
}

#[cfg(test)]
mod test {
    use super::{BATCH_GOAL_BYTES, BATCH_MAX_EVENTS, DatadogTracesConfig};
    use crate::{config::ValidatedSink, sinks::datadog::LocalDatadogCommonConfig};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DatadogTracesConfig>();
    }

    #[test]
    fn validate_produces_usable_batch_settings() {
        let config = DatadogTracesConfig::default();
        let validated = config.validate().expect("validation should succeed");
        assert_eq!(validated.batcher_settings.size_limit, BATCH_GOAL_BYTES);
        assert_eq!(validated.batcher_settings.item_limit, BATCH_MAX_EVENTS);
    }

    #[test]
    fn validate_rejects_malformed_endpoint() {
        let config = DatadogTracesConfig {
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
    fn validate_rejects_non_http_scheme() {
        let config = DatadogTracesConfig {
            local_dd_common: LocalDatadogCommonConfig::new(
                Some("ftp://localhost:8080".to_string()),
                None,
                None,
            ),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }
}
