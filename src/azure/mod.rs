//! Shared functionality for the Azure components.
use std::sync::Arc;

use azure_core::RetryOptions;
use azure_identity::{AutoRefreshingTokenCredential, DefaultAzureCredential};
use azure_storage::{prelude::*, CloudLocation, ConnectionString};
use azure_storage_blobs;
use azure_storage_queues;

/// Builds Azure Storage Container Client.
///
/// To authenticate only **one** of `connection_string` or `storage_account` should be used.
pub fn build_container_client(
    connection_string: Option<String>,
    storage_account: Option<String>,
    container_name: String,
    endpoint: Option<String>,
) -> crate::Result<Arc<azure_storage_blobs::prelude::ContainerClient>> {
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
                Some(uri) => azure_storage_blobs::prelude::ClientBuilder::with_location(
                    CloudLocation::Custom {
                        uri: uri.to_string(),
                    },
                    connection_string.storage_credentials()?,
                ),
                // Without a valid blob_endpoint in the connection_string, assume we are in Azure
                // Commercial (AzureCloud location) and create a default Blob Storage Client that
                // builds the API endpoint location using the account_name as input
                None => azure_storage_blobs::prelude::ClientBuilder::new(
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
                Some(endpoint) => azure_storage_blobs::prelude::ClientBuilder::with_location(
                    CloudLocation::Custom { uri: endpoint },
                    storage_credentials,
                ),
                // Use the storage_account configuration parameter and assume we are in Azure
                // Commercial (AzureCloud location) and build the blob storage API endpoint using
                // the storage_account as input.
                None => azure_storage_blobs::prelude::ClientBuilder::new(
                    storage_account_p,
                    storage_credentials,
                ),
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

/// Builds Azure Queue Service Client.
///
/// To authenticate only **one** of `connection_string` or `storage_account` should be used.
pub fn build_queue_client(
    connection_string: Option<String>,
    storage_account: Option<String>,
    queue_name: String,
    endpoint: Option<String>,
) -> crate::Result<Arc<azure_storage_queues::QueueClient>> {
    let client;
    match (connection_string, storage_account) {
        (Some(connection_string_p), None) => {
            let connection_string = ConnectionString::new(&connection_string_p)?;

            client = match connection_string.queue_endpoint {
                // When the queue_endpoint is provided, we use the Custom CloudLocation since it is
                // required to contain the full URI to the storage queue API endpoint, this means
                // that account_name is not required to exist in the connection_string since
                // account_name is only used with the default CloudLocation in the Azure SDK to
                // generate the storage API endpoint
                Some(uri) => azure_storage_queues::QueueServiceClientBuilder::with_location(
                    CloudLocation::Custom {
                        uri: uri.to_string(),
                    },
                    connection_string.storage_credentials()?,
                ),
                // Without a valid queue_endpoint in the connection_string, assume we are in Azure
                // Commercial (AzureCloud location) and create a default Blob Storage Client that
                // builds the API endpoint location using the account_name as input
                None => azure_storage_queues::QueueServiceClientBuilder::new(
                    connection_string
                        .account_name
                        .ok_or("Account name missing in connection string")?,
                    connection_string.storage_credentials()?,
                ),
            }
            .retry(RetryOptions::none())
            .build()
            .queue_client(queue_name);
        }
        (None, Some(storage_account_p)) => {
            let creds = std::sync::Arc::new(DefaultAzureCredential::default());
            let auto_creds = std::sync::Arc::new(AutoRefreshingTokenCredential::new(creds));
            let storage_credentials = StorageCredentials::token_credential(auto_creds);

            client = match endpoint {
                // If a queue_endpoint is provided in the configuration, use it with a Custom
                // CloudLocation, to allow overriding the storage queue API endpoint
                Some(endpoint) => azure_storage_queues::QueueServiceClientBuilder::with_location(
                    CloudLocation::Custom { uri: endpoint },
                    storage_credentials,
                ),
                // Use the storage_account configuration parameter and assume we are in Azure
                // Commercial (AzureCloud location) and build the blob storage API endpoint using
                // the storage_account as input.
                None => azure_storage_queues::QueueServiceClientBuilder::new(
                    storage_account_p,
                    storage_credentials,
                ),
            }
            .retry(RetryOptions::none())
            .build()
            .queue_client(queue_name);
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
