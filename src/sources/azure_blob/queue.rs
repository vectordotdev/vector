use std::{
    io::{BufRead, BufReader, Cursor},
    panic,
    sync::Arc,
};

use async_stream::stream;
use azure_storage_blobs::prelude::ContainerClient;
use azure_storage_queues::{operations::Message, QueueClient};
use base64::{prelude::BASE64_STANDARD, Engine};
use futures::stream::StreamExt;
use serde::Deserialize;
use serde_with::serde_as;

use vector_lib::internal_event::{
    ByteSize, BytesReceived, InternalEventHandle, Protocol, Registered,
};

use crate::{
    azure,
    sinks::prelude::configurable_component,
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
}

pub fn make_azure_row_stream(cfg: &AzureBlobConfig) -> crate::Result<BlobPackStream> {
    let queue_client = make_queue_client(cfg)?;
    let container_client = make_container_client(cfg)?;
    let bytes_received = register!(BytesReceived::from(Protocol::HTTP));

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
                    None => info!("Message {msg_id} failed to be processed, \
                            no blob stream stream created from it. \
                            Will retry on next message."),
                }
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
    // TODO get the event type const from library?
    if body.event_type != "Microsoft.Storage.BlobCreated" {
        warn!(
            "Ignoring event because of wrong event type: {}",
            body.event_type
        );
        return None;
    }
    // TODO some smarter parsing should be done here
    let parts = body.subject.split("/").collect::<Vec<_>>();
    let container = parts[4];
    // TODO here we'd like to check if container matches the container
    // from config.
    let blob = parts[6];
    info!(
        "New blob created in container '{}': '{}'",
        &container, &blob
    );

    let blob_client = container_client.blob_client(blob);
    let mut result: Vec<u8> = vec![];
    let mut stream = blob_client.get().into_stream();
    while let Some(value) = stream.next().await {
        let mut body = value.unwrap().data;
        while let Some(value) = body.next().await {
            let value = value.expect("Failed to read body chunk");
            result.extend(&value);
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
                let line = line.expect("ASDF");
                bytes_received_copy.emit(ByteSize(line.len()));
                yield line;
            }
        }),
        success_handler: Box::new(|| {
            Box::pin(async move {
                queue_client_copy
                    .pop_receipt_client(message)
                    .delete()
                    .await
                    .expect("Failed removing messages from queue");
            })
        }),
    })
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
