use std::{
    io::{BufRead, BufReader},
    num::NonZeroU32,
};

use azure_core::{error::HttpError, prelude::Range};
use azure_storage_blobs::prelude::*;
use bytes::{Buf, BytesMut};
use flate2::read::GzDecoder;
use futures::{stream, Stream, StreamExt};
use http::StatusCode;
use vector_lib::codecs::{
    encoding::FramingConfig, JsonSerializerConfig, NewlineDelimitedEncoderConfig,
    TextSerializerConfig,
};
use vector_lib::ByteSizeOf;

use super::config::AzureBlobSinkConfig;
use crate::{
    event::{Event, EventArray, LogEvent},
    sinks::{
        azure_common,
        util::{Compression, TowerRequestConfig},
        VectorSink,
    },
    test_util::{
        components::{assert_sink_compliance, SINK_TAGS},
        random_events_with_stream, random_lines, random_lines_with_stream, random_string,
    },
};

#[tokio::test]
async fn azure_blob_healthcheck_passed() {
    let config = AzureBlobSinkConfig::new_emulator().await;
    let client = azure_common::config::build_client(
        config.connection_string.map(Into::into),
        None,
        config.container_name.clone(),
        None,
    )
    .expect("Failed to create client");

    let response = azure_common::config::build_healthcheck(config.container_name, client);

    response.expect("Failed to pass healthcheck");
}

#[tokio::test]
async fn azure_blob_healthcheck_unknown_container() {
    let config = AzureBlobSinkConfig::new_emulator().await;
    let config = AzureBlobSinkConfig {
        container_name: String::from("other-container-name"),
        ..config
    };
    let client = azure_common::config::build_client(
        config.connection_string.map(Into::into),
        config.storage_account.map(Into::into),
        config.container_name.clone(),
        config.endpoint.clone(),
    )
    .expect("Failed to create client");

    assert_eq!(
        azure_common::config::build_healthcheck(config.container_name, client)
            .unwrap()
            .await
            .unwrap_err()
            .to_string(),
        "Container: \"other-container-name\" not found"
    );
}

#[tokio::test]
async fn azure_blob_insert_lines_into_blob() {
    let blob_prefix = format!("lines/into/blob/{}", random_string(10));
    let config = AzureBlobSinkConfig::new_emulator().await;
    let config = AzureBlobSinkConfig {
        blob_prefix: blob_prefix.clone().try_into().unwrap(),
        ..config
    };
    let (lines, input) = random_lines_with_stream(100, 10, None);

    config.run_assert(input).await;

    let blobs = config.list_blobs(blob_prefix).await;
    assert_eq!(blobs.len(), 1);
    assert!(blobs[0].clone().ends_with(".log"));
    let (blob, blob_lines) = config.get_blob(blobs[0].clone()).await;
    assert_eq!(blob.properties.content_type, String::from("text/plain"));
    assert_eq!(lines, blob_lines);
}

