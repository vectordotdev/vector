use std::{
    result::Result as StdResult,
    sync::Arc,
    task::{Context, Poll},
};

use azure_core::http::RequestContent;
use azure_storage_blob::{BlobContainerClient, models::BlockBlobClientUploadOptions};
use futures::future::BoxFuture;
use tower::Service;
use tracing::Instrument;

use crate::sinks::azure_common::config::{AzureBlobRequest, AzureBlobResponse};

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
            let upload_options = BlockBlobClientUploadOptions {
                blob_content_type: Some(request.content_type.to_string()),
                blob_content_encoding: request.content_encoding.map(|e| e.to_string()),
                ..Default::default()
            };

            let result = blob_client
                .upload(
                    RequestContent::from(request.blob_data.to_vec()),
                    false,
                    byte_size as u64,
                    Some(upload_options),
                )
                .instrument(info_span!("request").or_current())
                .await
                .map_err(|err| err.into());

            result.map(|_resp| AzureBlobResponse {
                inner: (),
                events_byte_size: request
                    .request_metadata
                    .into_events_estimated_json_encoded_byte_size(),
                byte_size,
            })
        })
    }
}
