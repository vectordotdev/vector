use std::sync::Arc;

use vector_lib::{
    codecs::TextSerializerConfig,
    lookup::lookup_v2::{ConfigValuePath, OptionalTargetPath},
    sensitive_string::SensitiveString,
};

use super::{encoder::HecLogsEncoder, request_builder::HecLogsRequestBuilder, sink::HecLogsSink};
use crate::{
    config::ValidatedSink,
    http::HttpClient,
    sinks::{
        prelude::*,
        splunk_hec::common::{
            EndpointTarget, SplunkHecDefaultBatchSettings,
            acknowledgements::HecClientAcknowledgementsConfig,
            build_healthcheck, build_http_batch_service, create_client,
            service::{HecService, HttpRequestBuilder},
        },
        util::{HttpEndpoint, http::HttpRetryLogic},
    },
    template::ConfinementConfig,
};

/// Configuration for the `splunk_hec_logs` sink.
#[configurable_component(sink(
    "splunk_hec_logs",
    "Deliver log data to Splunk's HTTP Event Collector."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct HecLogsSinkConfig {
    /// Default Splunk HEC token.
    ///
    /// If an event has a token set in its secrets (`splunk_hec_token`), it prevails over the one set here.
    #[serde(alias = "token")]
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
    /// By default, the [global `log_schema.host_key` option][global_host_key] is used if log
    /// events are Legacy namespaced, or the semantic meaning of "host" is used, if defined.
    ///
    /// [global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
    // NOTE: The `OptionalTargetPath` is wrapped in an `Option` in order to distinguish between a true
    //       `None` type and an empty string. This is necessary because `OptionalTargetPath` deserializes an
    //       empty string to a `None` path internally.
    pub host_key: Option<OptionalTargetPath>,

    /// Fields to be [added to Splunk index][splunk_field_index_docs].
    ///
    /// [splunk_field_index_docs]: https://docs.splunk.com/Documentation/Splunk/8.0.0/Data/IFXandHEC
    #[serde(default)]
    #[configurable(metadata(docs::examples = "field1", docs::examples = "field2"))]
    pub indexed_fields: Vec<ConfigValuePath>,

    /// The name of the index to send events to.
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

    pub encoding: EncodingConfig,

    #[serde(default)]
    pub compression: Compression,

    #[serde(default)]
    pub batch: BatchConfig<SplunkHecDefaultBatchSettings>,

    #[serde(default)]
    pub request: TowerRequestConfig,

    pub tls: Option<TlsConfig>,

    #[serde(default)]
    pub acknowledgements: HecClientAcknowledgementsConfig,

    // This settings is relevant only for the `humio_logs` sink and should be left as `None`
    // everywhere else.
    #[serde(skip)]
    pub timestamp_nanos_key: Option<String>,

    /// Overrides the name of the log field used to retrieve the timestamp to send to Splunk HEC.
    /// When set to `“”`, a timestamp is not set in the events sent to Splunk HEC.
    ///
    /// By default, either the [global `log_schema.timestamp_key` option][global_timestamp_key] is used
    /// if log events are Legacy namespaced, or the semantic meaning of "timestamp" is used, if defined.
    ///
    /// [global_timestamp_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.timestamp_key
    #[configurable(metadata(docs::examples = "timestamp", docs::examples = ""))]
    // NOTE: The `OptionalTargetPath` is wrapped in an `Option` in order to distinguish between a true
    //       `None` type and an empty string. This is necessary because `OptionalTargetPath` deserializes an
    //       empty string to a `None` path internally.
    pub timestamp_key: Option<OptionalTargetPath>,

    /// Passes the `auto_extract_timestamp` option to Splunk.
    ///
    /// This option is only relevant to Splunk v8.x and above, and is only applied when
    /// `endpoint_target` is set to `event`.
    ///
    /// Setting this to `true` causes Splunk to extract the timestamp from the message text
    /// rather than use the timestamp embedded in the event. The timestamp must be in the format
    /// `yyyy-mm-dd hh:mm:ss`.
    #[serde(default)]
    pub auto_extract_timestamp: Option<bool>,

    #[serde(default = "default_endpoint_target")]
    pub endpoint_target: EndpointTarget,

    #[serde(flatten)]
    pub confinement: ConfinementConfig,
}

const fn default_endpoint_target() -> EndpointTarget {
    EndpointTarget::Event
}

