use std::{
    result::Result as StdResult,
    sync::Arc,
    task::{Context, Poll},
};

use azure_core::{
    error::ErrorKind,
    http::{RequestContent, StatusCode},
};
use azure_storage_blob::{
    BlobClient, BlobContainerClient,
    models::{AppendBlobClientCreateOptions, BlockBlobClientUploadOptions, StorageErrorCode},
};
use bytes::Bytes;
use futures::future::BoxFuture;
use tower::Service;
use tracing::Instrument;

use crate::sinks::azure_common::config::{AzureBlobRequest, AzureBlobResponse, AzureBlobType};

#[derive(Clone)]
pub struct AzureBlobService {
    // Using the new azure_storage_blob container client.
    client: Arc<BlobContainerClient>,
}

impl AzureBlobService {
    pub const fn new(client: Arc<BlobContainerClient>) -> AzureBlobService {
        AzureBlobService { client }
    }
}

impl Service<AzureBlobRequest> for AzureBlobService {
    type Response = AzureBlobResponse;
    type Error = Box<dyn std::error::Error + std::marker::Send + std::marker::Sync>;
    type Future = BoxFuture<'static, StdResult<Self::Response, Self::Error>>;

    // Emission of an internal event in case of errors is handled upstream by the caller.
    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<StdResult<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of internal events for errors and dropped events is handled upstream by the caller.
    fn call(&mut self, request: AzureBlobRequest) -> Self::Future {
        let this = self.clone();

        Box::pin(async move {
            let blob_client = this
                .client
                .blob_client(request.metadata.partition_key.as_str());
            let byte_size = request.blob_data.len();

            let result = match request.blob_type {
                AzureBlobType::Block => {
                    upload_block_blob(
                        &blob_client,
                        request.blob_data,
                        request.content_type,
                        request.content_encoding,
                    )
                    .await
                }
                AzureBlobType::Append => {
                    append_blob(
                        &blob_client.append_blob_client(),
                        request.blob_data,
                        request.content_type,
                        request.content_encoding,
                    )
                    .await
                }
            };

            result.map(|()| AzureBlobResponse {
                events_byte_size: request
                    .request_metadata
                    .into_events_estimated_json_encoded_byte_size(),
                byte_size,
            })
        })
    }
}

async fn upload_block_blob(
    blob_client: &BlobClient,
    data: Bytes,
    content_type: &str,
    content_encoding: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let upload_options = BlockBlobClientUploadOptions {
        blob_content_type: Some(content_type.to_string()),
        blob_content_encoding: content_encoding.map(str::to_string),
        ..Default::default()
    }
    .if_not_exists();

    blob_client
        .upload(RequestContent::from(data.to_vec()), Some(upload_options))
        .instrument(info_span!("request").or_current())
        .await
        .map(|_| ())
        .map_err(|e| e.into())
}

// Extracts the Azure storage error code string from an azure_core::Error, if present.
fn storage_error_code(e: &azure_core::Error) -> Option<String> {
    match e.kind() {
        ErrorKind::HttpResponse { error_code, .. } => error_code.clone(),
        _ => None,
    }
}

// Appends `data` to an existing append blob, creating the blob first if it doesn't exist.
// Uses the EAFP pattern: attempt append, create on 404, retry once.
// A 409 Conflict on create is swallowed — it means a concurrent writer created the blob first.
async fn append_blob(
    append_client: &azure_storage_blob::AppendBlobClient,
    data: Bytes,
    content_type: &str,
    content_encoding: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let data_len = data.len() as u64;

    match append_client
        .append_block(RequestContent::from(data.to_vec()), data_len, None)
        .instrument(info_span!("request").or_current())
        .await
    {
        Ok(_) => return Ok(()),
        Err(e) => {
            let error_code = storage_error_code(&e);

            if e.http_status() == Some(StatusCode::NotFound) {
                if error_code.as_deref() == Some(StorageErrorCode::ContainerNotFound.as_ref()) {
                    // Container doesn't exist — this is a misconfiguration, not a missing blob.
                    // Propagate immediately; creating the blob would also fail.
                    warn!(
                        message = "Azure container not found when appending to blob. \
                        Verify the container exists and that `container_name` is correct.",
                    );
                    return Err(e.into());
                }
                // BlobNotFound (or unrecognised 404) — first write, fall through to create.
            } else {
                if error_code.as_deref() == Some(StorageErrorCode::BlockCountExceedsLimit.as_ref())
                {
                    warn!(
                        message = "Azure append blob has reached the 50,000-block limit. \
                        No further data can be appended to this blob. \
                        Configure `blob_time_format` for time-based rotation, \
                        or delete the full blob manually.",
                    );
                }
                return Err(e.into());
            }
        }
    }

    let create_opts = AppendBlobClientCreateOptions {
        blob_content_type: Some(content_type.to_string()),
        blob_content_encoding: content_encoding.map(str::to_string),
        ..Default::default()
    }
    .if_not_exists();

    match append_client
        .create(Some(create_opts))
        .instrument(info_span!("request").or_current())
        .await
    {
        Ok(_) => {}
        Err(e) if e.http_status() == Some(StatusCode::Conflict) => {} // race: already created
        Err(e) => return Err(e.into()),
    }

    append_client
        .append_block(RequestContent::from(data.to_vec()), data_len, None)
        .instrument(info_span!("request").or_current())
        .await
        .map(|_| ())
        .map_err(|e| e.into())
}
