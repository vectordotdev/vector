use std::sync::Arc;

use azure_core::error::Error as AzureCoreError;

use crate::sinks::azure_common::connection_string::{Auth, ParsedConnectionString};
use crate::sinks::azure_common::shared_key_policy::SharedKeyAuthorizationPolicy;
use azure_core::http::Url;
use azure_storage_blob::{BlobContainerClient, BlobContainerClientOptions};

use azure_core::http::StatusCode;
use bytes::Bytes;
use futures::FutureExt;
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
    type Error = AzureCoreError;
    type Request = AzureBlobRequest;
    type Response = AzureBlobResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error.http_status() {
            Some(code) => code.is_server_error() || code == StatusCode::TooManyRequests,
            None => false,
        }
    }
}

#[derive(Debug)]
pub struct AzureBlobResponse {
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
    client: Arc<BlobContainerClient>,
) -> crate::Result<Healthcheck> {
    let healthcheck = async move {
        let resp: crate::Result<()> = match client.get_properties(None).await {
            Ok(_) => Ok(()),
            Err(error) => {
                let code = error.http_status();
                Err(match code {
                    Some(StatusCode::Forbidden) => Box::new(HealthcheckError::InvalidCredentials),
                    Some(StatusCode::NotFound) => Box::new(HealthcheckError::UnknownContainer {
                        container: container_name,
                    }),
                    Some(status) => Box::new(HealthcheckError::Unknown { status }),
                    None => "unknown status code".into(),
                })
            }
        };
        resp
    };

    Ok(healthcheck.boxed())
}

pub fn build_client(
    connection_string: String,
    container_name: String,
    proxy: &crate::config::ProxyConfig,
) -> crate::Result<Arc<BlobContainerClient>> {
    // Parse connection string without legacy SDK
    let parsed = ParsedConnectionString::parse(&connection_string)
        .map_err(|e| format!("Invalid connection string: {e}"))?;
    // Compose container URL (SAS appended if present)
    let container_url = parsed
        .container_url(&container_name)
        .map_err(|e| format!("Failed to build container URL: {e}"))?;
    let url = Url::parse(&container_url).map_err(|e| format!("Invalid container URL: {e}"))?;

    // Prepare options; attach Shared Key policy if needed
    let mut options = BlobContainerClientOptions::default();
    match parsed.auth() {
        Auth::Sas { .. } | Auth::None => {
            // No extra policy; SAS is in the URL already (or anonymous)
        }
        Auth::SharedKey {
            account_name,
            account_key,
        } => {
            let policy = SharedKeyAuthorizationPolicy::new(
                account_name,
                account_key,
                // Use an Azurite-supported storage service version
                String::from("2025-11-05"),
            )
            .map_err(|e| format!("Failed to create SharedKey policy: {e}"))?;
            options
                .client_options
                .per_call_policies
                .push(Arc::new(policy));
        }
    }

    // Use reqwest v0.13 since Azure SDK 0.32+ implements HttpClient for reqwest::Client v0.13
    let mut reqwest_builder = reqwest_13::ClientBuilder::new();
    let bypass_proxy = {
        let host = url.host_str().unwrap_or("");
        let port = url.port();
        proxy.no_proxy.matches(host)
            || port
                .map(|p| proxy.no_proxy.matches(&format!("{}:{}", host, p)))
                .unwrap_or(false)
    };
    if bypass_proxy || !proxy.enabled {
        // Ensure no proxy (and disable any potential system proxy auto-detection)
        reqwest_builder = reqwest_builder.no_proxy();
    } else {
        if let Some(http) = &proxy.http {
            let p = reqwest_13::Proxy::http(http)
                .map_err(|e| format!("Invalid HTTP proxy URL: {e}"))?;
            // If credentials are embedded in the proxy URL, reqwest will handle them.
            reqwest_builder = reqwest_builder.proxy(p);
        }
        if let Some(https) = &proxy.https {
            let p = reqwest_13::Proxy::https(https)
                .map_err(|e| format!("Invalid HTTPS proxy URL: {e}"))?;
            // If credentials are embedded in the proxy URL, reqwest will handle them.
            reqwest_builder = reqwest_builder.proxy(p);
        }
    }
    options.client_options.transport = Some(azure_core::http::Transport::new(std::sync::Arc::new(
        reqwest_builder
            .build()
            .map_err(|e| format!("Failed to build reqwest client: {e}"))?,
    )));
    let client =
        BlobContainerClient::from_url(url, None, Some(options)).map_err(|e| format!("{e}"))?;
    Ok(Arc::new(client))
}
