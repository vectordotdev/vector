//! Integration tests for the `azure_blob` source, run against Azurite's blob and queue services.
//! Azurite does not run Event Grid, so these tests enqueue synthetic notifications themselves.

use std::{
    num::{NonZeroU64, NonZeroUsize},
    time::Duration,
};

use azure_core::http::{RequestContent, StatusCode};
use azure_storage_blob::models::BlockBlobClientUploadOptions;
use azure_storage_queue::{QueueClient, models::QueueMessage};
use base64::prelude::{BASE64_STANDARD, Engine as _};
use similar_asserts::assert_eq;
use vector_lib::{
    codecs::{JsonDeserializerConfig, decoding::DeserializerConfig},
    lookup::path,
};
use vrl::value::Value;

use tokio::time::Instant;

use super::*;
use crate::{
    SourceSender,
    config::{ComponentKey, ProxyConfig, SourceConfig, SourceContext},
    event::EventStatus::{self, *},
    line_agg,
    sources::util::MultilineConfig,
    test_util::{
        collect_n,
        components::{SOURCE_TAGS, assert_source_compliance},
        lines_from_gzip_file, random_lines, trace_init,
    },
};

/// The notification wire formats an Event Grid subscription can deliver to a Storage Queue.
#[derive(Clone, Copy, Debug)]
enum NotificationFormat {
    /// Event Grid schema, base64-encoded (the format Event Grid itself uses).
    EventGridBase64,
    /// Event Grid schema, raw JSON (manual or test messages).
    EventGridRaw,
    /// CloudEvents 1.0 schema, base64-encoded.
    CloudEventsBase64,
}

fn azurite_address() -> String {
    std::env::var("AZURITE_ADDRESS").unwrap_or_else(|_| "localhost".into())
}

fn connection_string() -> String {
    let address = azurite_address();
    format!(
        "UseDevelopmentStorage=true;DefaultEndpointsProtocol=http;AccountName=devstoreaccount1;AccountKey=Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==;BlobEndpoint=http://{address}:10000/devstoreaccount1;QueueEndpoint=http://{address}:10001/devstoreaccount1;"
    )
}

fn config(
    queue_name: &str,
    multiline: Option<MultilineConfig>,
    log_namespace: bool,
    decoding: DeserializerConfig,
) -> AzureBlobConfig {
    AzureBlobConfig {
        connection_string: Some(connection_string().into()),
        strategy: Strategy::StorageQueue,
        compression: Compression::Auto,
        multiline,
        queue: Some(queue::Config {
            queue_name: queue_name.to_string(),
            poll_secs: 1,
            // Deliberately short: the assertions below shut the source down and wait for the
            // timeout to lapse so that any message left in the queue is visible to `peek_messages`.
            visibility_timeout_secs: 2,
            max_number_of_messages: 10,
            // Serialized on purpose. `collect_n` drops the receiver as soon as it has its events,
            // so a concurrent poller can re-send into a closed pipeline and leave the message
            // undeleted, failing the queue-depth assertions.
            client_concurrency: Some(NonZeroUsize::new(1).expect("nonzero")),
            ..Default::default()
        }),
        acknowledgements: true.into(),
        log_namespace: Some(log_namespace),
        decoding,
        ..Default::default()
    }
}

async fn test_clients(
    config: &AzureBlobConfig,
    queue_name: &str,
) -> (BlobContainerClient, QueueClient, String) {
    let container_name = uuid::Uuid::new_v4().to_string();
    let clients = config
        .create_client_source(&ProxyConfig::default())
        .await
        .expect("Failed to build client source");

    let container_client = clients
        .container_client(&container_name)
        .expect("Failed to build container client");
    match container_client.create(None).await {
        Ok(_) => {}
        Err(error) if error.http_status() == Some(StatusCode::Conflict) => {}
        Err(error) => panic!("Failed to create container: {error}"),
    }

    let queue_client = clients
        .queue_client(queue_name)
        .expect("Failed to build queue client");
    match queue_client.create(None).await {
        Ok(_) => {}
        Err(error) if error.http_status() == Some(StatusCode::Conflict) => {}
        Err(error) => panic!("Failed to create queue: {error}"),
    }

    (container_client, queue_client, container_name)
}

async fn upload_blob(
    container_client: &BlobContainerClient,
    blob_name: &str,
    payload: Vec<u8>,
    content_type: Option<&str>,
    content_encoding: Option<&str>,
) {
    let options = BlockBlobClientUploadOptions {
        blob_content_type: content_type.map(ToOwned::to_owned),
        blob_content_encoding: content_encoding.map(ToOwned::to_owned),
        // Force a single-shot PutBlob. The SDK otherwise splits anything over 4 MiB into
        // PutBlock/PutBlockList, which Azurite rejects under Shared Key auth.
        partition_size: Some(NonZeroU64::new(64 * 1024 * 1024).expect("nonzero")),
        ..Default::default()
    };
    container_client
        .blob_client(blob_name)
        .upload(RequestContent::from(payload), Some(options))
        .await
        .expect("Failed to upload blob");
}

