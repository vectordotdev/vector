use super::config::HdfsConfig;
use crate::codecs::SinkType;
use crate::event::LogEvent;
use crate::sinks::util::{request_builder::RequestBuilder, Compression};
use crate::{codecs::Encoder, sinks::util::request_builder::EncodeResult};
use crate::{
    codecs::EncodingConfigWithFraming, sinks::opendal_common::OpendalRequest,
    sinks::opendal_common::OpendalRequestBuilder,
};
use bytes::Bytes;
use codecs::encoding::Framer;
use codecs::{JsonSerializerConfig, NewlineDelimitedEncoderConfig};
use vector_core::partition::Partitioner;

fn default_config(encoding: EncodingConfigWithFraming) -> HdfsConfig {
    HdfsConfig {
        name_node: "default".to_string(),
        encoding,
        compression: Compression::gzip_default(),
        batch: Default::default(),
        acknowledgements: Default::default(),
        prefix: "tmp/".to_string(),
    }
}

#[test]
fn hdfs_generate_config() {
    crate::test_util::test_generate_config::<HdfsConfig>();
}

fn request_builder(sink_config: &HdfsConfig) -> OpendalRequestBuilder {
    let transformer = sink_config.encoding.transformer();
    let (framer, serializer) = sink_config
        .encoding
        .build(SinkType::MessageBased)
        .expect("encoding must build with success");
    let encoder = Encoder::<Framer>::new(framer, serializer);

    OpendalRequestBuilder {
        encoder: (transformer, encoder),
        compression: sink_config.compression,
    }
}

fn build_request(compression: Compression) -> OpendalRequest {
    let sink_config = HdfsConfig {
        compression,
        ..default_config(
            (
                Some(NewlineDelimitedEncoderConfig::new()),
                JsonSerializerConfig::default(),
            )
                .into(),
        )
    };
    let log = LogEvent::default().into();
    let key = sink_config
        .key_partitioner()
        .unwrap()
        .partition(&log)
        .expect("key wasn't provided");
    let request_builder = request_builder(&sink_config);
    let (metadata, metadata_request_builder, _events) =
        request_builder.split_input((key, vec![log]));
    let payload = EncodeResult::uncompressed(Bytes::new());
    let request_metadata = metadata_request_builder.build(&payload);

    request_builder.build_request(metadata, request_metadata, payload)
}

#[test]
fn hdfs_build_request() {
    let req = build_request(Compression::None);
    assert!(req.metadata.partition_key.starts_with("tmp/"));
    assert!(req.metadata.partition_key.ends_with(".log"));

    let req = build_request(Compression::None);
    assert!(req.metadata.partition_key.starts_with("tmp/"));
    assert!(req.metadata.partition_key.ends_with(".log"));

    let req = build_request(Compression::gzip_default());
    assert!(req.metadata.partition_key.starts_with("tmp/"));
    assert!(req.metadata.partition_key.ends_with(".log.gz"));

    let req = build_request(Compression::zlib_default());
    assert!(req.metadata.partition_key.starts_with("tmp/"));
    assert!(req.metadata.partition_key.ends_with(".log.zz"));
}
