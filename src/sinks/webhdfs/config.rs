use opendal::{Operator, layers::LoggingLayer, services::Webhdfs};
use tower::ServiceBuilder;
use vector_lib::{
    codecs::{JsonSerializerConfig, NewlineDelimitedEncoderConfig, encoding::Framer},
    config::{AcknowledgementsConfig, DataType, Input},
    configurable::configurable_component,
    sink::VectorSink,
    stream::BatcherSettings,
};

use crate::{
    codecs::{Encoder, EncodingConfigWithFraming, SinkType},
    config::{GenerateConfig, SinkConfig, SinkContext, ValidatedSink},
    sinks::{
        Healthcheck,
        opendal_common::*,
        util::{
            BatchConfig, BulkSizeBasedDefaultBatchSettings, Compression, HttpEndpoint,
            partitioner::KeyPartitioner,
        },
    },
    template::{ConfinedTemplate, ConfinementConfig, Template},
};

/// The default WebHDFS endpoint, used when `endpoint` is not configured.
fn default_endpoint() -> HttpEndpoint {
    HttpEndpoint::parse("http://127.0.0.1:9870")
        .expect("static default endpoint should be a valid http(s) URL")
}

/// Configuration for the `webhdfs` sink.
#[configurable_component(sink("webhdfs", "WebHDFS."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct WebHdfsConfig {
    /// The root path for WebHDFS.
    ///
    /// Must be a valid directory.
    ///
    /// The final file path is in the format of `{root}/{prefix}{suffix}`.
    #[serde(default)]
    pub root: String,

    /// A prefix to apply to all keys.
    ///
    /// Prefixes are useful for partitioning objects, such as by creating a blob key that
    /// stores blobs under a particular directory. If using a prefix for this purpose, it must end
    /// in `/` to act as a directory path. A trailing `/` is **not** automatically added.
    ///
    /// The final file path is in the format of `{root}/{prefix}{suffix}`.
    #[serde(default)]
    #[configurable(metadata(docs::templateable))]
    pub prefix: String,

    /// An HDFS cluster consists of a single NameNode, a master server that manages the file system namespace and regulates access to files by clients.
    ///
    /// The endpoint is the HDFS's web restful HTTP API endpoint.
    ///
    /// For more information, see the [HDFS Architecture][hdfs_arch] documentation.
    ///
    /// [hdfs_arch]: https://hadoop.apache.org/docs/r3.3.4/hadoop-project-dist/hadoop-hdfs/HdfsDesign.html#NameNode_and_DataNodes
    #[serde(default = "default_endpoint")]
    #[configurable(metadata(docs::examples = "http://127.0.0.1:9870"))]
    pub endpoint: HttpEndpoint,

    #[serde(flatten)]
    pub encoding: EncodingConfigWithFraming,

    #[serde(default = "Compression::gzip_default")]
    pub compression: Compression,

    #[serde(default)]
    pub batch: BatchConfig<BulkSizeBasedDefaultBatchSettings>,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    #[serde(flatten)]
    pub confinement: ConfinementConfig,
}