#[tokio::test]
async fn azure_blob_insert_json_into_blob() {
    let blob_prefix = format!("json/into/blob/{}", random_string(10));
    let config = AzureBlobSinkConfig::new_emulator().await;
    let config = AzureBlobSinkConfig {
        blob_prefix: blob_prefix.clone().try_into().unwrap(),
        encoding: (
            Some(NewlineDelimitedEncoderConfig::new()),
            JsonSerializerConfig::default(),
        )
            .into(),
        ..config
    };
    let (events, input) = random_events_with_stream(100, 10, None);

    config.run_assert(input).await;

    let blobs = config.list_blobs(blob_prefix).await;
    assert_eq!(blobs.len(), 1);
    assert!(blobs[0].clone().ends_with(".log"));
    let (blob, blob_lines) = config.get_blob(blobs[0].clone()).await;
    assert_eq!(blob.properties.content_encoding, None);
    assert_eq!(
        blob.properties.content_type,
        String::from("application/x-ndjson")
    );
    let expected = events
        .iter()
        .map(|event| serde_json::to_string(&event.as_log().all_event_fields().unwrap()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(expected, blob_lines);
}

#[tokio::test]
// This test will fail with Azurite blob emulator because of this issue:
// https://github.com/Azure/Azurite/issues/629
async fn azure_blob_insert_lines_into_blob_gzip() {
    let blob_prefix = format!("lines-gzip/into/blob/{}", random_string(10));
    let config = AzureBlobSinkConfig::new_emulator().await;
    let config = AzureBlobSinkConfig {
        blob_prefix: blob_prefix.clone().try_into().unwrap(),
        compression: Compression::gzip_default(),
        ..config
    };
    let (lines, events) = random_lines_with_stream(100, 10, None);

    config.run_assert(events).await;

    let blobs = config.list_blobs(blob_prefix).await;
    assert_eq!(blobs.len(), 1);
    assert!(blobs[0].clone().ends_with(".log.gz"));
    let (blob, blob_lines) = config.get_blob(blobs[0].clone()).await;
    assert_eq!(blob.properties.content_encoding, Some(String::from("gzip")));
    assert_eq!(blob.properties.content_type, String::from("text/plain"));
    assert_eq!(lines, blob_lines);
}

#[ignore]
#[tokio::test]
// This test will fail with Azurite blob emulator because of this issue:
// https://github.com/Azure/Azurite/issues/629
async fn azure_blob_insert_json_into_blob_gzip() {
    let blob_prefix = format!("json-gzip/into/blob/{}", random_string(10));
    let config = AzureBlobSinkConfig::new_emulator().await;
    let config = AzureBlobSinkConfig {
        blob_prefix: blob_prefix.clone().try_into().unwrap(),
        encoding: (
            Some(NewlineDelimitedEncoderConfig::new()),
            JsonSerializerConfig::default(),
        )
            .into(),
        compression: Compression::gzip_default(),
        ..config
    };
    let (events, input) = random_events_with_stream(100, 10, None);

    config.run_assert(input).await;

    let blobs = config.list_blobs(blob_prefix).await;
    assert_eq!(blobs.len(), 1);
    assert!(blobs[0].clone().ends_with(".log.gz"));
    let (blob, blob_lines) = config.get_blob(blobs[0].clone()).await;
    assert_eq!(blob.properties.content_encoding, Some(String::from("gzip")));
    assert_eq!(
        blob.properties.content_type,
        String::from("application/x-ndjson")
    );
    let expected = events
        .iter()
        .map(|event| serde_json::to_string(&event.as_log().all_event_fields().unwrap()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(expected, blob_lines);
}

#[tokio::test]
async fn azure_blob_rotate_files_after_the_buffer_size_is_reached() {
    let groups = 3;
    let (lines, size, input) = random_lines_with_stream_with_group_key(100, 30, groups);
    let size_per_group = (size / groups) + 10;

    let blob_prefix = format!("lines-rotate/into/blob/{}", random_string(10));
    let mut config = AzureBlobSinkConfig::new_emulator().await;
    config.batch.max_bytes = Some(size_per_group);

    let config = AzureBlobSinkConfig {
        blob_prefix: (blob_prefix.clone() + "{{key}}").try_into().unwrap(),
        blob_append_uuid: Some(false),
        batch: config.batch,
        ..config
    };

    config.run_assert(input).await;

    let blobs = config.list_blobs(blob_prefix).await;
    assert_eq!(blobs.len(), 3);
    let response = stream::iter(blobs)
        .fold(Vec::new(), |mut acc, blob| async {
            let (_, lines) = config.get_blob(blob).await;
            acc.push(lines);
            acc
        })
        .await;

    for i in 0..groups {
        assert_eq!(&lines[(i * 10)..((i + 1) * 10)], response[i].as_slice());
    }
}

impl AzureBlobSinkConfig {
    pub async fn new_emulator() -> AzureBlobSinkConfig {
        let address = std::env::var("AZURE_ADDRESS").unwrap_or_else(|_| "localhost".into());
        let config = AzureBlobSinkConfig {
                connection_string: Some(format!("UseDevelopmentStorage=true;DefaultEndpointsProtocol=http;AccountName=devstoreaccount1;AccountKey=Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==;BlobEndpoint=http://{}:10000/devstoreaccount1;QueueEndpoint=http://{}:10001/devstoreaccount1;TableEndpoint=http://{}:10002/devstoreaccount1;", address, address, address).into()),
                storage_account: None,
                container_name: "logs".to_string(),
                endpoint: None,
                blob_prefix: Default::default(),
                blob_time_format: None,
                blob_append_uuid: None,
                encoding: (None::<FramingConfig>, TextSerializerConfig::default()).into(),
                compression: Compression::None,
                batch: Default::default(),
                request: TowerRequestConfig::default(),
                acknowledgements: Default::default(),
            };

        config.ensure_container().await;

        config
    }

    fn to_sink(&self) -> VectorSink {
        let client = azure_common::config::build_client(
            self.connection_string.clone().map(Into::into),
            self.storage_account.clone().map(Into::into),
            self.container_name.clone(),
            self.endpoint.clone(),
        )
        .expect("Failed to create client");

        self.build_processor(client).expect("Failed to create sink")
    }

    async fn run_assert(&self, input: impl Stream<Item = EventArray> + Send) {
        // `to_sink` needs to be inside the assertion check
        assert_sink_compliance(&SINK_TAGS, async move { self.to_sink().run(input).await })
            .await
            .expect("Running sink failed");
    }

    pub async fn list_blobs(&self, prefix: String) -> Vec<String> {
        let client = azure_common::config::build_client(
            self.connection_string.clone().map(Into::into),
            self.storage_account.clone().map(Into::into),
            self.container_name.clone(),
            self.endpoint.clone(),
        )
        .unwrap();
        let response = client
            .list_blobs()
            .prefix(prefix)
            .max_results(NonZeroU32::new(1000).unwrap())
            .delimiter("/")
            .include_metadata(true)
            .into_stream()
            .next()
            .await
            .expect("Failed to fetch blobs")
            .unwrap();

        let blobs = response
            .blobs
            .blobs()
            .map(|blob| blob.name.clone())
            .collect::<Vec<_>>();

        blobs
    }

    pub async fn get_blob(&self, blob: String) -> (Blob, Vec<String>) {
        let client = azure_common::config::build_client(
            self.connection_string.clone().map(Into::into),
            self.storage_account.clone().map(Into::into),
            self.container_name.clone(),
            self.endpoint.clone(),
        )
        .unwrap();
        let response = client
            .blob_client(blob)
            .get()
            .range(Range::new(0, 1024 * 1024))
            .into_stream()
            .next()
            .await
            .expect("Failed to get blob")
            .unwrap();

        (
            response.blob,
            self.get_blob_content(response.data.collect().await.unwrap().to_vec()),
        )
    }

    fn get_blob_content(&self, data: Vec<u8>) -> Vec<String> {
        let body = BytesMut::from(data.as_slice()).freeze().reader();

        if self.compression == Compression::None {
            BufReader::new(body).lines().map(|l| l.unwrap()).collect()
        } else {
            BufReader::new(GzDecoder::new(body))
                .lines()
                .map(|l| l.unwrap())
                .collect()
        }
    }

    async fn ensure_container(&self) {
        let client = azure_common::config::build_client(
            self.connection_string.clone().map(Into::into),
            self.storage_account.clone().map(Into::into),
            self.container_name.clone(),
            self.endpoint.clone(),
        )
        .unwrap();
        let request = client
            .create()
            .public_access(PublicAccess::None)
            .into_future();

        let response = match request.await {
            Ok(_) => Ok(()),
            Err(reason) => match reason.downcast_ref::<HttpError>() {
                Some(err) => match StatusCode::from_u16(err.status().into()) {
                    Ok(StatusCode::CONFLICT) => Ok(()),
                    _ => Err(format!("Unexpected status code {}", err.status())),
                },
                _ => Err(format!("Unexpected error {}", reason)),
            },
        };

        response.expect("Failed to create container")
    }
}

fn random_lines_with_stream_with_group_key(
    len: usize,
    count: usize,
    groups: usize,
) -> (Vec<String>, usize, impl Stream<Item = EventArray>) {
    let key = count / groups;
    let lines = random_lines(len).take(count).collect::<Vec<_>>();
    let (size, events) = lines
        .clone()
        .into_iter()
        .enumerate()
        .map(move |(i, line)| {
            let mut log = LogEvent::from(line);
            let i = ((i / key) + 1) as i32;
            log.insert("key", i);
            Event::from(log)
        })
        .fold((0, Vec::new()), |(mut size, mut events), event| {
            size += event.size_of();
            events.push(event.into());
            (size, events)
        });

    (lines, size, stream::iter(events))
}
