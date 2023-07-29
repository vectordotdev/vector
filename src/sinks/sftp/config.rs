use codecs::{encoding::Framer, JsonSerializerConfig, NewlineDelimitedEncoderConfig};
use opendal::{layers::LoggingLayer, services::Sftp, Operator};
use tower::ServiceBuilder;
use vector_config::configurable_component;
use vector_core::{
    config::{AcknowledgementsConfig, DataType, Input},
    sink::VectorSink,
};

use crate::{
    codecs::{Encoder, EncodingConfigWithFraming, SinkType},
    config::{GenerateConfig, SinkConfig, SinkContext},
    sinks::{
        opendal_common::*,
        util::{
            partitioner::KeyPartitioner, BatchConfig, BulkSizeBasedDefaultBatchSettings,
            Compression,
        },
        Healthcheck,
    },
};

/// Configuration for the `sftp` sink.
#[configurable_component(sink("sftp", "Sftp."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct SftpConfig {
    /// The root path for Sftp.
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

    /// The endpoint to connect to sftp.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "127.0.0.1:22"))]
    pub endpoint: String,

    /// The user to connect to sftp.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "ubuntu"))]
    pub user: String,

    /// The key path that sftp used to connect to sftp.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "/path/to/ssh/key/path"))]
    pub key: String,

    /// The known_hosts_strategy that sftp used to connect to sftp.
    ///
    /// Possible value includes:
    ///
    /// - Strict (default)
    /// - Accept
    /// - Add
    #[serde(default)]
    #[configurable(metadata(docs::examples = "strict"))]
    pub known_hosts_strategy: String,

    #[serde(flatten)]
    pub encoding: EncodingConfigWithFraming,

    #[configurable(derived)]
    #[serde(default = "Compression::gzip_default")]
    pub compression: Compression,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<BulkSizeBasedDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for SftpConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            root: "/".to_string(),
            prefix: "%F/".to_string(),
            endpoint: "127.0.0.1:22".to_string(),

            user: "ubuntu".to_string(),
            key: "/home/ubuntu/.ssh/id_rsa".to_string(),
            known_hosts_strategy: "strict".to_string(),
            encoding: (
                Some(NewlineDelimitedEncoderConfig::new()),
                JsonSerializerConfig::default(),
            )
                .into(),
            compression: Compression::gzip_default(),
            batch: BatchConfig::default(),

            acknowledgements: Default::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "sftp")]
impl SinkConfig for SftpConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let op = self.build_operator()?;

        let check_op = op.clone();
        let healthcheck = Box::pin(async move { Ok(check_op.check().await?) });

        let sink = self.build_processor(op)?;
        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().1.input_type() & DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl SftpConfig {
    pub fn build_operator(&self) -> crate::Result<Operator> {
        // Build OpenDal Operator
        let mut builder = Sftp::default();
        // Prefix logic will be handled by key_partitioner.
        builder.root(&self.root);
        builder.endpoint(&self.endpoint);
        builder.user(&self.user);
        builder.key(&self.key);
        builder.known_hosts_strategy(&self.known_hosts_strategy);

        let op = Operator::new(builder)?
            .layer(LoggingLayer::default())
            .finish();
        Ok(op)
    }

    pub fn build_processor(&self, op: Operator) -> crate::Result<VectorSink> {
        // Configure our partitioning/batching.
        let batcher_settings = self.batch.into_batcher_settings()?;

        let transformer = self.encoding.transformer();
        let (framer, serializer) = self.encoding.build(SinkType::MessageBased)?;
        let encoder = Encoder::<Framer>::new(framer, serializer);

        let request_builder = OpenDalRequestBuilder {
            encoder: (transformer, encoder),
            compression: self.compression,
        };

        // TODO: we can add tower middleware here.
        let svc = ServiceBuilder::new().service(OpenDalService::new(op));

        let sink = OpenDalSink::new(
            svc,
            request_builder,
            self.key_partitioner()?,
            batcher_settings,
        );

        Ok(VectorSink::from_event_streamsink(sink))
    }

    pub fn key_partitioner(&self) -> crate::Result<KeyPartitioner> {
        let prefix = self.prefix.clone().try_into()?;
        Ok(KeyPartitioner::new(prefix))
    }
}
