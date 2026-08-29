use std::sync::Arc;

use futures_util::FutureExt;
use tower::ServiceBuilder;
use vector_lib::{
    configurable::configurable_component, lookup::lookup_v2::OptionalValuePath,
    sensitive_string::SensitiveString, sink::VectorSink, stream::BatcherSettings,
};

use super::{request_builder::HecMetricsRequestBuilder, sink::HecMetricsSink};

use crate::{
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext, ValidatedSink,
    },
    http::HttpClient,
    sinks::{
        Healthcheck,
        splunk_hec::common::{
            EndpointTarget, SplunkHecDefaultBatchSettings,
            acknowledgements::HecClientAcknowledgementsConfig,
            build_healthcheck, build_http_batch_service, config_host_key, create_client,
            service::{HecService, HttpRequestBuilder},
        },
        util::{
            BatchConfig, Compression, HttpEndpoint, ServiceBuilderExt, TowerRequestConfig,
            http::HttpRetryLogic,
        },
    },
    template::{ConfinedTemplate, Template},
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
    pub endpoint: HttpEndpoint,

    /// Overrides the name of the log field used to retrieve the hostname to send to Splunk HEC.
    ///
    /// By default, the [global `log_schema.host_key` option][global_host_key] is used.
    ///
    /// [global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
    #[serde(default = "config_host_key")]
    pub host_key: OptionalValuePath,

    /// The name of the index where to send the events to.
    ///
    /// If not specified, the default index defined within Splunk is used.
    #[configurable(metadata(
        docs::examples = "index-{{ host }}",
        docs::examples = "custom_index"
    ))]
    pub index: Option<Template>,

    /// The sourcetype of events sent to this sink.
    ///
    /// If unset, Splunk defaults to `httpevent`.
    #[configurable(metadata(
        docs::examples = "sourcetype-{{ sourcetype }}",
        docs::examples = "_json",
    ))]
    pub sourcetype: Option<Template>,

    /// The source of events sent to this sink.
    ///
    /// This is typically the filename the logs originated from.
    ///
    /// If unset, the Splunk collector sets it.
    #[configurable(metadata(
        docs::examples = "source-{{ file }}",
        docs::examples = "/var/log/syslog",
        docs::examples = "UDP:514"
    ))]
    pub source: Option<Template>,

    #[serde(default)]
    pub compression: Compression,

    #[serde(default)]
    pub batch: BatchConfig<SplunkHecDefaultBatchSettings>,

    #[serde(default)]
    pub request: TowerRequestConfig,

    pub tls: Option<TlsConfig>,

    #[serde(default)]
    pub acknowledgements: HecClientAcknowledgementsConfig,

    #[serde(flatten)]
    pub confinement: crate::template::ConfinementConfig,
}