impl GenerateConfig for HecLogsSinkConfig {
    fn generate_config() -> serde_json::Value {
        serde_json::to_value(Self {
            default_token: "${VECTOR_SPLUNK_HEC_TOKEN}".to_owned().into(),
            endpoint: HttpEndpoint::parse("http://example.com").unwrap(),
            host_key: None,
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
            timestamp_key: None,
            auto_extract_timestamp: None,
            endpoint_target: EndpointTarget::Event,
            confinement: ConfinementConfig::default(),
        })
        .unwrap()
    }
}

impl HecLogsSinkConfig {
    /// Pure structural validation. `component_name` is threaded into the
    /// per-template security warnings emitted from `Template::confine`, so
    /// wrapping sinks (Humio) see their own type in logs rather than the
    /// delegated `splunk_hec_logs`.
    pub(crate) fn validate_with_component_name(
        &self,
        component_name: &'static str,
    ) -> crate::Result<ValidatedHecLogsSink> {
        if self.auto_extract_timestamp.is_some() && self.endpoint_target == EndpointTarget::Raw {
            return Err("`auto_extract_timestamp` cannot be set for the `raw` endpoint.".into());
        }

        // The endpoint is validated at config load as an absolute http(s) URL.

        let index = self
            .index
            .clone()
            .map(|t| t.confine(&self.confinement, component_name, "index"))
            .transpose()?;
        let source = self
            .source
            .clone()
            .map(|t| t.confine(&self.confinement, component_name, "source"))
            .transpose()?;
        let sourcetype = self
            .sourcetype
            .clone()
            .map(|t| t.confine(&self.confinement, component_name, "sourcetype"))
            .transpose()?;

        let batch_settings = self.batch.into_batcher_settings()?;

        Ok(ValidatedHecLogsSink {
            index,
            source,
            sourcetype,
            batch_settings,
        })
    }

