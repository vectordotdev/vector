use std::io::{BufRead, BufReader};
use std::sync::Arc;

use azure_core::http::StatusCode;
use azure_storage_blob::BlobContainerClient;

use bytes::{Buf, BytesMut};
use flate2::read::MultiGzDecoder;
use futures::{Stream, StreamExt, stream};
use vector_lib::{
    ByteSizeOf,
    codecs::{
        JsonSerializerConfig, NewlineDelimitedEncoderConfig, TextSerializerConfig,
        encoding::FramingConfig,
    },
};

use super::config::AzureBlobSinkConfig;
use crate::{
    event::{Event, EventArray, LogEvent},
    sinks::{
        VectorSink, azure_common,
        azure_common::config::AzureBlobType,
        util::{Compression, TowerRequestConfig},
    },
    test_util::{
        components::{SINK_TAGS, assert_sink_compliance},
        random_events_with_stream, random_lines, random_lines_with_stream, random_string,
    },
    tls,
};

#[tokio::test]
async fn azure_blob_healthcheck_passed() {
    let config = AzureBlobSinkConfig::new_emulator().await;
    let client = config.build_test_client().await;

    azure_common::config::build_healthcheck(config.container_name, client)
        .expect("Failed to build healthcheck")
        .await
        .expect("Failed to pass healthcheck");
}

#[tokio::test]
async fn azure_blob_healthcheck_passed_with_oauth() {
    let config = AzureBlobSinkConfig::new_emulator_with_oauth().await;
    let client = config.build_test_client().await;

    azure_common::config::build_healthcheck(config.container_name, client)
        .expect("Failed to build healthcheck")
        .await
        .expect("Failed to pass healthcheck");
}

#[tokio::test]
async fn azure_blob_healthcheck_unknown_container() {
    let config = AzureBlobSinkConfig::new_emulator().await;
    let config = AzureBlobSinkConfig {
        container_name: String::from("other-container-name"),
        ..config
    };
    let client = config.build_test_client().await;

    assert_eq!(
        azure_common::config::build_healthcheck(config.container_name, client)
            .unwrap()
            .await
            .unwrap_err()
            .to_string(),
        "Container: \"other-container-name\" not found"
    );
}

async fn assert_insert_lines_into_blob(config: AzureBlobSinkConfig) {
    let blob_prefix = format!("lines/into/blob/{}", random_string(10));
    let config = AzureBlobSinkConfig {
        blob_prefix: blob_prefix.clone().try_into().unwrap(),
        ..config
    };
    let (lines, input) = random_lines_with_stream(100, 10, None);

    config.run_assert(input).await;

    let blobs = config.list_blobs(blob_prefix).await;
    assert_eq!(blobs.len(), 1);
    assert!(blobs[0].clone().ends_with(".log"));
    let (content_type, content_encoding, blob_lines) = config.get_blob(blobs[0].clone()).await;
    assert_eq!(content_type, Some(String::from("text/plain")));
    assert_eq!(content_encoding, None);
    assert_eq!(lines, blob_lines);
}

#[tokio::test]
async fn azure_blob_insert_lines_into_blob() {
    assert_insert_lines_into_blob(AzureBlobSinkConfig::new_emulator().await).await;
}

#[tokio::test]
async fn azure_blob_insert_lines_into_blob_with_oauth() {
    assert_insert_lines_into_blob(AzureBlobSinkConfig::new_emulator_with_oauth().await).await;
}

