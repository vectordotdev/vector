use std::sync::Arc;

use azure_core::{error::HttpError, RetryOptions};
use azure_identity::{AutoRefreshingTokenCredential, DefaultAzureCredential};
use azure_storage::{prelude::*, CloudLocation, ConnectionString};
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

pub fn build_client(
    connection_string: Option<String>,
    storage_account: Option<String>,
    container_name: String,
    endpoint: Option<String>,
) -> crate::Result<Arc<ContainerClient>> {
    let client;
    match (connection_string, storage_account) {
        (Some(connection_string_p), None) => {
            let connection_string = ConnectionString::new(&connection_string_p)?;

            client = match connection_string.blob_endpoint {
                // When the blob_endpoint is provided, we use the Custom CloudLocation since it is
                // required to contain the full URI to the blob storage API endpoint, this means
                // that account_name is not required to exist in the connection_string since
                // account_name is only used with the default CloudLocation in the Azure SDK to
                // generate the storage API endpoint
                Some(uri) => ClientBuilder::with_location(
                    CloudLocation::Custom {
                        uri: uri.to_string(),
                    },
                    connection_string.storage_credentials()?,
                ),
                // Without a valid blob_endpoint in the connection_string, assume we are in Azure
                // Commercial (AzureCloud location) and create a default Blob Storage Client that
                // builds the API endpoint location using the account_name as input
                None => ClientBuilder::new(
                    connection_string
                        .account_name
                        .ok_or("Account name missing in connection string")?,
                    connection_string.storage_credentials()?,
                ),
            }
            .retry(RetryOptions::none())
            .container_client(container_name);
        }
        (None, Some(storage_account_p)) => {
            let creds = std::sync::Arc::new(DefaultAzureCredential::default());
            let auto_creds = std::sync::Arc::new(AutoRefreshingTokenCredential::new(creds));
            let storage_credentials = StorageCredentials::token_credential(auto_creds);

            client = match endpoint {
                // If a blob_endpoint is provided in the configuration, use it with a Custom
                // CloudLocation, to allow overriding the blob storage API endpoint
                Some(endpoint) => ClientBuilder::with_location(
                    CloudLocation::Custom { uri: endpoint },
                    storage_credentials,
                ),
                // Use the storage_account configuration parameter and assume we are in Azure
                // Commercial (AzureCloud location) and build the blob storage API endpoint using
                // the storage_account as input.
                None => ClientBuilder::new(storage_account_p, storage_credentials),
            }
            .retry(RetryOptions::none())
            .container_client(container_name);
        }
        (None, None) => {
            return Err("Either `connection_string` or `storage_account` has to be provided".into())
        }
        (Some(_), Some(_)) => {
            return Err(
                "`connection_string` and `storage_account` can't be provided at the same time"
                    .into(),
            )
        }
    }
    Ok(std::sync::Arc::new(client))
}
