use std::{
    io::{BufRead, BufReader, Cursor},
    panic,
    sync::Arc,
};

use anyhow::anyhow;
use async_stream::stream;
use azure_storage_blobs::prelude::ContainerClient;
use azure_storage_queues::{operations::Message, QueueClient};
use base64::{prelude::BASE64_STANDARD, Engine};
use futures::stream::StreamExt;
use serde::Deserialize;
use serde_with::serde_as;
use tokio::{select, time};

use vector_lib::{
    configurable::configurable_component,
    internal_event::{ByteSize, BytesReceived, InternalEventHandle, Protocol, Registered},
};

use crate::{
    azure,
    shutdown::ShutdownSignal,
    sources::azure_blob::{AzureBlobConfig, BlobPack, BlobPackStream},
};

/// Azure Queue configuration options.
#[serde_as]
#[configurable_component]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub(super) struct Config {
    /// The name of the storage queue to poll for events.
    pub(super) queue_name: String,

    /// How long to wait while polling the event grid queue for new messages, in seconds.
    ///
    // NOTE: We restrict this to u32 for safe conversion to i32 later.
    #[serde(default = "default_poll_secs")]
    #[derivative(Default(value = "default_poll_secs()"))]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    pub(super) poll_secs: u32,
}

pub fn make_azure_row_stream(
    cfg: &AzureBlobConfig,
    shutdown: ShutdownSignal,
) -> crate::Result<BlobPackStream> {
    let queue_client = make_queue_client(cfg)?;
    let container_client = make_container_client(cfg)?;
    let bytes_received = register!(BytesReceived::from(Protocol::HTTP));
    let poll_interval = std::time::Duration::from_secs(
        cfg.queue
            .as_ref()
            .ok_or(anyhow!("Missing Event Grid queue config."))?
            .poll_secs as u64,
    );

    Ok(Box::pin(stream! {
        // TODO: add a way to stop this loop, possibly with shutdown
        loop {
            let messages = match queue_client.get_messages().await {
                Ok(messages) => messages,
                Err(e) => {
                    error!("Failed reading messages: {}", e); // TODO: consider emit!
                    continue;
                }
            };

            for message in messages.messages {
                let msg_id = message.message_id.clone();
                match proccess_event_grid_message(
                    message,
                    &container_client,
                    &queue_client,
                    bytes_received.clone()
                ).await {
                    Some(blob_pack) => yield blob_pack,
                    None => trace!("Message {msg_id} failed to be processed or is ignored, \
                            no blob stream stream created from it. \
                            Will retry on next message."),
                }
            }
            // allow shutdown to break sleeping
            select! {
                _ = shutdown.clone() => {
                    info!("Shutdown signal received, terminating azure row stream.");
                    break;
                },
                _ = time::sleep(poll_interval) => { }
            }
        }
    }))
}

pub fn make_queue_client(cfg: &AzureBlobConfig) -> crate::Result<Arc<QueueClient>> {
    let q = cfg.queue.clone().ok_or("Missing queue.")?;
    azure::build_queue_client(
        cfg.connection_string
            .as_ref()
            .map(|v| v.inner().to_string()),
        cfg.storage_account.as_ref().map(|v| v.to_string()),
        q.queue_name.clone(),
        cfg.endpoint.clone(),
        cfg.client_credentials.clone(),
    )
}

