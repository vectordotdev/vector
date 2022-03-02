use bytes::Bytes;
use chrono::Utc;
use vector_core::partition::Partitioner;

use super::config::AzureBlobSinkConfig;
use super::service::AzureBlobRequestOptions;

use crate::event::Event;
use crate::sinks::util::{
    encoding::StandardEncodings, request_builder::RequestBuilder, Compression,
};

fn default_config(e: StandardEncodings) -> AzureBlobSinkConfig {
    AzureBlobSinkConfig {
        connection_string: Default::default(),
        container_name: Default::default(),
        blob_prefix: Default::default(),
        blob_time_format: Default::default(),
        blob_append_uuid: Default::default(),
        encoding: e.into(),
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
    let log = Event::from("test message");
    let compression = Compression::None;
    let container_name = String::from("logs");
    let sink_config = AzureBlobSinkConfig {
        blob_prefix: Some("blob".into()),
        container_name: container_name.clone(),
        ..default_config(StandardEncodings::Text)
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
        encoding: StandardEncodings::Text.into(),
        compression,
    };

    let (metadata, _events) = request_options.split_input((key, vec![log]));
    let request = request_options.build_request(metadata, Bytes::new());

    assert_eq!(request.metadata.partition_key, "blob.log".to_string());
    assert_eq!(request.content_encoding, None);
    assert_eq!(request.content_type, "text/plain");
}

#[test]
fn azure_blob_build_request_with_compression() {
    let log = Event::from("test message");
    let compression = Compression::gzip_default();
    let container_name = String::from("logs");
    let sink_config = AzureBlobSinkConfig {
        blob_prefix: Some("blob".into()),
        container_name: container_name.clone(),
        ..default_config(StandardEncodings::Text)
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
        encoding: StandardEncodings::Text.into(),
        compression,
    };

    let (metadata, _events) = request_options.split_input((key, vec![log]));
    let request = request_options.build_request(metadata, Bytes::new());

    assert_eq!(request.metadata.partition_key, "blob.log.gz".to_string());
    assert_eq!(request.content_encoding, Some("gzip"));
    assert_eq!(request.content_type, "application/gzip");
}

#[test]
fn azure_blob_build_request_with_time_format() {
    let log = Event::from("test message");
    let compression = Compression::None;
    let container_name = String::from("logs");
    let sink_config = AzureBlobSinkConfig {
        blob_prefix: Some("blob".into()),
        container_name: container_name.clone(),
        ..default_config(StandardEncodings::Text)
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
        encoding: StandardEncodings::Text.into(),
        compression,
    };

    let (metadata, _events) = request_options.split_input((key, vec![log]));
    let request = request_options.build_request(metadata, Bytes::new());

    assert_eq!(
        request.metadata.partition_key,
        format!("blob{}.log", Utc::now().format("%F"))
    );
    assert_eq!(request.content_encoding, None);
    assert_eq!(request.content_type, "text/plain");
}

#[test]
fn azure_blob_build_request_with_uuid() {
    let log = Event::from("test message");
    let compression = Compression::None;
    let container_name = String::from("logs");
    let sink_config = AzureBlobSinkConfig {
        blob_prefix: Some("blob".into()),
        container_name: container_name.clone(),
        ..default_config(StandardEncodings::Text)
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
        encoding: StandardEncodings::Text.into(),
        compression,
    };

    let (metadata, _events) = request_options.split_input((key, vec![log]));
    let request = request_options.build_request(metadata, Bytes::new());

    assert_ne!(request.metadata.partition_key, "blob.log".to_string());
    assert_eq!(request.content_encoding, None);
    assert_eq!(request.content_type, "text/plain");
}
