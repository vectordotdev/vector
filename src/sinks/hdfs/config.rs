use crate::codecs::Encoder;
use crate::codecs::EncodingConfigWithFraming;
use crate::codecs::SinkType;
use crate::config::{GenerateConfig, SinkConfig, SinkContext};
use crate::sinks::opendal_common::*;
use crate::sinks::util::partitioner::KeyPartitioner;
use crate::sinks::util::BatchConfig;
use crate::sinks::util::BulkSizeBasedDefaultBatchSettings;
use crate::sinks::util::Compression;
use crate::sinks::Healthcheck;
use codecs::encoding::Framer;
use codecs::JsonSerializerConfig;
use codecs::NewlineDelimitedEncoderConfig;
use opendal::layers::LoggingLayer;
use opendal::services::Hdfs;
use opendal::Operator;
use vector_config::configurable_component;
use vector_core::{
    config::{AcknowledgementsConfig, Input},
    sink::VectorSink,
};

/// Configuration for the `hdfs` sink.
#[configurable_component(sink("hdfs"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]

pub struct HdfsConfig {
    /// A prefix to apply to all keys.
    ///
    /// Prefixes are useful for partitioning objects, such as by creating an blob key that
    /// stores blobs under a particular "directory". If using a prefix for this purpose, it must end
    /// in `/` to act as a directory path. A trailing `/` is **not** automatically added.
    #[serde(default)]
    #[configurable(metadata(docs::templateable))]
    pub prefix: String,

    /// An HDFS cluster consists of a single NameNode, a master server that manages the file system namespace and regulates access to files by clients.
    ///
    /// For example:
    ///
    /// - `default`: visiting local fs.
    /// - `http://172.16.80.2:8090` visiting name node at `172.16.80.2`
    ///
    /// For more information: [HDFS Architecture](https://hadoop.apache.org/docs/r3.3.4/hadoop-project-dist/hadoop-hdfs/HdfsDesign.html#NameNode_and_DataNodes)
    #[configurable(derived)]
    #[serde(default)]
    pub name_node: String,

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

impl GenerateConfig for HdfsConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            prefix: "/tmp".to_string(),
            name_node: "default".to_string(),

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
impl SinkConfig for HdfsConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        // Build OpenDAL Operator
        let mut builder = Hdfs::default();
        // Prefix logic will be handled by key_partitioner.
        builder.root("/");
        builder.name_node(&self.name_node);

        let op = Operator::create(builder)?
            .layer(LoggingLayer::default())
            .finish();

        let check_op = op.clone();
        let healthcheck = Box::pin(async move { Ok(check_op.check().await?) });

        let sink = self.build_processor(op)?;
        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl HdfsConfig {
    pub fn build_processor(&self, op: Operator) -> crate::Result<VectorSink> {
        // Configure our partitioning/batching.
        let batcher_settings = self.batch.into_batcher_settings()?;

        let transformer = self.encoding.transformer();
        let (framer, serializer) = self.encoding.build(SinkType::MessageBased)?;
        let encoder = Encoder::<Framer>::new(framer, serializer);

        let request_builder = OpendalRequestBuilder {
            encoder: (transformer, encoder),
            compression: self.compression,
        };

        let sink = OpendalSink::new(
            op,
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