fn notification_body(container: &str, blob: &str, format: NotificationFormat) -> String {
    let address = azurite_address();
    let url = format!("http://{address}:10000/devstoreaccount1/{container}/{blob}");
    let subject = format!("/blobServices/default/containers/{container}/blobs/{blob}");

    let json = match format {
        NotificationFormat::EventGridBase64 | NotificationFormat::EventGridRaw => format!(
            r#"{{
                "topic": "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Storage/storageAccounts/devstoreaccount1",
                "subject": "{subject}",
                "eventType": "Microsoft.Storage.BlobCreated",
                "eventTime": "2026-06-01T12:00:00.000Z",
                "id": "00000000-0000-0000-0000-000000000000",
                "data": {{
                    "api": "PutBlob",
                    "blobType": "BlockBlob",
                    "url": "{url}",
                    "eTag": "0x8DC0000000000000"
                }},
                "dataVersion": "",
                "metadataVersion": "1"
            }}"#
        ),
        NotificationFormat::CloudEventsBase64 => format!(
            r#"{{
                "specversion": "1.0",
                "type": "Microsoft.Storage.BlobCreated",
                "source": "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Storage/storageAccounts/devstoreaccount1",
                "subject": "{subject}",
                "time": "2026-06-01T12:00:00.000Z",
                "id": "00000000-0000-0000-0000-000000000000",
                "data": {{
                    "api": "PutBlob",
                    "blobType": "BlockBlob",
                    "url": "{url}",
                    "eTag": "0x8DC0000000000000"
                }}
            }}"#
        ),
    };

    match format {
        NotificationFormat::EventGridRaw => json,
        NotificationFormat::EventGridBase64 | NotificationFormat::CloudEventsBase64 => {
            BASE64_STANDARD.encode(json)
        }
    }
}

async fn enqueue_notification(queue_client: &QueueClient, body: String) {
    let message = QueueMessage {
        message_text: Some(body),
    };
    queue_client
        .send_message(
            message.try_into().expect("Failed to encode queue message"),
            None,
        )
        .await
        .expect("Failed to enqueue notification");
}

/// Count visible messages without altering their visibility.
async fn count_messages(queue_client: &QueueClient) -> usize {
    queue_client
        .peek_messages(None)
        .await
        .expect("Failed to peek messages")
        .into_model()
        .expect("Failed to decode peeked messages")
        .items
        .map(|items| items.len())
        .unwrap_or(0)
}

#[allow(clippy::too_many_arguments)]
async fn test_event(
    blob_name: Option<String>,
    content_encoding: Option<&str>,
    content_type: Option<&str>,
    multiline: Option<MultilineConfig>,
    payload: Vec<u8>,
    expected_lines: Vec<String>,
    status: EventStatus,
    log_namespace: bool,
    decoding: DeserializerConfig,
    format: NotificationFormat,
    delete_failed_message: bool,
) {
    assert_source_compliance(&SOURCE_TAGS, async move {
        let blob_name = blob_name.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let queue_name = uuid::Uuid::new_v4().to_string();

        let mut config = config(&queue_name, multiline, log_namespace, decoding);
        config.queue.as_mut().unwrap().delete_failed_message = delete_failed_message;

        let (container_client, queue_client, container_name) =
            test_clients(&config, &queue_name).await;

        upload_blob(
            &container_client,
            &blob_name,
            payload,
            content_type,
            content_encoding,
        )
        .await;

        enqueue_notification(
            &queue_client,
            notification_body(&container_name, &blob_name, format),
        )
        .await;

        let (tx, rx) = SourceSender::new_test_finalize(status);
        let key = ComponentKey::from("azure_blob_test");
        let (cx, shutdown) = SourceContext::new_shutdown(&key, tx);
        let namespace = cx.log_namespace(Some(log_namespace));
        let source = config.build(cx).await.unwrap();
        tokio::spawn(async move { source.await.unwrap() });

        let events = collect_n(rx, expected_lines.len()).await;

        assert_eq!(expected_lines.len(), events.len());
        for (i, event) in events.iter().enumerate() {
            if let Some(schema_definition) =
                config.outputs(namespace).pop().unwrap().schema_definition
            {
                schema_definition.is_valid_for_event(event).unwrap();
            }

            let message = expected_lines[i].as_str();

            let log = event.as_log();
            if log_namespace {
                assert_eq!(log.value(), &Value::from(message));
            } else {
                assert_eq!(log["message"], message.into());
            }
            assert_eq!(
                namespace
                    .get_source_metadata(
                        AzureBlobConfig::NAME,
                        log,
                        path!("container"),
                        path!("container")
                    )
                    .unwrap(),
                &container_name.clone().into()
            );
            assert_eq!(
                namespace
                    .get_source_metadata(AzureBlobConfig::NAME, log, path!("blob"), path!("blob"))
                    .unwrap(),
                &blob_name.clone().into()
            );
            assert_eq!(
                namespace
                    .get_source_metadata(
                        AzureBlobConfig::NAME,
                        log,
                        path!("storage_account"),
                        path!("storage_account")
                    )
                    .unwrap(),
                &"devstoreaccount1".into()
            );
        }

        tokio::time::sleep(Duration::from_secs(5)).await;

        shutdown
            .shutdown_all(Some(Instant::now() + Duration::from_secs(3)))
            .await;
        tokio::time::sleep(Duration::from_secs(5)).await;
        match status {
            Errored => {
                assert_eq!(count_messages(&queue_client).await, 1);
            }
            Rejected if !delete_failed_message => {
                assert_eq!(count_messages(&queue_client).await, 1);
            }
            _ => {
                assert_eq!(count_messages(&queue_client).await, 0);
            }
        };
    })
    .await;
}