    /// Build the sink from validated state. Only environment-dependent work
    /// (client creation, healthcheck) happens here; the validated state is
    /// consumed without recomputing any pure validation.
    pub(crate) fn build_from_validated(
        &self,
        cx: SinkContext,
        validated: &ValidatedHecLogsSink,
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

#[async_trait::async_trait]
#[typetag::serde(name = "splunk_hec_logs")]
impl SinkConfig for HecLogsSinkConfig {
    fn confinement_config(&self) -> Option<&crate::template::ConfinementConfig> {
        Some(&self.confinement)
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().input_type() & DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements.inner
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedHecLogsSink {
    index: Option<ConfinedTemplate>,
    source: Option<ConfinedTemplate>,
    sourcetype: Option<ConfinedTemplate>,
    batch_settings: BatcherSettings,
}

#[async_trait::async_trait]
impl ValidatedSink for HecLogsSinkConfig {
    type Validated = ValidatedHecLogsSink;

    fn validate(&self) -> crate::Result<ValidatedHecLogsSink> {
        self.validate_with_component_name(Self::NAME)
    }

    async fn build(
        &self,
        validated: &ValidatedHecLogsSink,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        self.build_from_validated(cx, validated)
    }
}

impl HecLogsSinkConfig {
    pub fn build_processor(
        &self,
        client: HttpClient,
        _: SinkContext,
        validated: &ValidatedHecLogsSink,
    ) -> crate::Result<VectorSink> {
        let ack_client = if self.acknowledgements.indexer_acknowledgements_enabled {
            Some(client.clone())
        } else {
            None
        };

        let transformer = self.encoding.transformer();
        let serializer = self.encoding.build()?;
        let encoder = HecLogsEncoder {
            transformer,
            encoder: Encoder::<()>::new(serializer),
            auto_extract_timestamp: self.auto_extract_timestamp.unwrap_or_default(),
        };
        let request_builder = HecLogsRequestBuilder {
            encoder,
            compression: self.compression,
        };

        let request_settings = self.request.into_settings();
        let http_request_builder = Arc::new(HttpRequestBuilder::new(
            self.endpoint.clone().into(),
            self.endpoint_target,
            self.default_token.inner().to_owned(),
            self.compression,
        ));
        let http_service = ServiceBuilder::new()
            .settings(request_settings, HttpRetryLogic::default())
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

        let sink = HecLogsSink {
            service,
            request_builder,
            batch_settings: validated.batch_settings,
            sourcetype: validated.sourcetype.clone(),
            source: validated.source.clone(),
            index: validated.index.clone(),
            indexed_fields: self
                .indexed_fields
                .iter()
                .map(|config_path| config_path.0.clone())
                .collect(),
            host_key: self.host_key.clone(),
            timestamp_nanos_key: self.timestamp_nanos_key.clone(),
            timestamp_key: self.timestamp_key.clone(),
            endpoint_target: self.endpoint_target,
            auto_extract_timestamp: self.auto_extract_timestamp.unwrap_or_default(),
        };

        Ok(VectorSink::from_event_streamsink(sink))
    }
}

#[cfg(test)]
mod tests {
    use vector_lib::{
        codecs::{JsonSerializerConfig, MetricTagValues, encoding::format::JsonSerializerOptions},
        config::LogNamespace,
    };

    use super::*;
    use crate::components::validation::prelude::*;
    use crate::template::{ConfinementConfig, Template};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<HecLogsSinkConfig>();
    }

    #[test]
    fn validate_produces_usable_state() {
        use crate::config::ValidatedSink;

        let config = HecLogsSinkConfig {
            default_token: "token".to_string().into(),
            endpoint: HttpEndpoint::parse("http://localhost:8088").unwrap(),
            host_key: None,
            indexed_fields: vec![],
            index: Some("custom_index".try_into().unwrap()),
            sourcetype: None,
            source: None,
            encoding: JsonSerializerConfig::default().into(),
            compression: Compression::default(),
            batch: Default::default(),
            request: Default::default(),
            tls: None,
            acknowledgements: Default::default(),
            timestamp_nanos_key: None,
            timestamp_key: None,
            auto_extract_timestamp: None,
            endpoint_target: EndpointTarget::Event,
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

    #[test]
    fn confinement_rejects_unconfined_index() {
        let template = Template::try_from("{{ index }}").unwrap();
        let config = ConfinementConfig::default();
        let result = template.confine(&config, "splunk_hec_logs", "index");
        assert!(result.is_err());
    }

    #[test]
    fn confinement_opt_out_allows_unconfined_index() {
        let template = Template::try_from("{{ index }}").unwrap();
        let config = ConfinementConfig {
            dangerously_allow_unconfined_template_resolution: true,
        };
        let result = template.confine(&config, "splunk_hec_logs", "index");
        assert!(result.is_ok());
    }

    #[test]
    fn confinement_allows_prefixed_index() {
        let template = Template::try_from("events-{{ env }}").unwrap();
        let config = ConfinementConfig::default();
        let result = template.confine(&config, "splunk_hec_logs", "index");
        assert!(result.is_ok());
    }

    impl ValidatableComponent for HecLogsSinkConfig {
        fn validation_configuration() -> ValidationConfiguration {
            let endpoint = HttpEndpoint::parse("http://127.0.0.1:9001").unwrap();

            let mut batch = BatchConfig::default();
            batch.max_events = Some(1);

            let config = Self {
                endpoint: endpoint.clone(),
                default_token: "i_am_an_island".to_string().into(),
                host_key: None,
                indexed_fields: vec![],
                index: None,
                sourcetype: None,
                source: None,
                encoding: EncodingConfig::new(
                    JsonSerializerConfig::new(
                        MetricTagValues::Full,
                        JsonSerializerOptions::default(),
                    )
                    .into(),
                    Transformer::default(),
                ),
                compression: Compression::default(),
                batch,
                request: TowerRequestConfig {
                    timeout_secs: 2,
                    retry_attempts: 0,
                    ..Default::default()
                },
                tls: None,
                acknowledgements: HecClientAcknowledgementsConfig {
                    indexer_acknowledgements_enabled: false,
                    ..Default::default()
                },
                timestamp_nanos_key: None,
                timestamp_key: None,
                auto_extract_timestamp: None,
                endpoint_target: EndpointTarget::Raw,
                confinement: ConfinementConfig::default(),
            };

            let endpoint = endpoint
                .append_path("services/collector/raw")
                .unwrap()
                .into_uri();

            let external_resource = ExternalResource::new(
                ResourceDirection::Push,
                HttpResourceConfig::from_parts(endpoint, None),
                config.encoding.clone(),
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

    register_validatable_component!(HecLogsSinkConfig);
}
