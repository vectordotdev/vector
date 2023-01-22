use std::sync::Arc;

use futures_util::FutureExt;
use tower::ServiceBuilder;
use vector_common::sensitive_string::SensitiveString;
use vector_config::configurable_component;
use vector_core::sink::VectorSink;

use super::{request_builder::HecMetricsRequestBuilder, sink::HecMetricsSink};
use crate::{
    config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext},
    http::HttpClient,
    sinks::{
        splunk_hec::common::{
            acknowledgements::HecClientAcknowledgementsConfig,
            build_healthcheck, build_http_batch_service, create_client, host_key,
            service::{HecService, HttpRequestBuilder},
            EndpointTarget, SplunkHecDefaultBatchSettings,
        },
        util::{
            http::HttpRetryLogic, BatchConfig, Compression, ServiceBuilderExt, TowerRequestConfig,
        },
        Healthcheck,
    },
    template::Template,
    tls::TlsConfig,
};

/// Configuration of the `splunk_hec_metrics` sink.
#[configurable_component(sink("splunk_hec_metrics"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct HecMetricsSinkConfig {
    /// Sets the default namespace for any metrics sent.
    ///
    /// This namespace is only used if a metric has no existing namespace. When a namespace is
    /// present, it is used as a prefix to the metric name, and separated with a period (`.`).
    pub default_namespace: Option<String>,

    /// Default Splunk HEC token.
    ///
    /// If an event has a token set in its metadata, it will prevail over the one set here.
    #[serde(alias = "token")]
    pub default_token: SensitiveString,

    /// The base URL of the Splunk instance.
    pub endpoint: String,

    /// Overrides the name of the log field used to grab the hostname to send to Splunk HEC.
    ///
    /// By default, the [global `log_schema.host_key` option][global_host_key] is used.
    ///
    /// [global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
    #[serde(default = "host_key")]
    pub host_key: String,

    /// The name of the index where to send the events to.
    ///
    /// If not specified, the default index is used.
    pub index: Option<Template>,

    /// The sourcetype of events sent to this sink.
    ///
    /// If unset, Splunk will default to `httpevent`.
    pub sourcetype: Option<Template>,

    /// The source of events sent to this sink.
    ///
    /// This is typically the filename the logs originated from.
    ///
    /// If unset, the Splunk collector will set it.
    pub source: Option<Template>,

    #[configurable(derived)]
    #[serde(default)]
    pub compression: Compression,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<SplunkHecDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(default)]
    pub acknowledgements: HecClientAcknowledgementsConfig,
}

impl GenerateConfig for HecMetricsSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            default_namespace: None,
            default_token: "${VECTOR_SPLUNK_HEC_TOKEN}".to_owned().into(),
            endpoint: "http://localhost:8088".to_owned(),
            host_key: host_key(),
            index: None,
            sourcetype: None,
            source: None,
            compression: Compression::default(),
            batch: BatchConfig::default(),
            request: TowerRequestConfig::default(),
            tls: None,
            acknowledgements: Default::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
impl SinkConfig for HecMetricsSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = create_client(&self.tls, cx.proxy())?;
        let healthcheck = build_healthcheck(
            self.endpoint.clone(),
            self.default_token.inner().to_owned(),
            client.clone(),
        )
        .boxed();
        let sink = self.build_processor(client, cx)?;
        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements.inner
    }
}

impl HecMetricsSinkConfig {
    pub fn build_processor(
        &self,
        client: HttpClient,
        cx: SinkContext,
    ) -> crate::Result<VectorSink> {
        let ack_client = if self.acknowledgements.indexer_acknowledgements_enabled {
            Some(client.clone())
        } else {
            None
        };

        let request_builder = HecMetricsRequestBuilder {
            compression: self.compression,
        };

        let request_settings = self.request.unwrap_with(&TowerRequestConfig::default());
        let http_request_builder = Arc::new(HttpRequestBuilder::new(
            self.endpoint.clone(),
            EndpointTarget::default(),
            self.default_token.inner().to_owned(),
            self.compression,
        ));
        let http_service = ServiceBuilder::new()
            .settings(request_settings, HttpRetryLogic)
            .service(build_http_batch_service(
                client,
                Arc::clone(&http_request_builder),
                EndpointTarget::Event,
                false,
            ));

        let service = HecService::new(
            http_service,
            ack_client,
            http_request_builder,
            self.acknowledgements.clone(),
        );

        let batch_settings = self.batch.into_batcher_settings()?;

        let sink = HecMetricsSink {
            context: cx,
            service,
            batch_settings,
            request_builder,
            sourcetype: self.sourcetype.clone(),
            source: self.source.clone(),
            index: self.index.clone(),
            host: self.host_key.clone(),
            default_namespace: self.default_namespace.clone(),
        };

        Ok(VectorSink::from_event_streamsink(sink))
    }
}
