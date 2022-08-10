use std::{
    result::Result as StdResult,
    sync::Arc,
    task::{Context, Poll},
};

use azure_core::HttpError;
use azure_storage_blobs::prelude::*;
use futures::{future::BoxFuture, TryFutureExt};
use tower::Service;
use tracing::Instrument;
use vector_common::internal_event::{
    ByteSize, BytesSent, InternalEventHandle, Protocol, Registered,
};

use crate::{
    internal_events::azure_blob::{AzureBlobHttpError, AzureBlobResponseError},
    sinks::azure_common::config::{AzureBlobRequest, AzureBlobResponse},
};

#[derive(Clone)]
pub(crate) struct AzureBlobService {
    client: Arc<ContainerClient>,
    bytes_sent: Registered<BytesSent>,
}

impl AzureBlobService {
    pub fn new(client: Arc<ContainerClient>) -> AzureBlobService {
        let bytes_sent = register!(BytesSent::from(Protocol::HTTPS));
        AzureBlobService { client, bytes_sent }
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
        let this = self.clone();

        Box::pin(async move {
            let client = this
                .client
                .as_blob_client(request.metadata.partition_key.as_str());
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
                            emit!(AzureBlobResponseError::from(*status))
                        }
                        _ => emit!(AzureBlobHttpError {
                            error: reason.to_string()
                        }),
                    };
                })
                .inspect_ok(|_| this.bytes_sent.emit(ByteSize(byte_size)))
                .instrument(info_span!("request").or_current())
                .await;

            result.map(|inner| AzureBlobResponse {
                inner,
                count: request.metadata.count,
                events_byte_size: request.metadata.byte_size,
            })
        })
    }
}
