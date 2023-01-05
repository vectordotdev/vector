use std::sync::Arc;

use codecs::TextSerializerConfig;
use futures_util::FutureExt;
use tower::ServiceBuilder;
use vector_common::sensitive_string::SensitiveString;
use vector_config::configurable_component;
use vector_core::sink::VectorSink;

use super::{encoder::HecLogsEncoder, request_builder::HecLogsRequestBuilder, sink::HecLogsSink};
use crate::{
    codecs::{Encoder, EncodingConfig},
    config::{AcknowledgementsConfig, DataType, GenerateConfig, Input, SinkConfig, SinkContext},
    http::HttpClient,
    sinks::{
        splunk_hec::common::{
            acknowledgements::HecClientAcknowledgementsConfig,
            build_healthcheck, build_http_batch_service, create_client, host_key,
            service::{HecService, HttpRequestBuilder},
            timestamp_key, EndpointTarget, SplunkHecDefaultBatchSettings,
        },
        util::{
            http::HttpRetryLogic, BatchConfig, Compression, ServiceBuilderExt, TowerRequestConfig,
        },
        Healthcheck,
    },
    template::Template,
    tls::TlsConfig,
};

/// Configuration for the `splunk_hec_logs` sink.
#[configurable_component(sink("splunk_hec_logs"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct HecLogsSinkConfig {
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

    /// Fields to be [added to Splunk index][splunk_field_index_docs].
    ///
    /// [splunk_field_index_docs]: https://docs.splunk.com/Documentation/Splunk/8.0.0/Data/IFXandHEC
    #[serde(default)]
    pub indexed_fields: Vec<String>,

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
    pub encoding: EncodingConfig,

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

    // This settings is relevant only for the `humio_logs` sink and should be left as `None`
    // everywhere else.
    #[serde(skip)]
    pub timestamp_nanos_key: Option<String>,

    /// Overrides the name of the log field used to grab the timestamp to send to Splunk HEC.
    ///
    /// By default, the [global `log_schema.timestamp_key` option][global_timestamp_key] is used.
    ///
    /// [global_timestamp_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.timestamp_key
    #[serde(default = "crate::sinks::splunk_hec::common::timestamp_key")]
    pub timestamp_key: String,

    /// Passes the auto_extract_timestamp option to Splunk.
    /// Note this option is only used by Version 8 and above of Splunk.
    /// This will cause Splunk to extract the timestamp from the message text rather than use
    /// the timestamp embedded in the event. The timestamp must be in the format yyyy-mm-dd hh:mm:ss.
    /// This option only applies for the `Event` endpoint target.
    #[serde(default)]
    pub auto_extract_timestamp: Option<bool>,

    #[configurable(derived)]
    #[serde(default = "default_endpoint_target")]
    pub endpoint_target: EndpointTarget,
}

const fn default_endpoint_target() -> EndpointTarget {
    EndpointTarget::Event
}

impl GenerateConfig for HecLogsSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            default_token: "${VECTOR_SPLUNK_HEC_TOKEN}".to_owned().into(),
            endpoint: "endpoint".to_owned(),
            host_key: host_key(),
            indexed_fields: vec![],
            index: None,
            sourcetype: None,
            source: None,
            encoding: TextSerializerConfig::default().into(),
            compression: Compression::default(),
            batch: BatchConfig::default(),
            request: TowerRequestConfig::default(),
            tls: None,
            acknowledgements: Default::default(),
            timestamp_nanos_key: None,
            timestamp_key: timestamp_key(),
            auto_extract_timestamp: None,
            endpoint_target: EndpointTarget::Event,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
impl SinkConfig for HecLogsSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        if self.auto_extract_timestamp.is_some() && self.endpoint_target == EndpointTarget::Raw {
            return Err("`auto_extract_timestamp` cannot be set for the `raw` endpoint.".into());
        }

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
        Input::new(self.encoding.config().input_type() & DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements.inner
    }
}

impl HecLogsSinkConfig {
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

        let transformer = self.encoding.transformer();
        let serializer = self.encoding.build()?;
        let encoder = Encoder::<()>::new(serializer);
        let encoder = HecLogsEncoder {
            transformer,
            encoder,
            auto_extract_timestamp: self.auto_extract_timestamp.unwrap_or_default(),
        };
        let request_builder = HecLogsRequestBuilder {
            encoder,
            compression: self.compression,
        };

        let request_settings = self.request.unwrap_with(&TowerRequestConfig::default());
        let http_request_builder = Arc::new(HttpRequestBuilder::new(
            self.endpoint.clone(),
            self.endpoint_target,
            self.default_token.inner().to_owned(),
            self.compression,
        ));
        let http_service = ServiceBuilder::new()
            .settings(request_settings, HttpRetryLogic)
            .service(build_http_batch_service(
                client,
                Arc::clone(&http_request_builder),
                self.endpoint_target,
                self.auto_extract_timestamp.unwrap_or_default(),
            ));

        let service = HecService::new(
            http_service,
            ack_client,
            http_request_builder,
            self.acknowledgements.clone(),
        );

        let batch_settings = self.batch.into_batcher_settings()?;

        let sink = HecLogsSink {
            service,
            request_builder,
            context: cx,
            batch_settings,
            sourcetype: self.sourcetype.clone(),
            source: self.source.clone(),
            index: self.index.clone(),
            indexed_fields: self.indexed_fields.clone(),
            host: self.host_key.clone(),
            timestamp_nanos_key: self.timestamp_nanos_key.clone(),
            timestamp_key: self.timestamp_key.clone(),
            endpoint_target: self.endpoint_target,
        };

        Ok(VectorSink::from_event_streamsink(sink))
    }
}

#[cfg(test)]
mod tests {
    use super::HecLogsSinkConfig;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<HecLogsSinkConfig>();
    }
}
