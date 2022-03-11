use std::{
    io::{BufRead, BufReader},
    num::NonZeroU32,
};

use azure_core::{prelude::Range, HttpError};
use azure_storage_blobs::prelude::*;
use bytes::{Buf, BytesMut};
use flate2::read::GzDecoder;
use futures::{stream, Stream, StreamExt};
use http::StatusCode;
use vector_core::ByteSizeOf;

use super::config::AzureBlobSinkConfig;
use crate::{
    config::SinkContext,
    event::{Event, EventArray, LogEvent},
    sinks::{
        azure_common,
        util::{encoding::StandardEncodings, Compression, TowerRequestConfig},
        VectorSink,
    },
    test_util::{random_events_with_stream, random_lines, random_lines_with_stream},
};

#[tokio::test]
async fn azure_blob_healthcheck_passed() {
    let config = AzureBlobSinkConfig::new_emulator().await;
    let client =
        azure_common::config::build_client(config.connection_string, config.container_name.clone())
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
    let client =
        azure_common::config::build_client(config.connection_string, config.container_name.clone())
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
    let blob_prefix = String::from("lines/into/blob");
    let config = AzureBlobSinkConfig::new_emulator().await;
    let config = AzureBlobSinkConfig {
        blob_prefix: Some(blob_prefix.clone()),
        ..config
    };
    let sink = config.to_sink();
    let (lines, input) = random_lines_with_stream(100, 10, None);

    sink.run(input).await.expect("Failed to run sink");

    let blobs = config.list_blobs(blob_prefix.as_str()).await;
    assert_eq!(blobs.len(), 1);
    assert!(blobs[0].clone().ends_with(".log"));
    let (blob, blob_lines) = config.get_blob(blobs[0].clone()).await;
    assert_eq!(blob.properties.content_type, String::from("text/plain"));
    assert_eq!(lines, blob_lines);
}

