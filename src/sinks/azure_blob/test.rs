use bytes::Bytes;
use chrono::Utc;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::{
        NewlineDelimitedEncoder, TextSerializerConfig,
        encoding::{Framer, FramingConfig},
    },
    partition::Partitioner,
    request_metadata::GroupedCountByteSize,
};

use super::{config::AzureBlobSinkConfig, request_builder::AzureBlobRequestOptions};
use crate::{
    codecs::{Encoder, EncodingConfigWithFraming},
    event::{Event, LogEvent},
    sinks::azure_common::config::{AzureAuthentication, AzureBlobType, SpecificAzureCredential},
    sinks::prelude::*,
    sinks::util::{
        BatchConfig, Compression,
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
        batch: Default::default(),
        request: Default::default(),
        acknowledgements: Default::default(),
        tls: Default::default(),
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
        blob_type: Default::default(),
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
        blob_type: Default::default(),
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
        blob_type: Default::default(),
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
        blob_type: Default::default(),
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

#[tokio::test]
async fn azure_blob_build_config_with_null_auth() {
    let config: Result<AzureBlobSinkConfig, toml::de::Error> = toml::from_str::<AzureBlobSinkConfig>(
        r#"
            connection_string = "AccountName=mylogstorage"
            container_name = "my-logs"

            [encoding]
            codec = "json"

            [auth]
        "#,
    );

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
    let config: AzureBlobSinkConfig = toml::from_str::<AzureBlobSinkConfig>(
        r#"
            connection_string = "AccountName=mylogstorage"
            container_name = "my-logs"

            [encoding]
            codec = "json"

            [auth]
            azure_credential_kind = "client_secret_credential"
            azure_tenant_id = "00000000-0000-0000-0000-000000000000"
            azure_client_id = "mock-client-id"
            azure_client_secret = "mock-client-secret"
        "#,
    )
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
    let config: AzureBlobSinkConfig = toml::from_str::<AzureBlobSinkConfig>(
        r#"
            connection_string = "AccountName=mylogstorage"
            container_name = "my-logs"

            [encoding]
            codec = "json"

            [auth]
            azure_credential_kind = "client_certificate_credential"
            azure_tenant_id = "00000000-0000-0000-0000-000000000000"
            azure_client_id = "mock-client-id"
            certificate_path = "tests/data/ClientCertificateAuth.pfx"
            certificate_password = "MockPassword123"
        "#,
    )
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
    let config: AzureBlobSinkConfig = toml::from_str::<AzureBlobSinkConfig>(
        r#"
            account_name = "mylogstorage"
            container_name = "my-logs"

            [encoding]
            codec = "json"

            [auth]
            azure_credential_kind = "client_secret_credential"
            azure_tenant_id = "00000000-0000-0000-0000-000000000000"
            azure_client_id = "mock-client-id"
            azure_client_secret = "mock-client-secret"
        "#,
    )
    .unwrap_or_else(|error| panic!("Config parsing failed: {error:?}"));

    let cx = SinkContext::default();
    let _ = config
        .build(cx)
        .await
        .unwrap_or_else(|error| panic!("Failed to build sink: {error:?}"));
}

#[tokio::test]
async fn azure_blob_build_config_with_account_name_with_no_auth() {
    let config: AzureBlobSinkConfig = toml::from_str::<AzureBlobSinkConfig>(
        r#"
            account_name = "mylogstorage"
            container_name = "my-logs"

            [encoding]
            codec = "json"
        "#,
    )
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
    let config: AzureBlobSinkConfig = toml::from_str::<AzureBlobSinkConfig>(
        r#"
            blob_endpoint = "https://localhost:10000/devstoreaccount1"
            container_name = "my-logs"

            [encoding]
            codec = "json"

            [auth]
            azure_credential_kind = "client_secret_credential"
            azure_tenant_id = "00000000-0000-0000-0000-000000000000"
            azure_client_id = "mock-client-id"
            azure_client_secret = "mock-client-secret"
        "#,
    )
    .unwrap_or_else(|error| panic!("Config parsing failed: {error:?}"));

    let cx = SinkContext::default();
    let _ = config
        .build(cx)
        .await
        .unwrap_or_else(|error| panic!("Failed to build sink: {error:?}"));
}

#[tokio::test]
async fn azure_blob_build_config_with_blob_endpoint_with_no_auth() {
    let config: AzureBlobSinkConfig = toml::from_str::<AzureBlobSinkConfig>(
        r#"
            blob_endpoint = "https://localhost:10000/devstoreaccount1"
            container_name = "my-logs"

            [encoding]
            codec = "json"
        "#,
    )
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
    let config: AzureBlobSinkConfig = toml::from_str::<AzureBlobSinkConfig>(
        r#"
            connection_string = "AccountName=mylogstorage"
            account_name = "mylogstorage"
            container_name = "my-logs"

            [encoding]
            codec = "json"
        "#,
    )
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
    let config: AzureBlobSinkConfig = toml::from_str::<AzureBlobSinkConfig>(
        r#"
            connection_string = "AccountName=mylogstorage;AccountKey=mockkey"
            container_name = "my-logs"

            [encoding]
            codec = "json"

            [auth]
            azure_credential_kind = "client_secret_credential"
            azure_tenant_id = "00000000-0000-0000-0000-000000000000"
            azure_client_id = "mock-client-id"
            azure_client_secret = "mock-client-secret"
        "#,
    )
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
    let config: AzureBlobSinkConfig = toml::from_str::<AzureBlobSinkConfig>(
        r#"
            account_name = "mylogstorage"
            container_name = "my-logs"

            [encoding]
            codec = "json"

            [tls]
            ca_file = "tests/data/ca/certs/ca.cert.pem"

            [auth]
            azure_credential_kind = "client_secret_credential"
            azure_tenant_id = "00000000-0000-0000-0000-000000000000"
            azure_client_id = "mock-client-id"
            azure_client_secret = "mock-client-secret"
        "#,
    )
    .unwrap_or_else(|error| panic!("Config parsing failed: {error:?}"));

    let cx = SinkContext::default();
    let _ = config
        .build(cx)
        .await
        .unwrap_or_else(|error| panic!("Failed to build sink: {error:?}"));
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
        blob_time_format: "%Y-%m-%d".to_string(),
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
    };

    let mut byte_size = GroupedCountByteSize::new_untagged();
    byte_size.add_event(&log, log.estimated_json_encoded_size_of());

    let (metadata, request_metadata_builder, _events) =
        request_options.split_input((key, vec![log]));

    let payload = EncodeResult::uncompressed(Bytes::new(), byte_size);
    let request_metadata = request_metadata_builder.build(&payload);
    let request = request_options.build_request(metadata, request_metadata, payload);

    let expected_date = Utc::now().format("%Y-%m-%d").to_string();
    assert_eq!(
        request.metadata.partition_key,
        format!("blob/{expected_date}.log")
    );
    assert_eq!(request.blob_type, AzureBlobType::Append);
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
    };

    let mut byte_size = GroupedCountByteSize::new_untagged();
    byte_size.add_event(&log, log.estimated_json_encoded_size_of());

    let (metadata, request_metadata_builder, _events) =
        request_options.split_input((key, vec![log]));
    let payload = EncodeResult::uncompressed(Bytes::new(), byte_size);
    let request_metadata = request_metadata_builder.build(&payload);
    let request = request_options.build_request(metadata, request_metadata, payload);

    let expected = format!("app/{}.log", Utc::now().format("%Y-%m-%d-%H"));
    assert_eq!(request.metadata.partition_key, expected);
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
