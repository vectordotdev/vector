use bytes::Bytes;
use chrono::Utc;
use vector_lib::codecs::{
    encoding::{Framer, FramingConfig},
    NewlineDelimitedEncoder, TextSerializerConfig,
};
use vector_lib::request_metadata::GroupedCountByteSize;
use vector_lib::{partition::Partitioner, EstimatedJsonEncodedSizeOf};

use super::config::AzureBlobSinkConfig;
use super::request_builder::AzureBlobRequestOptions;
use crate::codecs::EncodingConfigWithFraming;
use crate::event::{Event, LogEvent};
use crate::sinks::util::{request_builder::RequestBuilder, Compression};
use crate::{codecs::Encoder, sinks::util::request_builder::EncodeResult};

fn default_config(encoding: EncodingConfigWithFraming) -> AzureBlobSinkConfig {
    AzureBlobSinkConfig {
        connection_string: Default::default(),
        storage_account: Default::default(),
        container_name: Default::default(),
        endpoint: Default::default(),
        blob_prefix: Default::default(),
        blob_time_format: Default::default(),
        blob_append_uuid: Default::default(),
        encoding,
        compression: Compression::gzip_default(),
        batch: Default::default(),
        request: Default::default(),
        acknowledgements: Default::default(),
    }
}

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<AzureBlobSinkConfig>();
}

#[test]
fn azure_blob_build_request_without_compression() {
    let log = Event::Log(LogEvent::from("test message"));
    let compression = Compression::None;
    let container_name = String::from("logs");
    let sink_config = AzureBlobSinkConfig {
        blob_prefix: "blob".try_into().unwrap(),
        container_name: container_name.clone(),
        ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
    };
    let blob_time_format = String::from("");
    let blob_append_uuid = false;

    let key = sink_config
        .key_partitioner()
        .unwrap()
        .partition(&log)
        .expect("key wasn't provided");

    let request_options = AzureBlobRequestOptions {
        container_name,
        blob_time_format,
        blob_append_uuid,
        encoder: (
            Default::default(),
            Encoder::<Framer>::new(
                NewlineDelimitedEncoder::default().into(),
                TextSerializerConfig::default().build().into(),
            ),
        ),
        compression,
    };

    let mut byte_size = GroupedCountByteSize::new_untagged();
    byte_size.add_event(&log, log.estimated_json_encoded_size_of());

    let (metadata, request_metadata_builder, _events) =
        request_options.split_input((key, vec![log]));

    let payload = EncodeResult::uncompressed(Bytes::new(), byte_size);
    let request_metadata = request_metadata_builder.build(&payload);
    let request = request_options.build_request(metadata, request_metadata, payload);

    assert_eq!(request.metadata.partition_key, "blob.log".to_string());
    assert_eq!(request.content_encoding, None);
    assert_eq!(request.content_type, "text/plain");
}

#[test]
fn azure_blob_build_request_with_compression() {
    let log = Event::Log(LogEvent::from("test message"));
    let compression = Compression::gzip_default();
    let container_name = String::from("logs");
    let sink_config = AzureBlobSinkConfig {
        blob_prefix: "blob".try_into().unwrap(),
        container_name: container_name.clone(),
        ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
    };
    let blob_time_format = String::from("");
    let blob_append_uuid = false;

    let key = sink_config
        .key_partitioner()
        .unwrap()
        .partition(&log)
        .expect("key wasn't provided");

    let request_options = AzureBlobRequestOptions {
        container_name,
        blob_time_format,
        blob_append_uuid,
        encoder: (
            Default::default(),
            Encoder::<Framer>::new(
                NewlineDelimitedEncoder::default().into(),
                TextSerializerConfig::default().build().into(),
            ),
        ),
        compression,
    };

    let mut byte_size = GroupedCountByteSize::new_untagged();
    byte_size.add_event(&log, log.estimated_json_encoded_size_of());

    let (metadata, request_metadata_builder, _events) =
        request_options.split_input((key, vec![log]));

    let payload = EncodeResult::uncompressed(Bytes::new(), byte_size);
    let request_metadata = request_metadata_builder.build(&payload);
    let request = request_options.build_request(metadata, request_metadata, payload);

    assert_eq!(request.metadata.partition_key, "blob.log.gz".to_string());
    assert_eq!(request.content_encoding, Some("gzip"));
    assert_eq!(request.content_type, "text/plain");
}

#[test]
fn azure_blob_build_request_with_time_format() {
    let log = Event::Log(LogEvent::from("test message"));
    let compression = Compression::None;
    let container_name = String::from("logs");
    let sink_config = AzureBlobSinkConfig {
        blob_prefix: "blob".try_into().unwrap(),
        container_name: container_name.clone(),
        ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
    };
    let blob_time_format = String::from("%F");
    let blob_append_uuid = false;

    let key = sink_config
        .key_partitioner()
        .unwrap()
        .partition(&log)
        .expect("key wasn't provided");

    let request_options = AzureBlobRequestOptions {
        container_name,
        blob_time_format,
        blob_append_uuid,
        encoder: (
            Default::default(),
            Encoder::<Framer>::new(
                NewlineDelimitedEncoder::default().into(),
                TextSerializerConfig::default().build().into(),
            ),
        ),
        compression,
    };

    let mut byte_size = GroupedCountByteSize::new_untagged();
    byte_size.add_event(&log, log.estimated_json_encoded_size_of());

    let (metadata, request_metadata_builder, _events) =
        request_options.split_input((key, vec![log]));

    let payload = EncodeResult::uncompressed(Bytes::new(), byte_size);
    let request_metadata = request_metadata_builder.build(&payload);
    let request = request_options.build_request(metadata, request_metadata, payload);

    assert_eq!(
        request.metadata.partition_key,
        format!("blob{}.log", Utc::now().format("%F"))
    );
    assert_eq!(request.content_encoding, None);
    assert_eq!(request.content_type, "text/plain");
}

#[test]
fn azure_blob_build_request_with_uuid() {
    let log = Event::Log(LogEvent::from("test message"));
    let compression = Compression::None;
    let container_name = String::from("logs");
    let sink_config = AzureBlobSinkConfig {
        blob_prefix: "blob".try_into().unwrap(),
        container_name: container_name.clone(),
        ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
    };
    let blob_time_format = String::from("");
    let blob_append_uuid = true;

    let key = sink_config
        .key_partitioner()
        .unwrap()
        .partition(&log)
        .expect("key wasn't provided");

    let request_options = AzureBlobRequestOptions {
        container_name,
        blob_time_format,
        blob_append_uuid,
        encoder: (
            Default::default(),
            Encoder::<Framer>::new(
                NewlineDelimitedEncoder::default().into(),
                TextSerializerConfig::default().build().into(),
            ),
        ),
        compression,
    };

    let mut byte_size = GroupedCountByteSize::new_untagged();
    byte_size.add_event(&log, log.estimated_json_encoded_size_of());

    let (metadata, request_metadata_builder, _events) =
        request_options.split_input((key, vec![log]));

    let payload = EncodeResult::uncompressed(Bytes::new(), byte_size);
    let request_metadata = request_metadata_builder.build(&payload);
    let request = request_options.build_request(metadata, request_metadata, payload);

    assert_ne!(request.metadata.partition_key, "blob.log".to_string());
    assert_eq!(request.content_encoding, None);
    assert_eq!(request.content_type, "text/plain");
}
