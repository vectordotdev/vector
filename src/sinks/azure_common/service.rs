use std::{
    result::Result as StdResult,
    sync::Arc,
    task::{Context, Poll},
};

use azure_storage_blobs::prelude::*;
use futures::future::BoxFuture;
use tower::Service;
use tracing::Instrument;

use crate::sinks::azure_common::config::{AzureBlobRequest, AzureBlobResponse};

#[derive(Clone)]
pub struct AzureBlobService {
    client: Arc<ContainerClient>,
}

impl AzureBlobService {
    pub fn new(client: Arc<ContainerClient>) -> AzureBlobService {
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
            let client = this
                .client
                .blob_client(request.metadata.partition_key.as_str());
            let byte_size = request.blob_data.len();
            let blob = client
                .put_block_blob(request.blob_data)
                .content_type(request.content_type);
            let blob = match request.content_encoding {
                Some(encoding) => blob.content_encoding(encoding),
                None => blob,
            };

            let result = blob
                .into_future()
                .instrument(info_span!("request").or_current())
                .await
                .map_err(|err| err.into());

            result.map(|inner| AzureBlobResponse {
                inner,
                events_byte_size: request
                    .request_metadata
                    .into_events_estimated_json_encoded_byte_size(),
                byte_size,
            })
        })
    }
}