#[tokio::test]
async fn azure_blob_insert_json_into_blob() {
    let blob_prefix = String::from("json/into/blob");
    let config = AzureBlobSinkConfig::new_emulator().await;
    let config = AzureBlobSinkConfig {
        blob_prefix: Some(blob_prefix.clone()),
        encoding: StandardEncodings::Ndjson.into(),
        ..config
    };
    let sink = config.to_sink();
    let (events, input) = random_events_with_stream(100, 10, None);

    sink.run(input).await.expect("Failed to run sink");

    let blobs = config.list_blobs(blob_prefix.as_str()).await;
    assert_eq!(blobs.len(), 1);
    assert!(blobs[0].clone().ends_with(".log"));
    let (blob, blob_lines) = config.get_blob(blobs[0].clone()).await;
    assert_eq!(blob.properties.content_type, String::from("text/plain"));
    let expected = events
        .iter()
        .map(|event| serde_json::to_string(&event.as_log().all_fields()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(expected, blob_lines);
}

#[ignore]
#[tokio::test]
// This test will fail with Azurite blob emulator because of this issue:
// https://github.com/Azure/Azurite/issues/629
async fn azure_blob_insert_lines_into_blob_gzip() {
    let blob_prefix = String::from("lines-gzip/into/blob");
    let config = AzureBlobSinkConfig::new_emulator().await;
    let config = AzureBlobSinkConfig {
        blob_prefix: Some(blob_prefix.clone()),
        compression: Compression::gzip_default(),
        ..config
    };
    let sink = config.to_sink();
    let (lines, events) = random_lines_with_stream(100, 10, None);

    sink.run(events).await.expect("Failed to run sink");

    let blobs = config.list_blobs(blob_prefix.as_str()).await;
    assert_eq!(blobs.len(), 1);
    assert!(blobs[0].clone().ends_with(".log.gz"));
    let (blob, blob_lines) = config.get_blob(blobs[0].clone()).await;
    assert_eq!(
        blob.properties.content_type,
        String::from("application/gzip")
    );
    assert_eq!(lines, blob_lines);
}

#[ignore]
#[tokio::test]
// This test will fail with Azurite blob emulator because of this issue:
// https://github.com/Azure/Azurite/issues/629
async fn azure_blob_insert_json_into_blob_gzip() {
    let blob_prefix = String::from("json-gzip/into/blob");
    let config = AzureBlobSinkConfig::new_emulator().await;
    let config = AzureBlobSinkConfig {
        blob_prefix: Some(blob_prefix.clone()),
        encoding: StandardEncodings::Ndjson.into(),
        compression: Compression::gzip_default(),
        ..config
    };
    let sink = config.to_sink();
    let (events, input) = random_events_with_stream(100, 10, None);

    sink.run(input).await.expect("Failed to run sink");

    let blobs = config.list_blobs(blob_prefix.as_str()).await;
    assert_eq!(blobs.len(), 1);
    assert!(blobs[0].clone().ends_with(".log.gz"));
    let (blob, blob_lines) = config.get_blob(blobs[0].clone()).await;
    assert_eq!(
        blob.properties.content_type,
        String::from("application/gzip")
    );
    let expected = events
        .iter()
        .map(|event| serde_json::to_string(&event.as_log().all_fields()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(expected, blob_lines);
}

#[tokio::test]
async fn azure_blob_rotate_files_after_the_buffer_size_is_reached() {
    let groups = 3;
    let (lines, size, input) = random_lines_with_stream_with_group_key(100, 30, groups);
    let size_per_group = (size / groups) + 10;

    let blob_prefix = String::from("lines-rotate/into/blob/");
    let mut config = AzureBlobSinkConfig::new_emulator().await;
    config.batch.max_bytes = Some(size_per_group);

    let config = AzureBlobSinkConfig {
        blob_prefix: Some(blob_prefix.clone() + "{{key}}"),
        blob_append_uuid: Some(false),
        batch: config.batch,
        ..config
    };

    let sink = config.to_sink();
    sink.run(input).await.expect("Failed to run sink");

    let blobs = config.list_blobs(blob_prefix.as_str()).await;
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
                connection_string: format!("UseDevelopmentStorage=true;DefaultEndpointsProtocol=http;AccountName=devstoreaccount1;AccountKey=Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==;BlobEndpoint=http://{}:10000/devstoreaccount1;QueueEndpoint=http://{}:10001/devstoreaccount1;TableEndpoint=http://{}:10002/devstoreaccount1;", address, address, address),
                container_name: "logs".to_string(),
                blob_prefix: None,
                blob_time_format: None,
                blob_append_uuid: None,
                encoding: StandardEncodings::Text.into(),
                compression: Compression::None,
                batch: Default::default(),
                request: TowerRequestConfig::default(),
                acknowledgements: Default::default(),
            };

        config.ensure_container().await;

        config
    }

    pub fn to_sink(&self) -> VectorSink {
        let cx = SinkContext::new_test();
        let client = azure_common::config::build_client(
            self.connection_string.clone(),
            self.container_name.clone(),
        )
        .expect("Failed to create client");

        self.build_processor(client, cx)
            .expect("Failed to create sink")
    }

    pub async fn list_blobs(&self, prefix: &str) -> Vec<String> {
        let client = azure_common::config::build_client(
            self.connection_string.clone(),
            self.container_name.clone(),
        )
        .unwrap();
        let response = client
            .list_blobs()
            .prefix(prefix)
            .max_results(NonZeroU32::new(1000).unwrap())
            .delimiter("/")
            .include_metadata(true)
            .execute()
            .await
            .expect("Failed to fetch blobs");

        let blobs = response
            .blobs
            .blobs
            .iter()
            .map(|blob| blob.name.clone())
            .collect::<Vec<_>>();

        blobs
    }

    pub async fn get_blob(&self, blob: String) -> (Blob, Vec<String>) {
        let client = azure_common::config::build_client(
            self.connection_string.clone(),
            self.container_name.clone(),
        )
        .unwrap();
        let response = client
            .as_blob_client(blob.as_str())
            .get()
            .range(Range::new(0, 1024 * 1024))
            .execute()
            .await
            .expect("Failed to get blob");

        (response.blob, self.get_blob_content(response.data.to_vec()))
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
            self.connection_string.clone(),
            self.container_name.clone(),
        )
        .unwrap();
        let request = client.create().public_access(PublicAccess::None).execute();

        let response = match request.await {
            Ok(_) => Ok(()),
            Err(reason) => match reason.downcast_ref::<HttpError>() {
                Some(HttpError::StatusCode { status, .. }) => match *status {
                    StatusCode::CONFLICT => Ok(()),
                    status => Err(format!("Unexpected status code {}", status)),
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
