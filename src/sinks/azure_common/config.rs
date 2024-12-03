use std::sync::Arc;

use azure_core::error::HttpError;
use azure_storage_blobs::{blob::operations::PutBlockBlobResponse, prelude::*};
use bytes::Bytes;
use futures::FutureExt;
use http::StatusCode;
use snafu::Snafu;
use vector_lib::stream::DriverResponse;
use vector_lib::{
    json_size::JsonSize,
    request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata},
};

use crate::{
    event::{EventFinalizers, EventStatus, Finalizable},
    sinks::{util::retries::RetryLogic, Healthcheck},
};

#[derive(Debug, Clone)]
pub struct AzureBlobRequest {
    pub blob_data: Bytes,
    pub content_encoding: Option<&'static str>,
    pub content_type: &'static str,
    pub metadata: AzureBlobMetadata,
    pub request_metadata: RequestMetadata,
}

impl Finalizable for AzureBlobRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.metadata.finalizers)
    }
}

impl MetaDescriptive for AzureBlobRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.request_metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.request_metadata
    }
}

#[derive(Clone, Debug)]
pub struct AzureBlobMetadata {
    pub partition_key: String,
    pub count: usize,
    pub byte_size: JsonSize,
    pub finalizers: EventFinalizers,
}

#[derive(Debug, Clone)]
pub struct AzureBlobRetryLogic;

impl RetryLogic for AzureBlobRetryLogic {
    type Error = HttpError;
    type Response = AzureBlobResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        error.status().is_server_error()
            || StatusCode::TOO_MANY_REQUESTS.as_u16() == Into::<u16>::into(error.status())
    }
}

#[derive(Debug)]
pub struct AzureBlobResponse {
    pub inner: PutBlockBlobResponse,
    pub events_byte_size: GroupedCountByteSize,
    pub byte_size: usize,
}

impl DriverResponse for AzureBlobResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.events_byte_size
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.byte_size)
    }
}

#[derive(Debug, Snafu)]
pub enum HealthcheckError {
    #[snafu(display("Invalid connection string specified"))]
    InvalidCredentials,
    #[snafu(display("Container: {:?} not found", container))]
    UnknownContainer { container: String },
    #[snafu(display("Unknown status code: {}", status))]
    Unknown { status: StatusCode },
}

pub fn build_healthcheck(
    container_name: String,
    client: Arc<ContainerClient>,
) -> crate::Result<Healthcheck> {
    let healthcheck = async move {
        let response = client.get_properties().into_future().await;

        let resp: crate::Result<()> = match response {
            Ok(_) => Ok(()),
            Err(reason) => Err(match reason.downcast_ref::<HttpError>() {
                Some(err) => match StatusCode::from_u16(err.status().into()) {
                    Ok(StatusCode::FORBIDDEN) => Box::new(HealthcheckError::InvalidCredentials),
                    Ok(StatusCode::NOT_FOUND) => Box::new(HealthcheckError::UnknownContainer {
                        container: container_name,
                    }),
                    Ok(status) => Box::new(HealthcheckError::Unknown { status }),
                    Err(_) => "unknown status code".into(),
                },
                _ => reason.into(),
            }),
        };
        resp
    };

    Ok(healthcheck.boxed())
}
