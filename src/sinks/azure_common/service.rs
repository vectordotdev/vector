use std::{
    result::Result as StdResult,
    sync::Arc,
    task::{Context, Poll},
};

use azure_core::HttpError;
use azure_storage_blobs::prelude::*;
use futures::{future::BoxFuture, TryFutureExt};
use tower::Service;
use tracing_futures::Instrument;

use crate::{
    internal_events::azure_blob::{AzureBlobErrorResponse, AzureBlobEventSent, AzureBlobHttpError},
    sinks::azure_common::config::{AzureBlobRequest, AzureBlobResponse},
};

#[derive(Clone)]
pub struct AzureBlobService {
    pub client: Arc<ContainerClient>,
}

impl AzureBlobService {
    pub const fn new(client: Arc<ContainerClient>) -> AzureBlobService {
        AzureBlobService { client }
    }
}

impl Service<AzureBlobRequest> for AzureBlobService {
    type Response = AzureBlobResponse;
    type Error = Box<dyn std::error::Error + std::marker::Send + std::marker::Sync>;
    type Future = BoxFuture<'static, StdResult<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<StdResult<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: AzureBlobRequest) -> Self::Future {
        let client =
            Arc::clone(&self.client).as_blob_client(request.metadata.partition_key.as_str());

        Box::pin(async move {
            let byte_size = request.blob_data.len();
            let blob = client
                .put_block_blob(request.blob_data)
                .content_type(request.content_type);
            let blob = match request.content_encoding {
                Some(encoding) => blob.content_encoding(encoding),
                None => blob,
            };

            let result = blob
                .execute()
                .inspect_err(|reason| {
                    match reason.downcast_ref::<HttpError>() {
                        Some(HttpError::StatusCode { status, .. }) => {
                            emit!(&AzureBlobErrorResponse { code: *status })
                        }
                        _ => emit!(&AzureBlobHttpError {
                            error: reason.to_string()
                        }),
                    };
                })
                .inspect_ok(|result| {
                    emit!(&AzureBlobEventSent {
                        request_id: result.request_id,
                        byte_size
                    })
                })
                .instrument(info_span!("request"))
                .await;

            result.map(|inner| AzureBlobResponse {
                inner,
                count: request.metadata.count,
                events_byte_size: request.metadata.byte_size,
            })
        })
    }
}
