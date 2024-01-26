use opendal::{layers::LoggingLayer, services::Webhdfs, Operator};
use tower::ServiceBuilder;
use vector_lib::codecs::{encoding::Framer, JsonSerializerConfig, NewlineDelimitedEncoderConfig};
use vector_lib::configurable::configurable_component;
use vector_lib::{
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
    #[serde(default)]
    #[configurable(metadata(docs::examples = "http://127.0.0.1:9870"))]
    pub endpoint: String,

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
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for WebHdfsConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            root: "/".to_string(),
            prefix: "%F/".to_string(),
            endpoint: "http://127.0.0.1:9870".to_string(),

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
#[typetag::serde(name = "webhdfs")]
impl SinkConfig for WebHdfsConfig {
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

impl WebHdfsConfig {
    pub fn build_operator(&self) -> crate::Result<Operator> {
        // Build OpenDal Operator
        let mut builder = Webhdfs::default();
        // Prefix logic will be handled by key_partitioner.
        builder.root(&self.root);
        builder.endpoint(&self.endpoint);

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
