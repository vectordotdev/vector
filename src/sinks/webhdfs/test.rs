use bytes::Bytes;
use vector_lib::codecs::{encoding::Framer, JsonSerializerConfig, NewlineDelimitedEncoderConfig};
use vector_lib::partition::Partitioner;
use vector_lib::request_metadata::GroupedCountByteSize;

use super::config::WebHdfsConfig;
use crate::{
    codecs::{Encoder, EncodingConfigWithFraming, SinkType},
    event::LogEvent,
    sinks::{
        opendal_common::{OpenDalRequest, OpenDalRequestBuilder},
        util::{
            request_builder::{EncodeResult, RequestBuilder},
            Compression,
        },
    },
};

fn default_config(encoding: EncodingConfigWithFraming) -> WebHdfsConfig {
    WebHdfsConfig {
        root: "/tmp/".to_string(),
        prefix: "%F/".to_string(),
        endpoint: "http://127.0.0.1:9870".to_string(),
        encoding,
        compression: Compression::gzip_default(),
        batch: Default::default(),
        acknowledgements: Default::default(),
    }
}

#[test]
fn webhdfs_generate_config() {
    crate::test_util::test_generate_config::<WebHdfsConfig>();
}

fn request_builder(sink_config: &WebHdfsConfig) -> OpenDalRequestBuilder {
    let transformer = sink_config.encoding.transformer();
    let (framer, serializer) = sink_config
        .encoding
        .build(SinkType::MessageBased)
        .expect("encoding must build with success");
    let encoder = Encoder::<Framer>::new(framer, serializer);

    OpenDalRequestBuilder {
        encoder: (transformer, encoder),
        compression: sink_config.compression,
    }
}

fn build_request(compression: Compression) -> OpenDalRequest {
    let sink_config = WebHdfsConfig {
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
    let byte_size = GroupedCountByteSize::new_untagged();
    let payload = EncodeResult::uncompressed(Bytes::new(), byte_size);
    let request_metadata = metadata_request_builder.build(&payload);

    request_builder.build_request(metadata, request_metadata, payload)
}

#[test]
fn webhdfs_build_request() {
    let req = build_request(Compression::None);
    assert!(req.metadata.partition_key.ends_with(".log"));

    let req = build_request(Compression::None);
    assert!(req.metadata.partition_key.ends_with(".log"));

    let req = build_request(Compression::gzip_default());
    assert!(req.metadata.partition_key.ends_with(".log.gz"));

    let req = build_request(Compression::zlib_default());
    assert!(req.metadata.partition_key.ends_with(".log.zz"));
}
