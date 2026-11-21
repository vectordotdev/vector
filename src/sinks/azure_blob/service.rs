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
    AppendBlobClient, BlobClient, BlobContainerClient,
    models::{AppendBlobClientCreateOptions, BlockBlobClientUploadOptions, StorageErrorCode},
};
use bytes::Bytes;
use futures::future::BoxFuture;
use tower::Service;
use tracing::Instrument;

use crate::sinks::azure_blob::config::{AzureBlobRequest, AzureBlobResponse, AzureBlobType};

#[derive(Clone)]
pub struct AzureBlobService {
    // Using the new azure_storage_blob container client.
    client: Arc<BlobContainerClient>,
    // Whether block blob names end in a UUID, which makes each name unique to one request.
    unique_blob_names: bool,
}

impl AzureBlobService {
    #[must_use]
    pub const fn new(
        client: Arc<BlobContainerClient>,
        unique_blob_names: bool,
    ) -> AzureBlobService {
        AzureBlobService {
            client,
            unique_blob_names,
        }
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
                        request.tags,
                        request.blob_metadata,
                        this.unique_blob_names,
                    )
                    .await
                }
                AzureBlobType::Append => {
                    append_blob(
                        &blob_client.append_blob_client(),
                        request.blob_data,
                        request.content_type,
                        request.content_encoding,
                        request.tags,
                        request.blob_metadata,
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
    content_type: &'static str,
    content_encoding: Option<&'static str>,
    tags: Option<String>,
    blob_metadata: Option<std::collections::HashMap<String, String>>,
    unique_blob_names: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let upload_options = BlockBlobClientUploadOptions {
        blob_content_type: Some(content_type.to_string()),
        blob_content_encoding: content_encoding.map(str::to_string),
        blob_tags_string: tags,
        metadata: blob_metadata,
        ..Default::default()
    }
    .if_not_exists();

    match blob_client
        .upload(RequestContent::from(data.to_vec()), Some(upload_options))
        .instrument(info_span!("request").or_current())
        .await
    {
        Ok(_) => Ok(()),
        Err(error) if unique_blob_names && is_existing_blob_conflict(&error) => {
            // The upload is conditional on the blob not existing, and the name ends in a UUID
            // generated for this request, so the existing blob was written by an earlier attempt
            // of this same request whose response was lost (a timeout, a connection reset, or a
            // retry inside the Azure SDK). Uploads are atomic, so that blob holds this payload.
            debug!(
                message = "Blob already exists from an earlier attempt of this request; treating as delivered.",
                error = %error,
            );
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

/// Whether `error` is the service rejecting a conditional (`If-None-Match: *`) upload because the
/// blob already exists.
///
/// A single-shot upload (`Put Blob`) reports this as `409 BlobAlreadyExists`; an upload large
/// enough to be staged in blocks ends in `Put Block List`, which may report it as
/// `412 ConditionNotMet`. No other precondition is set on the upload, so both mean the same thing.
pub(super) fn is_existing_blob_conflict(error: &azure_core::Error) -> bool {
    let error_code = storage_error_code(error);
    match error.http_status() {
        Some(StatusCode::Conflict) => {
            error_code.as_deref() == Some(StorageErrorCode::BlobAlreadyExists.as_ref())
        }
        Some(StatusCode::PreconditionFailed) => {
            error_code.as_deref() == Some(StorageErrorCode::ConditionNotMet.as_ref())
        }
        _ => false,
    }
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
//
// Tags and metadata are applied at blob creation time only. Azure's `Set Blob Metadata` /
// `Set Blob Tags` are separate REST calls that operate on the whole blob (not per-append), and
// applying them from Vector on every append would race with concurrent creators and Azurite
// integration testing — so this sink treats them as one-time initialization data.
async fn append_blob(
    append_client: &AppendBlobClient,
    data: Bytes,
    content_type: &'static str,
    content_encoding: Option<&'static str>,
    tags: Option<String>,
    blob_metadata: Option<std::collections::HashMap<String, String>>,
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
                        No further data can be appended to this blob until `blob_time_format` \
                        rolls over to a new name. Set a finer-grained `blob_time_format` (the \
                        default rotates hourly) so blobs rotate before the cap is reached, \
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
        blob_tags_string: tags,
        metadata: blob_metadata,
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
        .map_err(Into::into)
}