impl GenerateConfig for WebHdfsConfig {
    fn generate_config() -> serde_json::Value {
        serde_json::to_value(Self {
            root: "/".to_string(),
            prefix: "%F/".to_string(),
            endpoint: default_endpoint(),

            encoding: (
                Some(NewlineDelimitedEncoderConfig::new()),
                JsonSerializerConfig::default(),
            )
                .into(),
            compression: Compression::gzip_default(),
            batch: BatchConfig::default(),

            acknowledgements: Default::default(),
            confinement: ConfinementConfig::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "webhdfs")]
impl SinkConfig for WebHdfsConfig {
    fn confinement_config(&self) -> Option<&crate::template::ConfinementConfig> {
        Some(&self.confinement)
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().1.input_type() & DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedWebHdfs {
    batcher_settings: BatcherSettings,
    confined_prefix: ConfinedTemplate,
}

#[async_trait::async_trait]
impl ValidatedSink for WebHdfsConfig {
    type Validated = ValidatedWebHdfs;

    fn validate(&self) -> crate::Result<ValidatedWebHdfs> {
        let batcher_settings = self.batch.into_batcher_settings()?;
        let confined_prefix = self.confined_prefix()?;

        Ok(ValidatedWebHdfs {
            batcher_settings,
            confined_prefix,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedWebHdfs,
        _cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let op = self.build_operator()?;

        let check_op = op.clone();
        let healthcheck = Box::pin(async move { Ok(check_op.check().await?) });

        let sink = self.build_processor(op, validated)?;
        Ok((sink, healthcheck))
    }
}

impl WebHdfsConfig {
    pub fn build_operator(&self) -> crate::Result<Operator> {
        install_opendal_defaults();

        // Build OpenDal Operator
        let mut builder = Webhdfs::default();
        // Prefix logic will be handled by key_partitioner.
        builder = builder.root(&self.root);
        builder = builder.endpoint(&self.endpoint.to_string());

        let op = Operator::new(builder)?.layer(LoggingLayer::default());
        Ok(op)
    }

    pub fn build_processor(
        &self,
        op: Operator,
        validated: &ValidatedWebHdfs,
    ) -> crate::Result<VectorSink> {
        let ValidatedWebHdfs {
            batcher_settings,
            confined_prefix,
        } = validated.clone();

        let (framer, serializer) = self.encoding.build(SinkType::MessageBased)?;
        let encoder = Encoder::<Framer>::new(framer, serializer);

        let request_builder = OpenDalRequestBuilder {
            encoder: (self.encoding.transformer(), encoder),
            compression: self.compression,
        };

        // TODO: we can add tower middleware here.
        let svc = ServiceBuilder::new().service(OpenDalService::new(op));

        let sink = OpenDalSink::new(
            svc,
            request_builder,
            KeyPartitioner::new(confined_prefix, None),
            batcher_settings,
        );

        Ok(VectorSink::from_event_streamsink(sink))
    }

    fn confined_prefix(&self) -> crate::Result<ConfinedTemplate> {
        let prefix: Template = self.prefix.clone().try_into()?;
        prefix.confine(&self.confinement, Self::NAME, "prefix")
    }

    pub fn key_partitioner(&self) -> crate::Result<KeyPartitioner> {
        let prefix = self.confined_prefix()?;
        Ok(KeyPartitioner::new(prefix, None))
    }
}

/// Register OpenDAL services and install the native-tls HTTP transport.
///
/// `opendal::install_default` registers enabled services, but HTTP-transport
/// auto-install is gated on the `http-transport-reqwest` alias (rustls/aws-lc).
/// We use `http-transport-reqwest-native-tls` instead, so the transport is
/// installed separately. Both calls are idempotent.
fn install_opendal_defaults() {
    opendal::install_default();
    opendal_http_transport_reqwest::install_default();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<WebHdfsConfig>();
    }

    fn base_config() -> WebHdfsConfig {
        WebHdfsConfig {
            root: "/tmp/test/".into(),
            prefix: String::new(),
            endpoint: HttpEndpoint::parse("http://127.0.0.1:9870").unwrap(),
            encoding: (
                None::<vector_lib::codecs::encoding::FramingConfig>,
                vector_lib::codecs::TextSerializerConfig::default(),
            )
                .into(),
            compression: crate::sinks::util::Compression::None,
            batch: Default::default(),
            acknowledgements: Default::default(),
            confinement: ConfinementConfig::default(),
        }
    }

    #[test]
    fn confinement_rejects_unconfined_prefix() {
        let config = WebHdfsConfig {
            prefix: "{{ tenant }}".into(),
            ..base_config()
        };
        match config.key_partitioner() {
            Err(err) => assert!(
                err.to_string().contains("no literal string prefix"),
                "unexpected error: {err}"
            ),
            Ok(_) => panic!("expected confinement error"),
        }
    }

    #[test]
    fn confinement_opt_out_allows_unconfined_prefix() {
        let config = WebHdfsConfig {
            prefix: "{{ tenant }}".into(),
            confinement: ConfinementConfig {
                dangerously_allow_unconfined_template_resolution: true,
            },
            ..base_config()
        };
        assert!(config.key_partitioner().is_ok());
    }

    #[test]
    fn confinement_blocks_dotdot_escape_at_render() {
        use crate::event::Event;
        use vector_lib::event::LogEvent;
        use vector_lib::partition::Partitioner;
        use vrl::event_path;

        let config = WebHdfsConfig {
            prefix: "safe/{{ tenant }}/".into(),
            ..base_config()
        };
        let partitioner = config.key_partitioner().unwrap();
        let mut event = Event::Log(LogEvent::from("x"));
        event
            .as_mut_log()
            .insert(event_path!("tenant"), "../../escape");
        assert!(partitioner.partition(&event).is_none());
    }

    #[test]
    fn validate_produces_usable_values() {
        use crate::config::ValidatedSink;

        let config = WebHdfsConfig {
            prefix: "%F/".into(),
            ..base_config()
        };
        let validated = config.validate().expect("validation should succeed");
        // Bulk size-based defaults: 10 MB batches, 300s timeout.
        assert_eq!(
            validated.batcher_settings.timeout,
            std::time::Duration::from_secs(300)
        );
        assert_eq!(validated.batcher_settings.size_limit, 10_000_000);
        // The confined prefix retains the validated value.
        assert_eq!(validated.confined_prefix.to_string(), "%F/");
    }
}