pub fn make_container_client(cfg: &AzureBlobConfig) -> crate::Result<Arc<ContainerClient>> {
    azure::build_container_client(
        cfg.connection_string
            .as_ref()
            .map(|v| v.inner().to_string()),
        cfg.storage_account.as_ref().map(|v| v.to_string()),
        cfg.container_name.clone(),
        cfg.endpoint.clone(),
        cfg.client_credentials.clone(),
    )
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AzureStorageEvent {
    pub subject: String,
    pub event_type: String,
}

async fn proccess_event_grid_message(
    message: Message,
    container_client: &ContainerClient,
    queue_client: &QueueClient,
    bytes_received: Registered<BytesReceived>,
) -> Option<BlobPack> {
    let decoded_bytes = match BASE64_STANDARD.decode(&message.message_text) {
        Ok(decoded) => decoded,
        Err(e) => {
            error!("Failed decoding base64: {}", e);
            return None;
        }
    };
    let decoded_string = match String::from_utf8(decoded_bytes) {
        Ok(decoded) => decoded,
        Err(e) => {
            error!("Failed decoding utf8: {}", e);
            return None;
        }
    };
    let body: AzureStorageEvent = match serde_json::from_str(decoded_string.as_str()) {
        Ok(body) => body,
        Err(e) => {
            error!("Failed decoding json: {}", e);
            return None;
        }
    };
    if body.event_type != "Microsoft.Storage.BlobCreated" {
        trace!(
            "Ignoring event because of wrong event type: {}",
            body.event_type
        );
        return None;
    }
    match parse_subject(body.subject) {
        Some((container, blob)) => {
            if container != container_client.container_name() {
                trace!("Container name does not match. Skipping.");
                return None;
            }
            trace!(
                "New blob created in container '{}': '{}'",
                &container,
                &blob
            );
            let blob_client = container_client.blob_client(blob);
            let mut result: Vec<u8> = vec![];
            let mut stream = blob_client.get().into_stream();
            while let Some(value) = stream.next().await {
                match value {
                    Ok(response) => {
                        let mut body = response.data;
                        while let Some(value) = body.next().await {
                            match value {
                                Ok(chunk) => result.extend(&chunk),
                                Err(e) => {
                                    // This should now happen as long as `next()` is working
                                    // correctly. Leaving just a safeguard, not to crash Vector.
                                    trace!("Failed to read body chunk: {}", e);
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        // Logging as trace, as it will be retired.
                        trace!("Failed to get response: {}", e);
                        return None;
                    }
                }
            }

            let reader = Cursor::new(result);
            let buffered = BufReader::new(reader);
            let queue_client_copy = queue_client.clone();
            let bytes_received_copy = bytes_received.clone();

            Some(BlobPack {
                row_stream: Box::pin(stream! {
                    for line in buffered.lines() {
                        let line = line.map(|line| line.as_bytes().to_vec());
                        let line = match line {
                            Ok(l) => l,
                            Err(e) => {
                                error!("Failed to map line: {}", e);
                                break;
                            }
                        };
                        bytes_received_copy.emit(ByteSize(line.len()));
                        yield line;
                    }
                }),
                success_handler: Box::new(|| {
                    Box::pin(async move {
                        let res = queue_client_copy.pop_receipt_client(message).delete().await;

                        match res {
                            Ok(_) => {}
                            Err(e) => {
                                error!("Failed removing messages from queue: {}", e);
                            }
                        }
                    })
                }),
            })
        }
        None => {
            warn!("Failed parsing subject. Skipping.");
            return None;
        }
    }
}

fn parse_subject(subject: String) -> Option<(String, String)> {
    let parts: Vec<&str> = subject.split('/').collect();
    if parts.len() < 7 {
        warn!("Ignoring event because of wrong subject format");
        return None;
    }
    let container = parts[4];
    let blob = parts[6..].join("/");
    Some((container.to_string(), blob))
}

const fn default_poll_secs() -> u32 {
    15
}

#[test]
fn test_azure_storage_event() {
    let event_value: AzureStorageEvent = serde_json::from_str(
        r#"{
          "topic": "/subscriptions/fa5f2180-1451-4461-9b1f-aae7d4b33cf8/resourceGroups/events_poc/providers/Microsoft.Storage/storageAccounts/eventspocaccount",
          "subject": "/blobServices/default/containers/content/blobs/foo",
          "eventType": "Microsoft.Storage.BlobCreated",
          "id": "be3f21f7-201e-000b-7605-a29195062628",
          "data": {
            "api": "PutBlob",
            "clientRequestId": "1fa42c94-6dd3-4172-95c4-fd9cf56b5009",
            "requestId": "be3f21f7-201e-000b-7605-a29195000000",
            "eTag": "0x8DC701C5D3FFDF6",
            "contentType": "application/octet-stream",
            "contentLength": 0,
            "blobType": "BlockBlob",
            "url": "https://eventspocaccount.blob.core.windows.net/content/foo",
            "sequencer": "0000000000000000000000000005C5360000000000276a63",
            "storageDiagnostics": {
              "batchId": "fec5b12c-2006-0034-0005-a25936000000"
            }
          },
          "dataVersion": "",
          "metadataVersion": "1",
          "eventTime": "2024-05-09T11:37:10.5637878Z"
        }"#,
    ).unwrap();

    assert_eq!(
        event_value.subject,
        "/blobServices/default/containers/content/blobs/foo".to_string()
    );
    assert_eq!(
        event_value.event_type,
        "Microsoft.Storage.BlobCreated".to_string()
    );
}

#[test]
fn test_parse_subject_no_dir() {
    let subject = "/blobServices/default/containers/content/blobs/foo".to_string();
    let result = parse_subject(subject);
    assert_eq!(result, Some(("content".to_string(), "foo".to_string())));
}

#[test]
fn test_parse_subject_with_dirs() {
    let subject = "/blobServices/default/containers/insights-logs-signinlogs/blobs/tenantId=0e35ee7a-425d-45a5-9013-218c1eae8fd4/y=2024/m=06/d=20/h=05/m=00/PT1H.json".to_string();
    let result = parse_subject(subject);
    assert_eq!(
        result,
        Some((
            "insights-logs-signinlogs".to_string(),
            "tenantId=0e35ee7a-425d-45a5-9013-218c1eae8fd4/y=2024/m=06/d=20/h=05/m=00/PT1H.json"
                .to_string()
        ))
    );
}