async fn assert_insert_json_into_blob(config: AzureBlobSinkConfig) {
    let blob_prefix = format!("json/into/blob/{}", random_string(10));
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
    let (content_type, content_encoding, blob_lines) = config.get_blob(blobs[0].clone()).await;
    assert_eq!(content_encoding, None);
    assert_eq!(content_type, Some(String::from("application/x-ndjson")));
    let expected = events
        .iter()
        .map(|event| serde_json::to_string(&event.as_log().all_event_fields().unwrap()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(expected, blob_lines);
}

#[tokio::test]
async fn azure_blob_insert_json_into_blob() {
    assert_insert_json_into_blob(AzureBlobSinkConfig::new_emulator().await).await;
}

#[tokio::test]
async fn azure_blob_insert_json_into_blob_with_oauth() {
    assert_insert_json_into_blob(AzureBlobSinkConfig::new_emulator_with_oauth().await).await;
}

#[ignore]
#[tokio::test]
// This test fails to get the posted blob with "header not found content-length".
// However, we inspected that the sink writes the expected contents to Azure thus this is a retrieval/test issue.
// Additional context: https://github.com/Azure/Azurite/issues/629
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
    let (content_type, content_encoding, blob_lines) = config.get_blob(blobs[0].clone()).await;
    assert_eq!(content_encoding, Some(String::from("gzip")));
    assert_eq!(content_type, Some(String::from("text/plain")));
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
    let (content_type, content_encoding, blob_lines) = config.get_blob(blobs[0].clone()).await;
    assert_eq!(content_encoding, Some(String::from("gzip")));
    assert_eq!(content_type, Some(String::from("application/x-ndjson")));
    let expected = events
        .iter()
        .map(|event| serde_json::to_string(&event.as_log().all_event_fields().unwrap()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(expected, blob_lines);
}

async fn assert_rotate_files_after_the_buffer_size_is_reached(mut config: AzureBlobSinkConfig) {
    let groups = 3;
    let (lines, size, input) = random_lines_with_stream_with_group_key(100, 30, groups);
    let size_per_group = (size / groups) + 10;

    let blob_prefix = format!("lines-rotate/into/blob/{}", random_string(10));
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
            let (_, _, lines) = config.get_blob(blob).await;
            acc.push(lines);
            acc
        })
        .await;

    for i in 0..groups {
        assert_eq!(&lines[(i * 10)..((i + 1) * 10)], response[i].as_slice());
    }
}

#[tokio::test]
async fn azure_blob_rotate_files_after_the_buffer_size_is_reached() {
    assert_rotate_files_after_the_buffer_size_is_reached(AzureBlobSinkConfig::new_emulator().await)
        .await;
}

#[tokio::test]
async fn azure_blob_rotate_files_after_the_buffer_size_is_reached_with_oauth() {
    assert_rotate_files_after_the_buffer_size_is_reached(
        AzureBlobSinkConfig::new_emulator_with_oauth().await,
    )
    .await;
}

// ── Append blob integration tests ─────────────────────────────────────────────

/// Two sequential batches land in exactly one blob and their lines appear in order.
async fn assert_append_blob_reuses_same_blob(config: AzureBlobSinkConfig) {
    let blob_prefix = format!("append/basic/{}", random_string(10));
    let config = AzureBlobSinkConfig {
        blob_prefix: blob_prefix.clone().try_into().unwrap(),
        blob_type: AzureBlobType::Append,
        blob_time_format: Some(String::new()), // stable name — no time component
        blob_append_uuid: Some(false),
        ..config
    };

    let (lines1, input1) = random_lines_with_stream(100, 5, None);
    let (lines2, input2) = random_lines_with_stream(100, 5, None);

    config.run_assert(input1).await;
    config.run_assert(input2).await;

    let blobs = config.list_blobs(blob_prefix).await;
    assert_eq!(blobs.len(), 1, "append blob mode must reuse a single blob");
    let (content_type, _content_encoding, blob_lines) = config.get_blob(blobs[0].clone()).await;
    assert_eq!(content_type, Some(String::from("text/plain")));

    let expected: Vec<String> = lines1.into_iter().chain(lines2).collect();
    assert_eq!(blob_lines, expected);
}

#[tokio::test]
async fn azure_blob_append_blob_reuses_same_blob() {
    assert_append_blob_reuses_same_blob(AzureBlobSinkConfig::new_emulator().await).await;
}

#[tokio::test]
async fn azure_blob_append_blob_reuses_same_blob_with_oauth() {
    assert_append_blob_reuses_same_blob(AzureBlobSinkConfig::new_emulator_with_oauth().await).await;
}

/// NDJSON append: two batches of structured events land in one blob with the correct content-type
/// and all JSON lines intact and in order.
async fn assert_append_blob_json_encoding(config: AzureBlobSinkConfig) {
    let blob_prefix = format!("append/json/{}", random_string(10));
    let config = AzureBlobSinkConfig {
        blob_prefix: blob_prefix.clone().try_into().unwrap(),
        blob_type: AzureBlobType::Append,
        blob_time_format: Some(String::new()),
        blob_append_uuid: Some(false),
        encoding: (
            Some(NewlineDelimitedEncoderConfig::new()),
            JsonSerializerConfig::default(),
        )
            .into(),
        ..config
    };

    let (events1, input1) = random_events_with_stream(100, 5, None);
    let (events2, input2) = random_events_with_stream(100, 5, None);

    config.run_assert(input1).await;
    config.run_assert(input2).await;

    let blobs = config.list_blobs(blob_prefix).await;
    assert_eq!(blobs.len(), 1, "append blob must produce exactly one blob");
    let (content_type, _content_encoding, blob_lines) = config.get_blob(blobs[0].clone()).await;
    assert_eq!(
        content_type,
        Some(String::from("application/x-ndjson")),
        "content-type must reflect NDJSON encoding"
    );

    let expected: Vec<String> = events1
        .iter()
        .chain(events2.iter())
        .map(|e| serde_json::to_string(&e.as_log().all_event_fields().unwrap()).unwrap())
        .collect();
    assert_eq!(blob_lines, expected);
}

#[tokio::test]
async fn azure_blob_append_blob_json_encoding() {
    assert_append_blob_json_encoding(AzureBlobSinkConfig::new_emulator().await).await;
}

#[tokio::test]
async fn azure_blob_append_blob_json_encoding_with_oauth() {
    assert_append_blob_json_encoding(AzureBlobSinkConfig::new_emulator_with_oauth().await).await;
}

/// Default daily rotation: without explicit blob_time_format or blob_append_uuid overrides,
/// append blobs use `%Y-%m-%d` and no UUID — two batches both write to today's date blob.
async fn assert_append_blob_default_daily_rotation(config: AzureBlobSinkConfig) {
    let blob_prefix = format!("append/daily/{}/", random_string(10));
    let config = AzureBlobSinkConfig {
        blob_prefix: blob_prefix.clone().try_into().unwrap(),
        blob_type: AzureBlobType::Append,
        // Intentionally leave blob_time_format and blob_append_uuid at None
        // to exercise the type-aware defaults in build_processor.
        blob_time_format: None,
        blob_append_uuid: None,
        ..config
    };

    let (lines1, input1) = random_lines_with_stream(100, 5, None);
    let (lines2, input2) = random_lines_with_stream(100, 5, None);

    config.run_assert(input1).await;
    config.run_assert(input2).await;

    let blobs = config.list_blobs(blob_prefix.clone()).await;
    assert_eq!(
        blobs.len(),
        1,
        "both batches must go to the same daily-rotated blob"
    );

    // The blob name must embed today's date in %Y-%m-%d format.
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    assert!(
        blobs[0].contains(&today),
        "blob name '{}' must contain today's date '{}'",
        blobs[0],
        today
    );

    let (_content_type, _content_encoding, blob_lines) = config.get_blob(blobs[0].clone()).await;
    let expected: Vec<String> = lines1.into_iter().chain(lines2).collect();
    assert_eq!(blob_lines, expected);
}

#[tokio::test]
async fn azure_blob_append_blob_default_daily_rotation() {
    assert_append_blob_default_daily_rotation(AzureBlobSinkConfig::new_emulator().await).await;
}

#[tokio::test]
async fn azure_blob_append_blob_default_daily_rotation_with_oauth() {
    assert_append_blob_default_daily_rotation(AzureBlobSinkConfig::new_emulator_with_oauth().await)
        .await;
}

/// Forced multi-flush: a low batch.max_bytes causes Vector to flush many small blocks within a
/// single run. All blocks must land in one append blob and every line must be present.
async fn assert_append_blob_multiple_forced_flushes(config: AzureBlobSinkConfig) {
    let blob_prefix = format!("append/multiflush/{}", random_string(10));

    // Generate enough lines that at ~100 bytes each we get at least 5 forced flushes.
    let line_count = 50;
    let (lines, input) = random_lines_with_stream(100, line_count, None);

    // Rough per-line size: 100 bytes content + framing. Force a flush every ~3 lines.
    let flush_every_n_bytes = 350;

    let mut batch = config.batch;
    batch.max_bytes = Some(flush_every_n_bytes);

    let config = AzureBlobSinkConfig {
        blob_prefix: blob_prefix.clone().try_into().unwrap(),
        blob_type: AzureBlobType::Append,
        blob_time_format: Some(String::new()),
        blob_append_uuid: Some(false),
        batch,
        ..config
    };

    config.run_assert(input).await;

    let blobs = config.list_blobs(blob_prefix).await;
    assert_eq!(
        blobs.len(),
        1,
        "all forced flushes must append to the same blob"
    );
    let (_content_type, _content_encoding, blob_lines) = config.get_blob(blobs[0].clone()).await;
    assert_eq!(
        blob_lines.len(),
        line_count,
        "every flushed line must appear in the blob"
    );
    assert_eq!(blob_lines, lines);
}

#[tokio::test]
async fn azure_blob_append_blob_multiple_forced_flushes() {
    assert_append_blob_multiple_forced_flushes(AzureBlobSinkConfig::new_emulator().await).await;
}

impl AzureBlobSinkConfig {
    pub async fn new_emulator() -> AzureBlobSinkConfig {
        let address = std::env::var("AZURITE_ADDRESS").unwrap_or_else(|_| "localhost".into());
        let config = AzureBlobSinkConfig {
            auth: None,
            connection_string: Some(format!("UseDevelopmentStorage=true;DefaultEndpointsProtocol=http;AccountName=devstoreaccount1;AccountKey=Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==;BlobEndpoint=http://{address}:10000/devstoreaccount1;QueueEndpoint=http://{address}:10001/devstoreaccount1;TableEndpoint=http://{address}:10002/devstoreaccount1;").into()),
            account_name: None,
            blob_endpoint: None,
            container_name: "logs".to_string(),
            blob_prefix: Default::default(),
            blob_time_format: None,
            blob_append_uuid: None,
            blob_type: Default::default(),
            encoding: (None::<FramingConfig>, TextSerializerConfig::default()).into(),
            compression: Compression::None,
            batch: Default::default(),
            request: TowerRequestConfig::default(),
            acknowledgements: Default::default(),
            tls: None,
        };

        config.ensure_container().await;

        config
    }

    pub async fn new_emulator_with_oauth() -> AzureBlobSinkConfig {
        let address = std::env::var("AZURITE_OAUTH_ADDRESS").unwrap_or_else(|_| "localhost".into());
        let config = AzureBlobSinkConfig {
            auth: Some(azure_common::config::AzureAuthentication::MockCredential),
            connection_string: Some(format!("DefaultEndpointsProtocol=https;AccountName=devstoreaccount1;BlobEndpoint=https://{address}:14430/devstoreaccount1;QueueEndpoint=https://{address}:14431/devstoreaccount1;TableEndpoint=https://{address}:14432/devstoreaccount1;").into()),
            account_name: None,
            blob_endpoint: None,
            container_name: "logs".to_string(),
            blob_prefix: Default::default(),
            blob_time_format: None,
            blob_append_uuid: None,
            blob_type: Default::default(),
            encoding: (None::<FramingConfig>, TextSerializerConfig::default()).into(),
            compression: Compression::None,
            batch: Default::default(),
            request: TowerRequestConfig::default(),
            acknowledgements: Default::default(),
            tls: Some(azure_common::config::AzureBlobTlsConfig {
                ca_file: Some(tls::TEST_PEM_CA_PATH.into()),
            }),
        };

        config.ensure_container().await;

        config
    }

    async fn build_test_client(&self) -> Arc<BlobContainerClient> {
        azure_common::config::build_client(
            self.auth.clone(),
            self.connection_string
                .clone()
                .expect("failed to unwrap connection_string")
                .inner()
                .to_string(),
            self.container_name.clone(),
            &crate::config::ProxyConfig::default(),
            self.tls.clone(),
        )
        .await
        .expect("Failed to create client")
    }

    async fn to_sink(&self) -> VectorSink {
        let client = self.build_test_client().await;
        self.build_processor(client).expect("Failed to create sink")
    }

    async fn run_assert(&self, input: impl Stream<Item = EventArray> + Send) {
        // `to_sink` needs to be inside the assertion check
        assert_sink_compliance(
            &SINK_TAGS,
            async move { self.to_sink().await.run(input).await },
        )
        .await
        .expect("Running sink failed");
    }

    pub async fn list_blobs(&self, prefix: String) -> Vec<String> {
        let client = self.build_test_client().await;

        // Iterate pager results and collect blob names. Filter by prefix server-side.
        let mut pager = client
            .list_blobs(None)
            .expect("Failed to start list blobs pager");
        let mut names = Vec::new();
        while let Some(result) = pager.next().await {
            let item = result.expect("Failed to fetch blobs");
            if let Some(name) = item.name
                && name.starts_with(&prefix)
            {
                names.push(name);
            }
        }

        names
    }

    pub async fn get_blob(&self, blob: String) -> (Option<String>, Option<String>, Vec<String>) {
        let client = self.build_test_client().await;

        let blob_client = client.blob_client(&blob);

        // Fetch properties to obtain content-type and content-encoding
        let props_resp = blob_client
            .get_properties(None)
            .await
            .expect("Failed to get blob properties");
        let headers = props_resp.headers();
        let content_type = headers.iter().find_map(|(name, value)| {
            let key = name.as_str();
            if key.eq_ignore_ascii_case("content-type") {
                Some(value.as_str().to_string())
            } else {
                None
            }
        });
        let content_encoding = headers.iter().find_map(|(name, value)| {
            let key = name.as_str();
            if key.eq_ignore_ascii_case("content-encoding") {
                Some(value.as_str().to_string())
            } else {
                None
            }
        });

        // Download blob content (full or first MB as needed)
        let downloaded = blob_client
            .download(None)
            .await
            .expect("Failed to download blob");
        let body_bytes = downloaded
            .body
            .collect()
            .await
            .expect("Failed to read blob body");
        let data = body_bytes.to_vec();

        (content_type, content_encoding, self.get_blob_content(data))
    }

    fn get_blob_content(&self, data: Vec<u8>) -> Vec<String> {
        let body = BytesMut::from(data.as_slice()).freeze().reader();

        if self.compression == Compression::None {
            BufReader::new(body).lines().map(|l| l.unwrap()).collect()
        } else {
            BufReader::new(MultiGzDecoder::new(body))
                .lines()
                .map(|l| l.unwrap())
                .collect()
        }
    }

    async fn ensure_container(&self) {
        let client = self.build_test_client().await;
        let result = client.create(None).await;

        let response = match result {
            Ok(_) => Ok(()),
            Err(error) => match error.http_status() {
                Some(StatusCode::Conflict) => Ok(()),
                _ => Err(error),
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
