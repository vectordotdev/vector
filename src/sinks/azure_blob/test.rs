use bytes::Bytes;
use chrono::Utc;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::{
        GelfSerializerConfig, JsonSerializerConfig, NativeSerializerConfig,
        NewlineDelimitedEncoder, NewlineDelimitedEncoderConfig, TextSerializerConfig,
        encoding::{
            CharacterDelimitedEncoder, CharacterDelimitedEncoderConfig, Framer, FramingConfig,
        },
    },
    partition::Partitioner,
    request_metadata::GroupedCountByteSize,
};

use super::{
    config::{AzureBlobSinkConfig, AzureBlobType},
    request_builder::AzureBlobRequestOptions,
};
use crate::{
    codecs::{Encoder, EncodingConfigWithFraming},
    event::{Event, LogEvent},
    sinks::azure_common::config::{AzureAuthentication, SpecificAzureCredential},
    sinks::prelude::*,
    sinks::util::{
        BatchConfig, Compression,
        buffer::compression::CompressionLevel,
        encoding::Encoder as _,
        request_builder::{EncodeResult, RequestBuilder},
    },
};

fn default_config(encoding: EncodingConfigWithFraming) -> AzureBlobSinkConfig {
    AzureBlobSinkConfig {
        auth: Default::default(),
        connection_string: Default::default(),
        account_name: Default::default(),
        blob_endpoint: Default::default(),
        container_name: Default::default(),
        blob_prefix: Default::default(),
        blob_time_format: Default::default(),
        blob_append_uuid: Default::default(),
        blob_type: Default::default(),
        encoding,
        compression: Compression::gzip_default(),
        tags: Default::default(),
        metadata: Default::default(),
        batch: Default::default(),
        request: Default::default(),
        acknowledgements: Default::default(),
        tls: Default::default(),
        confinement: Default::default(),
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
        blob_type: AzureBlobType::Block,
        encoder: (
            Default::default(),
            Encoder::<Framer>::new(
                NewlineDelimitedEncoder::default().into(),
                TextSerializerConfig::default().build().into(),
            ),
        ),
        compression,
        tags: None,
        metadata: None,
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
        blob_type: AzureBlobType::Block,
        encoder: (
            Default::default(),
            Encoder::<Framer>::new(
                NewlineDelimitedEncoder::default().into(),
                TextSerializerConfig::default().build().into(),
            ),
        ),
        compression,
        tags: None,
        metadata: None,
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
        blob_type: AzureBlobType::Block,
        encoder: (
            Default::default(),
            Encoder::<Framer>::new(
                NewlineDelimitedEncoder::default().into(),
                TextSerializerConfig::default().build().into(),
            ),
        ),
        compression,
        tags: None,
        metadata: None,
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
        blob_type: AzureBlobType::Block,
        encoder: (
            Default::default(),
            Encoder::<Framer>::new(
                NewlineDelimitedEncoder::default().into(),
                TextSerializerConfig::default().build().into(),
            ),
        ),
        compression,
        tags: None,
        metadata: None,
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

#[tokio::test]
async fn azure_blob_build_config_with_null_auth() {
    let config: Result<AzureBlobSinkConfig, _> =
        serde_yaml::from_str::<AzureBlobSinkConfig>(indoc::indoc! {r#"
            connection_string: "AccountName=mylogstorage"
            container_name: my-logs
            encoding:
              codec: json
            auth: {}
        "#});

    match config {
        Ok(_) => panic!("Config parsing should have failed due to invalid auth config"),
        Err(e) => {
            let err_str = e.to_string();
            assert!(
                err_str.contains("data did not match any variant of untagged enum"),
                "Config parsing did not complain about invalid auth config: {}",
                err_str
            );
        }
    }
}

#[tokio::test]
async fn azure_blob_build_config_with_client_id_and_secret() {
    let config: AzureBlobSinkConfig =
        serde_yaml::from_str::<AzureBlobSinkConfig>(indoc::indoc! {r#"
            connection_string: "AccountName=mylogstorage"
            container_name: my-logs
            encoding:
              codec: json
            auth:
              azure_credential_kind: client_secret_credential
              azure_tenant_id: "00000000-0000-0000-0000-000000000000"
              azure_client_id: mock-client-id
              azure_client_secret: mock-client-secret
        "#})
        .unwrap_or_else(|error| panic!("Config parsing failed: {error:?}"));

    assert!(&config.auth.is_some());

    match &config.auth.clone().unwrap() {
        AzureAuthentication::Specific(SpecificAzureCredential::ClientSecretCredential {
            azure_tenant_id,
            azure_client_id,
            azure_client_secret,
        }) => {
            assert_eq!(azure_tenant_id, "00000000-0000-0000-0000-000000000000");
            assert_eq!(azure_client_id, "mock-client-id");
            let secret: String = azure_client_secret.inner().into();
            assert_eq!(secret, "mock-client-secret");
        }
        _ => panic!("Expected Specific(ClientSecretCredential) variant"),
    }

    let cx = SinkContext::default();
    let _sink = config
        .build(cx)
        .await
        .unwrap_or_else(|error| panic!("Failed to build sink: {error:?}"));
}

#[tokio::test]
async fn azure_blob_build_config_with_client_certificate() {
    let config: AzureBlobSinkConfig =
        serde_yaml::from_str::<AzureBlobSinkConfig>(indoc::indoc! {r#"
            connection_string: "AccountName=mylogstorage"
            container_name: my-logs
            encoding:
              codec: json
            auth:
              azure_credential_kind: client_certificate_credential
              azure_tenant_id: "00000000-0000-0000-0000-000000000000"
              azure_client_id: mock-client-id
              certificate_path: tests/data/ClientCertificateAuth.pfx
              certificate_password: MockPassword123
        "#})
        .unwrap_or_else(|error| panic!("Config parsing failed: {error:?}"));

    assert!(&config.auth.is_some());

    match &config.auth.clone().unwrap() {
        AzureAuthentication::Specific(SpecificAzureCredential::ClientCertificateCredential {
            ..
        }) => {
            // Expected variant
        }
        _ => panic!("Expected Specific(ClientCertificateCredential) variant"),
    }

    let cx = SinkContext::default();
    let _sink = config
        .build(cx)
        .await
        .unwrap_or_else(|error| panic!("Failed to build sink: {error:?}"));
}

#[tokio::test]
async fn azure_blob_build_config_with_account_name() {
    let config: AzureBlobSinkConfig =
        serde_yaml::from_str::<AzureBlobSinkConfig>(indoc::indoc! {r#"
            account_name: mylogstorage
            container_name: my-logs
            encoding:
              codec: json
            auth:
              azure_credential_kind: client_secret_credential
              azure_tenant_id: "00000000-0000-0000-0000-000000000000"
              azure_client_id: mock-client-id
              azure_client_secret: mock-client-secret
        "#})
        .unwrap_or_else(|error| panic!("Config parsing failed: {error:?}"));

    let cx = SinkContext::default();
    let _ = config
        .build(cx)
        .await
        .unwrap_or_else(|error| panic!("Failed to build sink: {error:?}"));
}

#[tokio::test]
async fn azure_blob_build_config_with_account_name_with_no_auth() {
    let config: AzureBlobSinkConfig =
        serde_yaml::from_str::<AzureBlobSinkConfig>(indoc::indoc! {r#"
            account_name: mylogstorage
            container_name: my-logs
            encoding:
              codec: json
        "#})
        .unwrap_or_else(|error| panic!("Config parsing failed: {error:?}"));

    let cx = SinkContext::default();
    let sink = config.build(cx).await;
    match sink {
        Ok(_) => panic!("Config build should have errored due to missing `auth`"),
        Err(e) => {
            let err_str = e.to_string();
            assert!(
                err_str.contains("`auth` configuration must be provided"),
                "Config build did not complain about missing `auth`: {}",
                err_str
            );
        }
    }
}

#[tokio::test]
async fn azure_blob_build_config_with_blob_endpoint() {
    let config: AzureBlobSinkConfig =
        serde_yaml::from_str::<AzureBlobSinkConfig>(indoc::indoc! {r#"
            blob_endpoint: "https://localhost:10000/devstoreaccount1"
            container_name: my-logs
            encoding:
              codec: json
            auth:
              azure_credential_kind: client_secret_credential
              azure_tenant_id: "00000000-0000-0000-0000-000000000000"
              azure_client_id: mock-client-id
              azure_client_secret: mock-client-secret
        "#})
        .unwrap_or_else(|error| panic!("Config parsing failed: {error:?}"));

    let cx = SinkContext::default();
    let _ = config
        .build(cx)
        .await
        .unwrap_or_else(|error| panic!("Failed to build sink: {error:?}"));
}

#[tokio::test]
async fn azure_blob_build_config_with_blob_endpoint_with_no_auth() {
    let config: AzureBlobSinkConfig =
        serde_yaml::from_str::<AzureBlobSinkConfig>(indoc::indoc! {r#"
            blob_endpoint: "https://localhost:10000/devstoreaccount1"
            container_name: my-logs
            encoding:
              codec: json
        "#})
        .unwrap_or_else(|error| panic!("Config parsing failed: {error:?}"));

    let cx = SinkContext::default();
    let sink = config.build(cx).await;
    match sink {
        Ok(_) => panic!("Config build should have errored due to missing `auth`"),
        Err(e) => {
            let err_str = e.to_string();
            assert!(
                err_str.contains("`auth` configuration must be provided"),
                "Config build did not complain about missing `auth`: {}",
                err_str
            );
        }
    }
}

#[tokio::test]
async fn azure_blob_build_config_with_conflicting_connection_string_and_account_name() {
    let config: AzureBlobSinkConfig =
        serde_yaml::from_str::<AzureBlobSinkConfig>(indoc::indoc! {r#"
            connection_string: "AccountName=mylogstorage"
            account_name: mylogstorage
            container_name: my-logs
            encoding:
              codec: json
        "#})
        .unwrap_or_else(|error| panic!("Config parsing failed: {error:?}"));

    let cx = SinkContext::default();
    let sink = config.build(cx).await;
    match sink {
        Ok(_) => panic!(
            "Config build should have errored due to conflicting connection_string and account_name"
        ),
        Err(e) => {
            let err_str = e.to_string();
            assert!(
                err_str.contains("`connection_string` and `account_name`"),
                "Config build did not complain about conflicting connection_string and account_name: {}",
                err_str
            );
        }
    }
}

#[tokio::test]
async fn azure_blob_build_config_with_conflicting_connection_string_and_client_id_and_secret() {
    let config: AzureBlobSinkConfig =
        serde_yaml::from_str::<AzureBlobSinkConfig>(indoc::indoc! {r#"
            connection_string: "AccountName=mylogstorage;AccountKey=mockkey"
            container_name: my-logs
            encoding:
              codec: json
            auth:
              azure_credential_kind: client_secret_credential
              azure_tenant_id: "00000000-0000-0000-0000-000000000000"
              azure_client_id: mock-client-id
              azure_client_secret: mock-client-secret
        "#})
        .unwrap_or_else(|error| panic!("Config parsing failed: {error:?}"));

    assert!(&config.auth.is_some());

    let cx = SinkContext::default();
    let sink = config.build(cx).await;
    match sink {
        Ok(_) => {
            panic!("Config build should have errored due to conflicting Shared Key and Client ID")
        }
        Err(e) => {
            let err_str = e.to_string();
            assert!(
                err_str
                    .contains("Cannot use both Shared Key and another Azure Authentication method"),
                "Config build did not complain about conflicting Shared Key and Client ID: {}",
                err_str
            );
        }
    }
}

#[tokio::test]
async fn azure_blob_build_config_with_custom_ca_certificate() {
    let config: AzureBlobSinkConfig =
        serde_yaml::from_str::<AzureBlobSinkConfig>(indoc::indoc! {r#"
            account_name: mylogstorage
            container_name: my-logs
            encoding:
              codec: json
            tls:
              ca_file: tests/data/ca/certs/ca.cert.pem
            auth:
              azure_credential_kind: client_secret_credential
              azure_tenant_id: "00000000-0000-0000-0000-000000000000"
              azure_client_id: mock-client-id
              azure_client_secret: mock-client-secret
        "#})
        .unwrap_or_else(|error| panic!("Config parsing failed: {error:?}"));

    let cx = SinkContext::default();
    let _ = config
        .build(cx)
        .await
        .unwrap_or_else(|error| panic!("Failed to build sink: {error:?}"));
}

#[test]
fn azure_blob_build_request_with_blob_tags() {
    use std::collections::BTreeMap;

    let log = Event::Log(LogEvent::from("test message"));
    let compression = Compression::None;
    let container_name = String::from("logs");
    let sink_config = AzureBlobSinkConfig {
        blob_prefix: "blob".try_into().unwrap(),
        container_name: container_name.clone(),
        ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
    };

    let mut tags = BTreeMap::new();
    tags.insert("Project".to_string(), "Blue".to_string());
    tags.insert("Owner".to_string(), "ops team".to_string());

    let key = sink_config
        .key_partitioner()
        .unwrap()
        .partition(&log)
        .expect("key wasn't provided");

    let request_options = AzureBlobRequestOptions {
        container_name,
        blob_time_format: String::from(""),
        blob_append_uuid: false,
        blob_type: AzureBlobType::Block,
        encoder: (
            Default::default(),
            Encoder::<Framer>::new(
                NewlineDelimitedEncoder::default().into(),
                TextSerializerConfig::default().build().into(),
            ),
        ),
        compression,
        tags: Some(tags),
        metadata: None,
    };

    let mut byte_size = GroupedCountByteSize::new_untagged();
    byte_size.add_event(&log, log.estimated_json_encoded_size_of());

    let (metadata, request_metadata_builder, _events) =
        request_options.split_input((key, vec![log]));

    let payload = EncodeResult::uncompressed(Bytes::new(), byte_size);
    let request_metadata = request_metadata_builder.build(&payload);
    let request = request_options.build_request(metadata, request_metadata, payload);

    // BTreeMap ordering: "Owner" < "Project"; space is percent-encoded as %20.
    assert_eq!(
        request.tags,
        Some("Owner=ops%20team&Project=Blue".to_string())
    );
    assert_eq!(request.blob_metadata, None);
}

#[test]
fn azure_blob_build_request_with_blob_metadata() {
    use std::collections::HashMap;

    let log = Event::Log(LogEvent::from("test message"));
    let compression = Compression::None;
    let container_name = String::from("logs");
    let sink_config = AzureBlobSinkConfig {
        blob_prefix: "blob".try_into().unwrap(),
        container_name: container_name.clone(),
        ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
    };

    let mut metadata = HashMap::new();
    metadata.insert("source".to_string(), "vector".to_string());

    let key = sink_config
        .key_partitioner()
        .unwrap()
        .partition(&log)
        .expect("key wasn't provided");

    let request_options = AzureBlobRequestOptions {
        container_name,
        blob_time_format: String::from(""),
        blob_append_uuid: false,
        blob_type: AzureBlobType::Block,
        encoder: (
            Default::default(),
            Encoder::<Framer>::new(
                NewlineDelimitedEncoder::default().into(),
                TextSerializerConfig::default().build().into(),
            ),
        ),
        compression,
        tags: None,
        metadata: Some(metadata.clone()),
    };

    let mut byte_size = GroupedCountByteSize::new_untagged();
    byte_size.add_event(&log, log.estimated_json_encoded_size_of());

    let (azure_metadata, request_metadata_builder, _events) =
        request_options.split_input((key, vec![log]));

    let payload = EncodeResult::uncompressed(Bytes::new(), byte_size);
    let request_metadata = request_metadata_builder.build(&payload);
    let request = request_options.build_request(azure_metadata, request_metadata, payload);

    assert_eq!(request.tags, None);
    assert_eq!(request.blob_metadata, Some(metadata));
}

#[test]
fn azure_blob_build_request_with_empty_blob_tags_and_metadata() {
    use std::collections::{BTreeMap, HashMap};

    let log = Event::Log(LogEvent::from("test message"));
    let compression = Compression::None;
    let container_name = String::from("logs");
    let sink_config = AzureBlobSinkConfig {
        blob_prefix: "blob".try_into().unwrap(),
        container_name: container_name.clone(),
        ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
    };

    let key = sink_config
        .key_partitioner()
        .unwrap()
        .partition(&log)
        .expect("key wasn't provided");

    // Empty maps must collapse to `None` so we do not emit empty headers.
    let request_options = AzureBlobRequestOptions {
        container_name,
        blob_time_format: String::from(""),
        blob_append_uuid: false,
        blob_type: AzureBlobType::Block,
        encoder: (
            Default::default(),
            Encoder::<Framer>::new(
                NewlineDelimitedEncoder::default().into(),
                TextSerializerConfig::default().build().into(),
            ),
        ),
        compression,
        tags: Some(BTreeMap::new()),
        metadata: Some(HashMap::new()),
    };

    let mut byte_size = GroupedCountByteSize::new_untagged();
    byte_size.add_event(&log, log.estimated_json_encoded_size_of());

    let (metadata, request_metadata_builder, _events) =
        request_options.split_input((key, vec![log]));

    let payload = EncodeResult::uncompressed(Bytes::new(), byte_size);
    let request_metadata = request_metadata_builder.build(&payload);
    let request = request_options.build_request(metadata, request_metadata, payload);

    assert_eq!(request.tags, None);
    assert_eq!(request.blob_metadata, None);
}

#[test]
fn azure_blob_build_request_append_blob_defaults() {
    let log = Event::Log(LogEvent::from("test message"));
    let container_name = String::from("logs");
    let sink_config = AzureBlobSinkConfig {
        blob_prefix: "blob/".try_into().unwrap(),
        container_name: container_name.clone(),
        ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
    };

    let key = sink_config
        .key_partitioner()
        .unwrap()
        .partition(&log)
        .expect("key wasn't provided");

    let request_options = AzureBlobRequestOptions {
        container_name,
        blob_time_format: "%Y-%m-%dT%H".to_string(),
        blob_append_uuid: false,
        blob_type: AzureBlobType::Append,
        encoder: (
            Default::default(),
            Encoder::<Framer>::new(
                NewlineDelimitedEncoder::default().into(),
                TextSerializerConfig::default().build().into(),
            ),
        ),
        compression: Compression::None,
        tags: None,
        metadata: None,
    };

    let mut byte_size = GroupedCountByteSize::new_untagged();
    byte_size.add_event(&log, log.estimated_json_encoded_size_of());

    let (metadata, request_metadata_builder, _events) =
        request_options.split_input((key, vec![log]));

    let payload = EncodeResult::uncompressed(Bytes::new(), byte_size);
    let request_metadata = request_metadata_builder.build(&payload);

    // Capture the hour window around `build_request`, which formats the key with its own
    // `Utc::now()`. Comparing against a single later `Utc::now()` would flake if the test
    // crossed an hour boundary between the two calls, so accept either side of the boundary.
    let before = Utc::now().format("%Y-%m-%dT%H").to_string();
    let request = request_options.build_request(metadata, request_metadata, payload);
    let after = Utc::now().format("%Y-%m-%dT%H").to_string();

    let key = &request.metadata.partition_key;
    assert!(
        *key == format!("blob/{before}.log") || *key == format!("blob/{after}.log"),
        "partition_key {key:?} did not match the expected hourly key for {before} or {after}"
    );
    assert_eq!(request.blob_type, AzureBlobType::Append);
}

/// An append blob accumulates batches, so JSON without explicit framing must be newline-delimited
/// rather than one array per batch — concatenated arrays are not parseable JSON.
#[test]
fn azure_blob_append_blob_json_defaults_to_newline_framing() {
    let config = AzureBlobSinkConfig {
        blob_type: AzureBlobType::Append,
        ..default_config((None::<FramingConfig>, JsonSerializerConfig::default()).into())
    };

    let encoder = config
        .build_encoder()
        .expect("append mode with the JSON codec and no framing must build");

    assert!(
        encoder.batch_prefix().is_empty() && encoder.batch_suffix(false) == b"\n",
        "append batches must be newline-delimited, not enclosed in a JSON array"
    );
    assert!(matches!(encoder.framer(), Framer::NewlineDelimited(_)));
}

/// A block blob holds exactly one batch, so the JSON array framing stays correct there.
#[test]
fn azure_blob_block_blob_json_keeps_array_framing() {
    let config = AzureBlobSinkConfig {
        blob_type: AzureBlobType::Block,
        ..default_config((None::<FramingConfig>, JsonSerializerConfig::default()).into())
    };

    let encoder = config.build_encoder().expect("block mode must build");

    assert_eq!(
        encoder.batch_prefix(),
        b"[",
        "block blobs must keep emitting one JSON array per blob"
    );
}

/// Why the append default is newline-delimited: Azure appends payloads with nothing between them, so
/// a blob only reads back as a record stream if the framing terminates the batch's last record.
#[test]
fn record_terminating_framing_survives_payload_concatenation() {
    fn payload(framer: Framer) -> String {
        let encoder = (
            Transformer::default(),
            Encoder::<Framer>::new(framer, TextSerializerConfig::default().build().into()),
        );
        let mut out = Vec::new();
        encoder
            .encode_input(
                vec![
                    Event::Log(LogEvent::from("a")),
                    Event::Log(LogEvent::from("b")),
                ],
                &mut out,
            )
            .expect("encoding must succeed");

        String::from_utf8(out).expect("payload must be utf8")
    }

    let newline = payload(NewlineDelimitedEncoder::default().into());
    assert_eq!(
        newline, "a\nb\n",
        "newline framing terminates the last record"
    );
    assert_eq!(
        format!("{newline}{newline}"),
        "a\nb\na\nb\n",
        "so concatenated payloads keep exactly one record per line"
    );

    let semicolon = payload(CharacterDelimitedEncoder::new(b';').into());
    assert_eq!(
        semicolon, "a;b",
        "character framing separates records but leaves the last one unterminated"
    );
    assert_eq!(
        format!("{semicolon}{semicolon}"),
        "a;ba;b",
        "so concatenating fuses `b` and `a` into a single record at the seam"
    );
}

/// Only the defaults are `blob_type`-aware. An explicit `framing` is passed through untouched, even
/// one whose seam behavior is the concatenation above — that is the user's call to make.
#[test]
fn azure_blob_append_blob_honors_explicit_framing() {
    let config = AzureBlobSinkConfig {
        blob_type: AzureBlobType::Append,
        ..default_config(
            (
                Some(CharacterDelimitedEncoderConfig::new(b';')),
                TextSerializerConfig::default(),
            )
                .into(),
        )
    };

    let encoder = config
        .build_encoder()
        .expect("explicitly configured framing must be honored");

    assert!(
        matches!(
            encoder.framer(),
            Framer::CharacterDelimited(CharacterDelimitedEncoder { delimiter: b';' })
        ),
        "append mode must not override an explicit framing choice"
    );
}

/// Block blobs hold exactly one batch, so nothing concatenates and every framing stays valid.
#[test]
fn azure_blob_block_blob_allows_non_terminating_framing() {
    let config = AzureBlobSinkConfig {
        blob_type: AzureBlobType::Block,
        ..default_config(
            (
                Some(CharacterDelimitedEncoderConfig::new(b';')),
                TextSerializerConfig::default(),
            )
                .into(),
        )
    };

    let encoder = config
        .build_encoder()
        .expect("block blobs must accept any framing");
    assert!(matches!(encoder.framer(), Framer::CharacterDelimited(_)));
}

/// `gelf` defaults to NUL-*separated* records, so appended batches would fuse at the seam. Append
/// mode rejects that default instead of writing an unparseable blob; block mode is unaffected, and
/// an explicit `framing` is still honored.
#[test]
fn azure_blob_append_blob_rejects_unterminated_default_framing() {
    let append_default = AzureBlobSinkConfig {
        blob_type: AzureBlobType::Append,
        ..default_config(
            (
                None::<FramingConfig>,
                GelfSerializerConfig::new(Default::default()),
            )
                .into(),
        )
    };
    let err = append_default
        .build_encoder()
        .expect_err("append mode must reject a separating default framing")
        .to_string();
    assert!(
        err.contains("framing"),
        "error must point at `framing`, got: {err}"
    );

    let append_explicit = AzureBlobSinkConfig {
        blob_type: AzureBlobType::Append,
        ..default_config(
            (
                Some(NewlineDelimitedEncoderConfig::new()),
                GelfSerializerConfig::new(Default::default()),
            )
                .into(),
        )
    };
    assert!(
        append_explicit.build_encoder().is_ok(),
        "an explicit terminating framing must be accepted"
    );

    let block = AzureBlobSinkConfig {
        blob_type: AzureBlobType::Block,
        ..default_config(
            (
                None::<FramingConfig>,
                GelfSerializerConfig::new(Default::default()),
            )
                .into(),
        )
    };
    assert!(
        block.build_encoder().is_ok(),
        "block blobs hold one batch, so the separating default stays valid there"
    );
}

/// The framing resolution is a `blob_type`-dependent config combination, so it must be settled by
/// `validate()` — before anything is built — rather than deep inside sink construction.
#[test]
fn azure_blob_append_blob_rejects_unterminated_default_framing_during_validation() {
    let config = append_blob_config_with_codec("gelf");

    // Fully qualified: `SinkConfig::build` and `ValidatedSink::build` would otherwise collide.
    let err = crate::config::ValidatedSink::validate(&config)
        .expect_err("validation must reject a separating default framing")
        .to_string();
    assert!(
        err.contains("framing"),
        "error must point at `framing`, got: {err}"
    );
}

/// Only the JSON default is `blob_type`-aware: codecs that already default to a stream-safe framing
/// resolve the same in both modes.
#[test]
fn azure_blob_append_blob_keeps_length_delimited_framing() {
    let config = AzureBlobSinkConfig {
        blob_type: AzureBlobType::Append,
        ..default_config((None::<FramingConfig>, NativeSerializerConfig).into())
    };

    let encoder = config
        .build_encoder()
        .expect("binary codecs must build in append mode");
    assert!(
        matches!(encoder.framer(), Framer::LengthDelimited(_)),
        "binary codecs must keep their length-delimited framing"
    );
}

/// Explicit newline framing resolves the same as the append default.
#[test]
fn azure_blob_append_blob_accepts_explicit_newline_framing() {
    let config = AzureBlobSinkConfig {
        blob_type: AzureBlobType::Append,
        ..default_config(
            (
                Some(NewlineDelimitedEncoderConfig::new()),
                JsonSerializerConfig::default(),
            )
                .into(),
        )
    };

    let encoder = config
        .build_encoder()
        .expect("explicit newline framing must be accepted in append mode");
    assert!(encoder.batch_prefix().is_empty());
}

#[test]
fn azure_blob_append_blob_naming_defaults_are_hourly() {
    let config = AzureBlobSinkConfig {
        blob_type: AzureBlobType::Append,
        blob_time_format: None,
        blob_append_uuid: None,
        ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
    };

    assert_eq!(
        config.resolved_blob_naming(),
        (false, "%Y-%m-%dT%H".to_string()),
        "append blobs must rotate hourly and omit the UUID so batches share a blob"
    );
}

#[test]
fn azure_blob_block_blob_naming_defaults_unchanged() {
    let config = AzureBlobSinkConfig {
        blob_type: AzureBlobType::Block,
        blob_time_format: None,
        blob_append_uuid: None,
        ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
    };

    assert_eq!(
        config.resolved_blob_naming(),
        (true, "%s".to_string()),
        "block blob defaults must not be affected by the append-mode defaults"
    );
}

#[test]
fn azure_blob_build_request_append_blob_with_compression() {
    let log = Event::Log(LogEvent::from("test message"));
    let container_name = String::from("logs");
    let sink_config = AzureBlobSinkConfig {
        blob_prefix: "blob".try_into().unwrap(),
        container_name: container_name.clone(),
        ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
    };

    let key = sink_config
        .key_partitioner()
        .unwrap()
        .partition(&log)
        .expect("key wasn't provided");

    let request_options = AzureBlobRequestOptions {
        container_name,
        blob_time_format: "".to_string(),
        blob_append_uuid: false,
        blob_type: AzureBlobType::Append,
        encoder: (
            Default::default(),
            Encoder::<Framer>::new(
                NewlineDelimitedEncoder::default().into(),
                TextSerializerConfig::default().build().into(),
            ),
        ),
        compression: Compression::gzip_default(),
        tags: None,
        metadata: None,
    };

    let mut byte_size = GroupedCountByteSize::new_untagged();
    byte_size.add_event(&log, log.estimated_json_encoded_size_of());

    let (metadata, request_metadata_builder, _events) =
        request_options.split_input((key, vec![log]));

    let payload = EncodeResult::uncompressed(Bytes::new(), byte_size);
    let request_metadata = request_metadata_builder.build(&payload);
    let request = request_options.build_request(metadata, request_metadata, payload);

    assert!(
        request.metadata.partition_key.ends_with(".log.gz"),
        "expected partition_key to end with .log.gz, got: {}",
        request.metadata.partition_key
    );
    assert_eq!(request.content_encoding, Some("gzip"));
    assert_eq!(request.blob_type, AzureBlobType::Append);
}

#[test]
fn azure_blob_append_blob_rejects_oversized_batch() {
    // Validates that batch.validate()?.limit_max_bytes(APPEND_BLOB_MAX_BLOCK_BYTES)?
    // rejects configurations that exceed the Azure 4 MiB append_block limit at startup.
    let mut batch: BatchConfig<crate::sinks::util::BulkSizeBasedDefaultBatchSettings> =
        BatchConfig::default();
    batch.max_bytes = Some(5_000_000); // 5 MB > 4 MiB limit

    let result = batch
        .validate()
        .and_then(|v| v.limit_max_bytes(4 * 1024 * 1024));
    assert!(
        result.is_err(),
        "Expected validation error when max_bytes exceeds the 4 MiB append blob limit"
    );
}

#[test]
fn azure_blob_append_blob_accepts_batch_at_limit() {
    let mut batch: BatchConfig<crate::sinks::util::BulkSizeBasedDefaultBatchSettings> =
        BatchConfig::default();
    batch.max_bytes = Some(4 * 1024 * 1024); // exactly 4 MiB — must be accepted

    let result = batch
        .validate()
        .and_then(|v| v.limit_max_bytes(4 * 1024 * 1024));
    assert!(
        result.is_ok(),
        "Expected max_bytes equal to the limit to be accepted"
    );
}

#[test]
fn azure_blob_block_blob_request_carries_block_type() {
    let log = Event::Log(LogEvent::from("test message"));
    let container_name = String::from("logs");
    let sink_config = AzureBlobSinkConfig {
        blob_prefix: "blob".try_into().unwrap(),
        container_name: container_name.clone(),
        ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
    };

    let key = sink_config
        .key_partitioner()
        .unwrap()
        .partition(&log)
        .expect("key wasn't provided");

    let request_options = AzureBlobRequestOptions {
        container_name,
        blob_time_format: "".to_string(),
        blob_append_uuid: false,
        blob_type: AzureBlobType::Block,
        encoder: (
            Default::default(),
            Encoder::<Framer>::new(
                NewlineDelimitedEncoder::default().into(),
                TextSerializerConfig::default().build().into(),
            ),
        ),
        compression: Compression::None,
        tags: None,
        metadata: None,
    };

    let mut byte_size = GroupedCountByteSize::new_untagged();
    byte_size.add_event(&log, log.estimated_json_encoded_size_of());

    let (metadata, request_metadata_builder, _events) =
        request_options.split_input((key, vec![log]));
    let payload = EncodeResult::uncompressed(Bytes::new(), byte_size);
    let request_metadata = request_metadata_builder.build(&payload);
    let request = request_options.build_request(metadata, request_metadata, payload);

    assert_eq!(request.blob_type, AzureBlobType::Block);
}

#[test]
fn azure_blob_append_blob_with_uuid_override_generates_unique_keys() {
    // Even in append mode, an explicit blob_append_uuid: true produces a UUID suffix.
    // This is intentional: some users may want distinct append blobs per flush.
    let container_name = String::from("logs");
    let sink_config = AzureBlobSinkConfig {
        blob_prefix: "blob".try_into().unwrap(),
        container_name: container_name.clone(),
        ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
    };

    let make_key = || {
        let log = Event::Log(LogEvent::from("test message"));
        let key = sink_config
            .key_partitioner()
            .unwrap()
            .partition(&log)
            .expect("key wasn't provided");

        let request_options = AzureBlobRequestOptions {
            container_name: container_name.clone(),
            blob_time_format: "".to_string(),
            blob_append_uuid: true, // explicit override: UUID even for append type
            blob_type: AzureBlobType::Append,
            encoder: (
                Default::default(),
                Encoder::<Framer>::new(
                    NewlineDelimitedEncoder::default().into(),
                    TextSerializerConfig::default().build().into(),
                ),
            ),
            compression: Compression::None,
            tags: None,
            metadata: None,
        };

        let mut byte_size = GroupedCountByteSize::new_untagged();
        byte_size.add_event(&log, log.estimated_json_encoded_size_of());

        let (metadata, request_metadata_builder, _events) =
            request_options.split_input((key, vec![log]));
        let payload = EncodeResult::uncompressed(Bytes::new(), byte_size);
        let request_metadata = request_metadata_builder.build(&payload);
        request_options
            .build_request(metadata, request_metadata, payload)
            .metadata
            .partition_key
    };

    let key1 = make_key();
    let key2 = make_key();
    assert_ne!(
        key1, key2,
        "uuid override must produce unique keys per flush"
    );
}

#[test]
fn azure_blob_append_blob_stable_name_without_uuid_and_time() {
    // An append blob with empty time format and no UUID always targets the same key,
    // which is the required property for append-mode continuous log streaming.
    let container_name = String::from("logs");
    let sink_config = AzureBlobSinkConfig {
        blob_prefix: "logs/app".try_into().unwrap(),
        container_name: container_name.clone(),
        ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
    };

    let make_key = || {
        let log = Event::Log(LogEvent::from("test message"));
        let key = sink_config
            .key_partitioner()
            .unwrap()
            .partition(&log)
            .expect("key wasn't provided");

        let request_options = AzureBlobRequestOptions {
            container_name: container_name.clone(),
            blob_time_format: "".to_string(), // no time component
            blob_append_uuid: false,          // no UUID
            blob_type: AzureBlobType::Append,
            encoder: (
                Default::default(),
                Encoder::<Framer>::new(
                    NewlineDelimitedEncoder::default().into(),
                    TextSerializerConfig::default().build().into(),
                ),
            ),
            compression: Compression::None,
            tags: None,
            metadata: None,
        };

        let mut byte_size = GroupedCountByteSize::new_untagged();
        byte_size.add_event(&log, log.estimated_json_encoded_size_of());

        let (metadata, request_metadata_builder, _events) =
            request_options.split_input((key, vec![log]));
        let payload = EncodeResult::uncompressed(Bytes::new(), byte_size);
        let request_metadata = request_metadata_builder.build(&payload);
        request_options
            .build_request(metadata, request_metadata, payload)
            .metadata
            .partition_key
    };

    let key1 = make_key();
    let key2 = make_key();
    assert_eq!(
        key1, key2,
        "append blob without UUID and time format must produce a stable key"
    );
    assert_eq!(key1, "logs/app.log");
}

/// An append blob holds one independently compressed stream per flush, so payloads produced with
/// different algorithms must never land in the same blob — the blob's `Content-Encoding` is set once,
/// at creation. The compression extension is part of the blob name, so a `compression` change rotates
/// the name instead of mixing formats. Only a level change inside one algorithm keeps the name, and
/// those streams concatenate (multi-member gzip, multi-frame zstd).
#[test]
fn azure_blob_append_blob_compression_change_rotates_blob_name() {
    let container_name = String::from("logs");
    let sink_config = AzureBlobSinkConfig {
        blob_prefix: "logs/app".try_into().unwrap(),
        container_name: container_name.clone(),
        ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
    };

    // The naming that makes the hazard possible at all: one stable blob per partition.
    let make_key = |compression: Compression| {
        let log = Event::Log(LogEvent::from("test message"));
        let key = sink_config
            .key_partitioner()
            .unwrap()
            .partition(&log)
            .expect("key wasn't provided");

        let request_options = AzureBlobRequestOptions {
            container_name: container_name.clone(),
            blob_time_format: "".to_string(),
            blob_append_uuid: false,
            blob_type: AzureBlobType::Append,
            encoder: (
                Default::default(),
                Encoder::<Framer>::new(
                    NewlineDelimitedEncoder::default().into(),
                    TextSerializerConfig::default().build().into(),
                ),
            ),
            compression,
            tags: None,
            metadata: None,
        };

        let mut byte_size = GroupedCountByteSize::new_untagged();
        byte_size.add_event(&log, log.estimated_json_encoded_size_of());

        let (metadata, request_metadata_builder, _events) =
            request_options.split_input((key, vec![log]));
        let payload = EncodeResult::uncompressed(Bytes::new(), byte_size);
        let request_metadata = request_metadata_builder.build(&payload);
        request_options
            .build_request(metadata, request_metadata, payload)
            .metadata
            .partition_key
    };

    let keys = [
        Compression::None,
        Compression::gzip_default(),
        Compression::zlib_default(),
        Compression::zstd_default(),
        Compression::Snappy,
    ]
    .map(make_key);

    assert_eq!(
        keys,
        [
            "logs/app.log",
            "logs/app.log.gz",
            "logs/app.log.zz",
            "logs/app.log.zst",
            "logs/app.log.snappy",
        ]
        .map(String::from),
        "every compression algorithm must target its own blob, so switching algorithms cannot \
         append a format the existing blob's Content-Encoding does not describe"
    );

    assert_eq!(
        make_key(Compression::Gzip(CompressionLevel::Fast)),
        make_key(Compression::Gzip(CompressionLevel::Best)),
        "a level change stays in the same blob, which is safe: gzip members concatenate"
    );
}

#[test]
fn azure_blob_append_blob_custom_time_format_hourly_rotation() {
    let log = Event::Log(LogEvent::from("test message"));
    let container_name = String::from("logs");
    let sink_config = AzureBlobSinkConfig {
        blob_prefix: "app/".try_into().unwrap(),
        container_name: container_name.clone(),
        ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
    };

    let key = sink_config
        .key_partitioner()
        .unwrap()
        .partition(&log)
        .expect("key wasn't provided");

    let request_options = AzureBlobRequestOptions {
        container_name,
        blob_time_format: "%Y-%m-%d-%H".to_string(), // hourly rotation
        blob_append_uuid: false,
        blob_type: AzureBlobType::Append,
        encoder: (
            Default::default(),
            Encoder::<Framer>::new(
                NewlineDelimitedEncoder::default().into(),
                TextSerializerConfig::default().build().into(),
            ),
        ),
        compression: Compression::None,
        tags: None,
        metadata: None,
    };

    let mut byte_size = GroupedCountByteSize::new_untagged();
    byte_size.add_event(&log, log.estimated_json_encoded_size_of());

    let (metadata, request_metadata_builder, _events) =
        request_options.split_input((key, vec![log]));
    let payload = EncodeResult::uncompressed(Bytes::new(), byte_size);
    let request_metadata = request_metadata_builder.build(&payload);

    // Bracket build_request with two Utc::now() samples so an hour boundary between the
    // formatter's clock read and the assertion doesn't flake the test.
    let before = Utc::now().format("%Y-%m-%d-%H").to_string();
    let request = request_options.build_request(metadata, request_metadata, payload);
    let after = Utc::now().format("%Y-%m-%d-%H").to_string();

    let key = &request.metadata.partition_key;
    assert!(
        *key == format!("app/{before}.log") || *key == format!("app/{after}.log"),
        "partition_key {key:?} did not match the expected hourly key for {before} or {after}"
    );
    assert_eq!(request.blob_type, AzureBlobType::Append);
}

#[tokio::test]
async fn azure_blob_config_parse_blob_type_append() {
    let config: AzureBlobSinkConfig = toml::from_str(
        r#"
            connection_string = "AccountName=mylogstorage"
            container_name = "my-logs"
            blob_type = "append"

            [encoding]
            codec = "json"
        "#,
    )
    .unwrap_or_else(|e| panic!("Config parsing failed: {e:?}"));

    assert_eq!(config.blob_type, AzureBlobType::Append);
}

#[tokio::test]
async fn azure_blob_config_default_blob_type_is_block() {
    let config: AzureBlobSinkConfig = toml::from_str(
        r#"
            connection_string = "AccountName=mylogstorage"
            container_name = "my-logs"

            [encoding]
            codec = "json"
        "#,
    )
    .unwrap_or_else(|e| panic!("Config parsing failed: {e:?}"));

    assert_eq!(
        config.blob_type,
        AzureBlobType::Block,
        "blob_type should default to Block when not specified"
    );
}

#[tokio::test]
async fn azure_blob_append_blob_default_max_bytes_succeeds() {
    // Without explicit batch.max_bytes, append mode defaults to 4 MiB automatically.
    // build() must not fail due to the 10 MB BulkSizeBasedDefault exceeding the limit.
    let config: AzureBlobSinkConfig = toml::from_str(
        r#"
            connection_string = "AccountName=mylogstorage"
            container_name = "my-logs"
            blob_type = "append"

            [encoding]
            codec = "json"
        "#,
    )
    .unwrap_or_else(|e| panic!("Config parsing failed: {e:?}"));

    let cx = SinkContext::default();
    let _ = config
        .build(cx)
        .await
        .unwrap_or_else(|e| panic!("build should succeed without explicit batch.max_bytes: {e:?}"));
}

#[tokio::test]
async fn azure_blob_append_blob_explicit_oversized_batch_fails_at_startup() {
    // If the user explicitly sets batch.max_bytes above the 4 MiB Azure limit, build must fail.
    let config: AzureBlobSinkConfig = toml::from_str(
        r#"
            connection_string = "AccountName=mylogstorage"
            container_name = "my-logs"
            blob_type = "append"

            [encoding]
            codec = "json"

            [batch]
            max_bytes = 5000000
        "#,
    )
    .unwrap_or_else(|e| panic!("Config parsing failed: {e:?}"));

    let cx = SinkContext::default();
    let err = match config.build(cx).await {
        Err(e) => e,
        Ok(_) => panic!(
            "build must fail when batch.max_bytes exceeds the 4 MiB Azure append_block limit"
        ),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("max_bytes") && msg.contains("exceeds"),
        "expected a max_bytes batch limit error, got: {msg}"
    );
}

#[tokio::test]
async fn azure_blob_append_blob_partial_batch_without_max_bytes_succeeds() {
    // A `[batch]` table that sets another field but omits `max_bytes` still triggers the
    // per-field serde default (the 10 MB bulk default). Append mode must treat that inherited
    // default as "unset" and fall back to the 4 MiB append limit, rather than failing at startup.
    let config: AzureBlobSinkConfig = toml::from_str(
        r#"
            connection_string = "AccountName=mylogstorage"
            container_name = "my-logs"
            blob_type = "append"

            [encoding]
            codec = "json"

            [batch]
            timeout_secs = 5
        "#,
    )
    .unwrap_or_else(|e| panic!("Config parsing failed: {e:?}"));

    let cx = SinkContext::default();
    let _ = config.build(cx).await.unwrap_or_else(|e| {
        panic!("build should succeed when [batch] omits max_bytes (only timeout_secs set): {e:?}")
    });
}

fn append_blob_config_with_compression(compression: &str) -> AzureBlobSinkConfig {
    toml::from_str(&format!(
        r#"
            connection_string = "AccountName=mylogstorage"
            container_name = "my-logs"
            blob_type = "append"
            compression = "{compression}"

            [encoding]
            codec = "json"
        "#
    ))
    .unwrap_or_else(|e| panic!("Config parsing failed: {e:?}"))
}

/// Raw Snappy and bare zlib cannot be concatenated, so a compressed append blob would be
/// unreadable past the first batch. Both must fail at startup rather than at read time.
#[tokio::test]
async fn azure_blob_append_blob_rejects_unappendable_compression() {
    for compression in ["snappy", "zlib"] {
        let config = append_blob_config_with_compression(compression);
        let err = match config.build(SinkContext::default()).await {
            Err(e) => e.to_string(),
            Ok(_) => panic!("build must fail for `compression` = `{compression}` in append mode"),
        };
        assert!(
            err.contains(compression) && err.contains("append"),
            "expected an append/compression incompatibility error, got: {err}"
        );
    }
}

#[tokio::test]
async fn azure_blob_append_blob_accepts_concatenable_compression() {
    for compression in ["gzip", "zstd", "none"] {
        let config = append_blob_config_with_compression(compression);
        let _ = config
            .build(SinkContext::default())
            .await
            .unwrap_or_else(|e| {
                panic!("build should succeed for `compression` = `{compression}`: {e:?}")
            });
    }
}

/// The restriction is specific to append blobs: block blobs are written whole, so every
/// compression algorithm stays valid there.
#[tokio::test]
async fn azure_blob_block_blob_accepts_any_compression() {
    for compression in ["snappy", "zlib", "gzip", "zstd", "none"] {
        let config: AzureBlobSinkConfig = toml::from_str(&format!(
            r#"
                connection_string = "AccountName=mylogstorage"
                container_name = "my-logs"
                compression = "{compression}"

                [encoding]
                codec = "json"
            "#
        ))
        .unwrap_or_else(|e| panic!("Config parsing failed: {e:?}"));

        let _ = config
            .build(SinkContext::default())
            .await
            .unwrap_or_else(|e| {
                panic!("block blob build should succeed for `{compression}`: {e:?}")
            });
    }
}

fn append_blob_config_with_codec(codec: &str) -> AzureBlobSinkConfig {
    toml::from_str(&format!(
        r#"
            connection_string = "AccountName=mylogstorage"
            container_name = "my-logs"
            blob_type = "append"

            [encoding]
            codec = "{codec}"
        "#
    ))
    .unwrap_or_else(|e| panic!("Config parsing failed: {e:?}"))
}

/// Azure orders appended blocks by the time it receives them, not by event order, so two flushes
/// in flight against the same blob can land out of order. Append mode resolves to a concurrency of
/// 1 instead of the adaptive default.
#[test]
fn azure_blob_append_blob_defaults_concurrency_to_one() {
    let config = AzureBlobSinkConfig {
        blob_type: AzureBlobType::Append,
        ..default_config((None::<FramingConfig>, JsonSerializerConfig::default()).into())
    };

    assert_eq!(config.resolved_request_settings().concurrency, Some(1));
}

/// Block blobs are written independently of each other, so they keep the adaptive default.
#[test]
fn azure_blob_block_blob_keeps_adaptive_concurrency() {
    let config = AzureBlobSinkConfig {
        blob_type: AzureBlobType::Block,
        ..default_config((None::<FramingConfig>, JsonSerializerConfig::default()).into())
    };

    assert_eq!(config.resolved_request_settings().concurrency, None);
}

/// Only the default is `blob_type`-aware: an explicitly configured concurrency stays the user's
/// call, in append mode too.
#[test]
fn azure_blob_append_blob_honors_explicit_concurrency() {
    let config = AzureBlobSinkConfig {
        blob_type: AzureBlobType::Append,
        request: TowerRequestConfig {
            concurrency: Concurrency::Fixed(4),
            ..Default::default()
        },
        ..default_config((None::<FramingConfig>, JsonSerializerConfig::default()).into())
    };

    assert_eq!(config.resolved_request_settings().concurrency, Some(4));
}

#[test]
fn azure_blob_build_request_append_blob_with_tags_and_metadata() {
    use std::collections::{BTreeMap, HashMap};

    let log = Event::Log(LogEvent::from("test message"));
    let container_name = String::from("logs");
    let sink_config = AzureBlobSinkConfig {
        blob_prefix: "app/".try_into().unwrap(),
        container_name: container_name.clone(),
        ..default_config((None::<FramingConfig>, TextSerializerConfig::default()).into())
    };

    let mut tags = BTreeMap::new();
    tags.insert("Project".to_string(), "Blue".to_string());
    let mut metadata = HashMap::new();
    metadata.insert("source".to_string(), "vector".to_string());

    let key = sink_config
        .key_partitioner()
        .unwrap()
        .partition(&log)
        .expect("key wasn't provided");

    let request_options = AzureBlobRequestOptions {
        container_name,
        blob_time_format: String::from(""),
        blob_append_uuid: false,
        blob_type: AzureBlobType::Append,
        encoder: (
            Default::default(),
            Encoder::<Framer>::new(
                NewlineDelimitedEncoder::default().into(),
                TextSerializerConfig::default().build().into(),
            ),
        ),
        compression: Compression::None,
        tags: Some(tags),
        metadata: Some(metadata.clone()),
    };

    let mut byte_size = GroupedCountByteSize::new_untagged();
    byte_size.add_event(&log, log.estimated_json_encoded_size_of());

    let (azure_metadata, request_metadata_builder, _events) =
        request_options.split_input((key, vec![log]));
    let payload = EncodeResult::uncompressed(Bytes::new(), byte_size);
    let request_metadata = request_metadata_builder.build(&payload);
    let request = request_options.build_request(azure_metadata, request_metadata, payload);

    // Tags/metadata must be propagated into the request regardless of blob_type: append blobs
    // apply them at blob-creation time (see service.rs::append_blob).
    assert_eq!(request.tags, Some("Project=Blue".to_string()));
    assert_eq!(request.blob_metadata, Some(metadata));
    assert_eq!(request.blob_type, AzureBlobType::Append);
}