#[tokio::test]
async fn azure_blob_process_message() {
    trace_init();

    let logs: Vec<String> = random_lines(100).take(10).collect();

    test_event(
        None,
        None,
        None,
        None,
        logs.join("\n").into_bytes(),
        logs,
        Delivered,
        false,
        DeserializerConfig::Bytes,
        NotificationFormat::EventGridBase64,
        true,
    )
    .await;
}

#[tokio::test]
async fn azure_blob_process_message_raw_json_notification() {
    trace_init();

    let logs: Vec<String> = random_lines(100).take(10).collect();

    test_event(
        None,
        None,
        None,
        None,
        logs.join("\n").into_bytes(),
        logs,
        Delivered,
        false,
        DeserializerConfig::Bytes,
        NotificationFormat::EventGridRaw,
        true,
    )
    .await;
}

#[tokio::test]
async fn azure_blob_process_message_cloud_events_notification() {
    trace_init();

    let logs: Vec<String> = random_lines(100).take(10).collect();

    test_event(
        None,
        None,
        None,
        None,
        logs.join("\n").into_bytes(),
        logs,
        Delivered,
        false,
        DeserializerConfig::Bytes,
        NotificationFormat::CloudEventsBase64,
        true,
    )
    .await;
}

#[tokio::test]
async fn azure_blob_process_json_message() {
    trace_init();

    let logs: Vec<String> = random_lines(100).take(10).collect();

    let json_logs: Vec<String> = logs
        .iter()
        .map(|msg| format!(r#"{{"message": "{msg}"}}"#))
        .collect();

    test_event(
        None,
        None,
        None,
        None,
        json_logs.join("\n").into_bytes(),
        logs,
        Delivered,
        false,
        DeserializerConfig::Json(JsonDeserializerConfig::default()),
        NotificationFormat::EventGridBase64,
        true,
    )
    .await;
}

#[tokio::test]
async fn azure_blob_process_message_with_log_namespace() {
    trace_init();

    let logs: Vec<String> = random_lines(100).take(10).collect();

    test_event(
        None,
        None,
        None,
        None,
        logs.join("\n").into_bytes(),
        logs,
        Delivered,
        true,
        DeserializerConfig::Bytes,
        NotificationFormat::EventGridBase64,
        true,
    )
    .await;
}

#[tokio::test]
async fn azure_blob_process_message_special_characters() {
    trace_init();

    let blob_name = format!("special blob {}", uuid::Uuid::new_v4());
    let logs: Vec<String> = random_lines(100).take(10).collect();

    test_event(
        Some(blob_name),
        None,
        None,
        None,
        logs.join("\n").into_bytes(),
        logs,
        Delivered,
        false,
        DeserializerConfig::Bytes,
        NotificationFormat::EventGridBase64,
        true,
    )
    .await;
}

#[tokio::test]
async fn azure_blob_process_message_larger_than_partition_size() {
    trace_init();

    // Larger than the SDK's 4 MiB `DEFAULT_DOWNLOAD_PARTITION_SIZE`, so `BlobClient::download`
    // takes its partitioned path and spawns tasks. Without the `azure_core/tokio` feature those
    // run on plain threads and panic on the connect timeout. Every other test here uses a ~1 KB
    // blob, which fits one partition and never spawns.
    let logs: Vec<String> = random_lines(100).take(60_000).collect();

    test_event(
        None,
        None,
        None,
        None,
        logs.join("\n").into_bytes(),
        logs,
        Delivered,
        false,
        DeserializerConfig::Bytes,
        NotificationFormat::EventGridBase64,
        true,
    )
    .await;
}

#[tokio::test]
async fn azure_blob_process_message_gzip() {
    use std::io::Read;

    trace_init();

    let logs: Vec<String> = random_lines(100).take(10).collect();

    let mut gz = flate2::read::GzEncoder::new(
        std::io::Cursor::new(logs.join("\n").into_bytes()),
        flate2::Compression::fast(),
    );
    let mut buffer = Vec::new();
    gz.read_to_end(&mut buffer).unwrap();

    test_event(
        None,
        Some("gzip"),
        None,
        None,
        buffer,
        logs,
        Delivered,
        false,
        DeserializerConfig::Bytes,
        NotificationFormat::EventGridBase64,
        true,
    )
    .await;
}

#[tokio::test]
async fn azure_blob_process_message_multipart_gzip() {
    use std::io::Read;

    trace_init();

    let logs = lines_from_gzip_file("tests/data/multipart-gzip.log.gz");

    let buffer = {
        let mut file =
            std::fs::File::open("tests/data/multipart-gzip.log.gz").expect("file can be opened");
        let mut data = Vec::new();
        file.read_to_end(&mut data).expect("file can be read");
        data
    };

    test_event(
        None,
        Some("gzip"),
        None,
        None,
        buffer,
        logs,
        Delivered,
        false,
        DeserializerConfig::Bytes,
        NotificationFormat::EventGridBase64,
        true,
    )
    .await;
}

#[tokio::test]
async fn azure_blob_process_message_multipart_zstd() {
    use std::io::{BufRead, BufReader, Read};

    trace_init();

    let logs: Vec<String> = {
        let file = std::fs::File::open("tests/data/multipart-zst.log").expect("file can be opened");
        BufReader::new(file).lines().map(|x| x.unwrap()).collect()
    };

    let buffer = {
        let mut file =
            std::fs::File::open("tests/data/multipart-zst.log.zst").expect("file can be opened");
        let mut data = Vec::new();
        file.read_to_end(&mut data).expect("file can be read");
        data
    };

    test_event(
        None,
        Some("zstd"),
        None,
        None,
        buffer,
        logs,
        Delivered,
        false,
        DeserializerConfig::Bytes,
        NotificationFormat::EventGridBase64,
        true,
    )
    .await;
}

#[tokio::test]
async fn azure_blob_process_message_multiline() {
    trace_init();

    let logs: Vec<String> = vec!["abc", "def", "geh"]
        .into_iter()
        .map(ToOwned::to_owned)
        .collect();

    test_event(
        None,
        None,
        None,
        Some(MultilineConfig {
            start_pattern: "abc".to_owned(),
            mode: line_agg::Mode::HaltWith,
            condition_pattern: "geh".to_owned(),
            timeout_ms: Duration::from_millis(1000),
        }),
        logs.join("\n").into_bytes(),
        vec!["abc\ndef\ngeh".to_owned()],
        Delivered,
        false,
        DeserializerConfig::Bytes,
        NotificationFormat::EventGridBase64,
        true,
    )
    .await;
}

#[tokio::test]
async fn azure_blob_handles_failed_status() {
    trace_init();

    let logs: Vec<String> = random_lines(100).take(10).collect();

    test_event(
        None,
        None,
        None,
        None,
        logs.join("\n").into_bytes(),
        logs,
        Rejected,
        false,
        DeserializerConfig::Bytes,
        NotificationFormat::EventGridBase64,
        true,
    )
    .await;
}

#[tokio::test]
async fn azure_blob_handles_failed_status_without_deletion() {
    trace_init();

    let logs: Vec<String> = random_lines(100).take(10).collect();

    test_event(
        None,
        None,
        None,
        None,
        logs.join("\n").into_bytes(),
        logs,
        Rejected,
        false,
        DeserializerConfig::Bytes,
        NotificationFormat::EventGridBase64,
        false,
    )
    .await;
}

#[tokio::test]
async fn azure_blob_ignores_other_event_types() {
    trace_init();

    let queue_name = uuid::Uuid::new_v4().to_string();
    let config = config(&queue_name, None, false, DeserializerConfig::Bytes);
    let (_container_client, queue_client, container_name) =
        test_clients(&config, &queue_name).await;

    let body = notification_body(
        &container_name,
        "some.log",
        NotificationFormat::EventGridRaw,
    )
    .replace(
        "Microsoft.Storage.BlobCreated",
        "Microsoft.Storage.BlobDeleted",
    );
    enqueue_notification(&queue_client, BASE64_STANDARD.encode(body)).await;

    let (tx, _rx) = SourceSender::new_test_finalize(Delivered);
    let cx = SourceContext::new_test(tx, None);
    let source = config.build(cx).await.unwrap();
    tokio::spawn(async move { source.await.unwrap() });

    // The ignored message must be deleted from the queue, not redelivered.
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert_eq!(count_messages(&queue_client).await, 0);
}
