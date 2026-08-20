use std::sync::{Arc, Mutex};

use futures_util::FutureExt;
use indoc::indoc;
use tokio::sync::oneshot::{Sender, channel};
use tower::ServiceBuilder;
use vector_lib::{config::AcknowledgementsConfig, configurable::configurable_component};

use super::{
    apm_stats::{Aggregator, flush_apm_stats},
    service::TraceApiRetry,
};
use crate::{
    config::{DynValidatedSink, GenerateConfig, Input, SinkConfig, SinkContext, ValidatedSink},
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
            TowerRequestSettings, service::ServiceBuilderExt,
        },
    },
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

    #[configurable(derived)]
    #[serde(default)]
    pub compression: Option<Compression>,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<DatadogTracesDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,
}

#[derive(Clone, Debug)]
pub struct ValidatedDatadogTraces {
    batcher_settings: vector_lib::stream::BatcherSettings,
    request_limits: TowerRequestSettings,
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
        validated: &ValidatedDatadogTraces,
    ) -> crate::Result<VectorSink> {
        let default_api_key: Arc<str> = Arc::from(dd_common.default_api_key.inner());
        let endpoints = self.generate_traces_endpoint_configuration(dd_common)?;

        let service = ServiceBuilder::new()
            .settings(validated.request_limits.clone(), TraceApiRetry)
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
        // tripwire= Receiver that the APM stats flusher listens for the exit signal on.
        let (shutdown, tripwire) = channel::<Sender<()>>();

        // Construct the APM stats flusher here; `TracesSink::run` drives it so build spawns no task.
        let protocol = endpoints.traces_endpoint.protocol().to_string();
        let flusher = flush_apm_stats(
            tripwire,
            client,
            compression,
            endpoints,
            Arc::clone(&apm_stats_aggregator),
        )
        .boxed();

        let sink = TracesSink::new(
            service,
            request_builder,
            validated.batcher_settings,
            shutdown,
            protocol,
            flusher,
        );

        Ok(VectorSink::from_event_streamsink(sink))
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

    fn as_dyn_validated(&self) -> Option<&dyn DynValidatedSink> {
        Some(self)
    }
}

#[async_trait::async_trait]
impl ValidatedSink for DatadogTracesConfig {
    type Validated = ValidatedDatadogTraces;

    fn validate(&self) -> crate::Result<Self::Validated> {
        if self.local_dd_common.endpoint.is_some() {
            self.local_dd_common.validate_endpoint()?;
        } else if let Some(site) = self.local_dd_common.site.as_deref() {
            HttpEndpoint::parse(&Self::traces_base_endpoint(None, site))?;
        }

        let batcher_settings = self
            .batch
            .validate()?
            .limit_max_bytes(BATCH_GOAL_BYTES)?
            .limit_max_events(BATCH_MAX_EVENTS)?
            .into_batcher_settings()?;

        Ok(ValidatedDatadogTraces {
            batcher_settings,
            request_limits: self.request.into_settings(),
        })
    }

    async fn build(
        &self,
        validated: &Self::Validated,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let dd_common = self.local_dd_common.with_globals_from(&cx)?;
        let client = self.local_dd_common.build_client(&cx.proxy, true)?;
        let healthcheck = dd_common.build_healthcheck(client.clone())?;
        let sink = self.build_sink(&dd_common, client, validated)?;

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
    fn validate_rejects_malformed_local_site_without_endpoint() {
        let config = DatadogTracesConfig {
            local_dd_common: LocalDatadogCommonConfig::new(
                None,
                Some("bad site".to_string()),
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
