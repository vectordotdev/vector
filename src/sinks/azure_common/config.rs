use std::sync::Arc;

use azure_core::error::HttpError;
use azure_core_for_storage::RetryOptions;
use azure_storage::{CloudLocation, ConnectionString};
use azure_storage_blobs::{blob::operations::PutBlockBlobResponse, prelude::*};
use bytes::Bytes;
use futures::FutureExt;
use http::StatusCode;
use snafu::Snafu;
use vector_lib::{
    json_size::JsonSize,
    request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata},
    stream::DriverResponse,
};

use crate::{
    event::{EventFinalizers, EventStatus, Finalizable},
    sinks::{Healthcheck, util::retries::RetryLogic},
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
    type Request = AzureBlobRequest;
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
            Err(error) => Err(match error.as_http_error() {
                Some(err) => match StatusCode::from_u16(err.status().into()) {
                    Ok(StatusCode::FORBIDDEN) => Box::new(HealthcheckError::InvalidCredentials),
                    Ok(StatusCode::NOT_FOUND) => Box::new(HealthcheckError::UnknownContainer {
                        container: container_name,
                    }),
                    Ok(status) => Box::new(HealthcheckError::Unknown { status }),
                    Err(_) => "unknown status code".into(),
                },
                _ => error.into(),
            }),
        };
        resp
    };

    Ok(healthcheck.boxed())
}

pub fn build_client(
    connection_string: String,
    container_name: String,
) -> crate::Result<Arc<ContainerClient>> {
    let client = {
        let connection_string = ConnectionString::new(&connection_string)?;

        // Extract account name from connection string or blob endpoint
        let account_name = match (&connection_string.account_name, &connection_string.blob_endpoint) {
            // If account_name is provided in the connection string, use it
            (Some(name), _) => name.to_string(),
            // If blob_endpoint is provided but account_name is not, extract it from the endpoint URL
            (None, Some(uri)) => extract_account_name_from_endpoint(&uri.to_string())?,
            // If neither is provided, return an error
            (None, None) => return Err("Account name missing in connection string and could not be extracted from blob endpoint".into()),
        };

        match connection_string.blob_endpoint {
            // When the blob_endpoint is provided, we use the Custom CloudLocation since it is
            // required to contain the full URI to the blob storage API endpoint, this means
            // that account_name is not required to exist in the connection_string since
            // account_name is only used with the default CloudLocation in the Azure SDK to
            // generate the storage API endpoint
            Some(uri) => ClientBuilder::with_location(
                CloudLocation::Custom {
                    uri: uri.to_string(),
                    account: account_name.to_string(),
                },
                connection_string.storage_credentials()?,
            ),
            // Without a valid blob_endpoint in the connection_string, assume we are in Azure
            // Commercial (AzureCloud location) and create a default Blob Storage Client that
            // builds the API endpoint location using the account_name as input
            None => ClientBuilder::new(account_name, connection_string.storage_credentials()?),
        }
        .retry(RetryOptions::none())
        .container_client(container_name)
    };
    Ok(Arc::new(client))
}

/// Extracts the account name from an Azure Blob Storage endpoint URL.
///
/// The account name is the subdomain before `.blob.core.windows.net` in the URL.
/// For example, from `https://mystorageaccount.blob.core.windows.net/`,
/// this function extracts `mystorageaccount`.
fn extract_account_name_from_endpoint(endpoint: &str) -> crate::Result<String> {
    // Parse the URL to extract the host
    let url = url::Url::parse(endpoint)
        .map_err(|e| format!("Failed to parse blob endpoint URL: {}", e))?;

    let host = url.host_str()
        .ok_or("Blob endpoint URL does not contain a valid host")?;

    // Extract account name from host (e.g., "mystorageaccount.blob.core.windows.net")
    // The account name is the first part before the first dot
    let account_name = host.split('.')
        .next()
        .ok_or("Failed to extract account name from blob endpoint")?;

    if account_name.is_empty() {
        return Err("Account name extracted from blob endpoint is empty".into());
    }

    Ok(account_name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_account_name_from_endpoint() {
        // Standard Azure blob endpoint
        let result = extract_account_name_from_endpoint("https://mystorageaccount.blob.core.windows.net/");
        assert_eq!(result.unwrap(), "mystorageaccount");

        // Azure blob endpoint without trailing slash
        let result = extract_account_name_from_endpoint("https://teststorage.blob.core.windows.net");
        assert_eq!(result.unwrap(), "teststorage");

        // Azure blob endpoint with path
        let result = extract_account_name_from_endpoint("https://myaccount.blob.core.windows.net/container");
        assert_eq!(result.unwrap(), "myaccount");

        // HTTP endpoint (for emulator)
        let result = extract_account_name_from_endpoint("http://127.0.0.1:10000/devstoreaccount1");
        assert_eq!(result.unwrap(), "127");

        // Invalid URL
        let result = extract_account_name_from_endpoint("not-a-url");
        assert!(result.is_err());

        // Empty host
        let result = extract_account_name_from_endpoint("https://");
        assert!(result.is_err());
    }

    #[test]
    fn test_build_client_with_account_name_in_connection_string() {
        // Connection string with AccountName explicitly provided
        let connection_string = "DefaultEndpointsProtocol=https;AccountName=myaccount;AccountKey=YWNjb3VudGtleQ==;EndpointSuffix=core.windows.net".to_string();
        let result = build_client(connection_string, "test-container".to_string());
        assert!(result.is_ok(), "Should succeed when AccountName is provided");
    }

    #[test]
    fn test_build_client_with_blob_endpoint_and_account_name() {
        // Connection string with both BlobEndpoint and AccountName
        let connection_string = "AccountName=myaccount;BlobEndpoint=https://myaccount.blob.core.windows.net/;SharedAccessSignature=sv=2021-01-01&sig=test".to_string();
        let result = build_client(connection_string, "test-container".to_string());
        assert!(result.is_ok(), "Should succeed when both AccountName and BlobEndpoint are provided");
    }

    #[test]
    fn test_build_client_with_blob_endpoint_without_account_name() {
        // Connection string with BlobEndpoint but no AccountName (the regression case)
        let connection_string = "BlobEndpoint=https://mystorageaccount.blob.core.windows.net/;SharedAccessSignature=sv=2021-01-01&sig=test".to_string();
        let result = build_client(connection_string, "test-container".to_string());
        assert!(result.is_ok(), "Should succeed by extracting AccountName from BlobEndpoint");
    }

    #[test]
    fn test_build_client_without_account_name_or_blob_endpoint() {
        // Connection string with neither AccountName nor BlobEndpoint
        let connection_string = "SharedAccessSignature=sv=2021-01-01&sig=test".to_string();
        let result = build_client(connection_string, "test-container".to_string());
        assert!(result.is_err(), "Should fail when neither AccountName nor BlobEndpoint is provided");
        assert!(result.unwrap_err().to_string().contains("Account name missing"));
    }
}
