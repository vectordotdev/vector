use std::sync::Arc;

use futures_util::FutureExt;
use tower::ServiceBuilder;
use vector_lib::configurable::configurable_component;
use vector_lib::lookup::lookup_v2::OptionalValuePath;
use vector_lib::sensitive_string::SensitiveString;
use vector_lib::sink::VectorSink;

use super::{request_builder::HecMetricsRequestBuilder, sink::HecMetricsSink};
use crate::{
    config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext},
    http::HttpClient,
    sinks::{
        splunk_hec::common::{
            acknowledgements::HecClientAcknowledgementsConfig,
            build_healthcheck, build_http_batch_service, config_host_key, create_client,
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
#[configurable_component(sink(
    "splunk_hec_metrics",
    "Deliver metric data to Splunk's HTTP Event Collector."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct HecMetricsSinkConfig {
    /// Sets the default namespace for any metrics sent.
    ///
    /// This namespace is only used if a metric has no existing namespace. When a namespace is
    /// present, it is used as a prefix to the metric name, and separated with a period (`.`).
    #[configurable(metadata(docs::examples = "service"))]
    pub default_namespace: Option<String>,

    /// Default Splunk HEC token.
    ///
    /// If an event has a token set in its metadata, it prevails over the one set here.
    #[serde(alias = "token")]
    #[configurable(metadata(
        docs::examples = "${SPLUNK_HEC_TOKEN}",
        docs::examples = "A94A8FE5CCB19BA61C4C08"
    ))]
    pub default_token: SensitiveString,

    /// The base URL of the Splunk instance.
    ///
    /// The scheme (`http` or `https`) must be specified. No path should be included since the paths defined
    /// by the [`Splunk`][splunk] API are used.
    ///
    /// [splunk]: https://docs.splunk.com/Documentation/Splunk/8.0.0/Data/HECRESTendpoints
    #[configurable(metadata(
        docs::examples = "https://http-inputs-hec.splunkcloud.com",
        docs::examples = "https://hec.splunk.com:8088",
        docs::examples = "http://example.com"
    ))]
    #[configurable(validation(format = "uri"))]
    pub endpoint: String,

    /// Overrides the name of the log field used to retrieve the hostname to send to Splunk HEC.
    ///
    /// By default, the [global `log_schema.host_key` option][global_host_key] is used.
    ///
    /// [global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
    #[configurable(metadata(docs::advanced))]
    #[serde(default = "config_host_key")]
    pub host_key: OptionalValuePath,

    /// The name of the index where to send the events to.
    ///
    /// If not specified, the default index defined within Splunk is used.
    #[configurable(metadata(docs::examples = "{{ host }}", docs::examples = "custom_index"))]
    pub index: Option<Template>,

    /// The sourcetype of events sent to this sink.
    ///
    /// If unset, Splunk defaults to `httpevent`.
    #[configurable(metadata(docs::advanced))]
    #[configurable(metadata(docs::examples = "{{ sourcetype }}", docs::examples = "_json",))]
    pub sourcetype: Option<Template>,

    /// The source of events sent to this sink.
    ///
    /// This is typically the filename the logs originated from.
    ///
    /// If unset, the Splunk collector sets it.
    #[configurable(metadata(docs::advanced))]
    #[configurable(metadata(
        docs::examples = "{{ file }}",
        docs::examples = "/var/log/syslog",
        docs::examples = "UDP:514"
    ))]
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
            host_key: config_host_key(),
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
#[typetag::serde(name = "splunk_hec_metrics")]
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

        let request_settings = self.request.into_settings();
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
            host_key: self.host_key.path.clone(),
            default_namespace: self.default_namespace.clone(),
        };

        Ok(VectorSink::from_event_streamsink(sink))
    }
}