impl GenerateConfig for HecMetricsSinkConfig {
    fn generate_config() -> serde_json::Value {
        serde_json::to_value(Self {
            default_namespace: None,
            default_token: "${VECTOR_SPLUNK_HEC_TOKEN}".to_owned().into(),
            endpoint: HttpEndpoint::parse("http://localhost:8088").unwrap(),
            host_key: config_host_key(),
            index: None,
            sourcetype: None,
            source: None,
            compression: Compression::default(),
            batch: BatchConfig::default(),
            request: TowerRequestConfig::default(),
            tls: None,
            acknowledgements: Default::default(),
            confinement: Default::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "splunk_hec_metrics")]
impl SinkConfig for HecMetricsSinkConfig {
    fn confinement_config(&self) -> Option<&crate::template::ConfinementConfig> {
        Some(&self.confinement)
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements.inner
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedHecMetricsSink {
    index: Option<ConfinedTemplate>,
    source: Option<ConfinedTemplate>,
    sourcetype: Option<ConfinedTemplate>,
    templated_field_keys: Box<[String]>,
    batch_settings: BatcherSettings,
}

#[async_trait::async_trait]
impl ValidatedSink for HecMetricsSinkConfig {
    type Validated = ValidatedHecMetricsSink;

    fn validate(&self) -> crate::Result<ValidatedHecMetricsSink> {
        // The endpoint is validated at config load as an absolute http(s) URL.

        let templated_field_keys =
            compute_templated_field_keys(&self.index, &self.source, &self.sourcetype);

        let confined_sourcetype = self
            .sourcetype
            .clone()
            .map(|t| t.confine(&self.confinement, Self::NAME, "sourcetype"))
            .transpose()?;
        let confined_source = self
            .source
            .clone()
            .map(|t| t.confine(&self.confinement, Self::NAME, "source"))
            .transpose()?;
        let confined_index = self
            .index
            .clone()
            .map(|t| t.confine(&self.confinement, Self::NAME, "index"))
            .transpose()?;

        let batch_settings = self.batch.into_batcher_settings()?;

        Ok(ValidatedHecMetricsSink {
            index: confined_index,
            source: confined_source,
            sourcetype: confined_sourcetype,
            templated_field_keys,
            batch_settings,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedHecMetricsSink,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = create_client(self.tls.as_ref(), cx.proxy())?;
        let healthcheck = build_healthcheck(
            self.endpoint.clone().into(),
            self.default_token.inner().to_owned(),
            client.clone(),
        )
        .boxed();
        let sink = self.build_processor(client, cx, validated)?;
        Ok((sink, healthcheck))
    }
}

pub(super) fn compute_templated_field_keys(
    index: &Option<Template>,
    source: &Option<Template>,
    sourcetype: &Option<Template>,
) -> Box<[String]> {
    [index, source, sourcetype]
        .iter()
        .filter_map(|t| t.as_ref())
        .filter_map(|t| t.get_fields())
        .flatten()
        .map(|f| f.replace("tags.", ""))
        .collect()
}

impl HecMetricsSinkConfig {
    pub fn build_processor(
        &self,
        client: HttpClient,
        _: SinkContext,
        validated: &ValidatedHecMetricsSink,
    ) -> crate::Result<VectorSink> {
        let ack_client = if self.acknowledgements.indexer_acknowledgements_enabled {
            Some(client.clone())
        } else {
            None
        };

        let request_builder =
            HecMetricsRequestBuilder::new(self.compression, validated.templated_field_keys.clone());

        let request_settings = self.request.into_settings();
        let http_request_builder = Arc::new(HttpRequestBuilder::new(
            self.endpoint.clone().into(),
            EndpointTarget::default(),
            self.default_token.inner().to_owned(),
            self.compression,
        ));
        let http_service = ServiceBuilder::new()
            .settings(request_settings, HttpRetryLogic::default())
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

        let sink = HecMetricsSink {
            service,
            batch_settings: validated.batch_settings,
            request_builder,
            sourcetype: validated.sourcetype.clone(),
            source: validated.source.clone(),
            index: validated.index.clone(),
            host_key: self.host_key.path.clone(),
            default_namespace: self.default_namespace.clone(),
        };

        Ok(VectorSink::from_event_streamsink(sink))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ValidatedSink;

    #[test]
    fn validate_produces_usable_state() {
        let config = HecMetricsSinkConfig {
            default_namespace: None,
            default_token: "token".to_string().into(),
            endpoint: HttpEndpoint::parse("http://localhost:8088").unwrap(),
            host_key: config_host_key(),
            index: Some("custom_index".try_into().unwrap()),
            sourcetype: None,
            source: None,
            compression: Compression::default(),
            batch: Default::default(),
            request: Default::default(),
            tls: None,
            acknowledgements: Default::default(),
            confinement: Default::default(),
        };

        let validated = config.validate().expect("validation should succeed");
        assert_eq!(
            validated.index.as_ref().unwrap().to_string(),
            "custom_index"
        );
        assert!(validated.source.is_none());
        assert!(validated.sourcetype.is_none());
    }
}
