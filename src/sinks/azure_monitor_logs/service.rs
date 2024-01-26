use bytes::Bytes;
use http::{
    header::{self, HeaderMap},
    HeaderName, HeaderValue, Request, StatusCode, Uri,
};
use hyper::Body;
use once_cell::sync::Lazy;
use openssl::{base64, hash, pkey, sign};
use regex::Regex;
use std::task::{Context, Poll};
use tracing::Instrument;
use vector_lib::lookup::lookup_v2::OwnedValuePath;

use crate::{http::HttpClient, sinks::prelude::*};

static LOG_TYPE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\w+$").unwrap());
static LOG_TYPE_HEADER: Lazy<HeaderName> = Lazy::new(|| HeaderName::from_static("log-type"));
static X_MS_DATE_HEADER: Lazy<HeaderName> = Lazy::new(|| HeaderName::from_static(X_MS_DATE));
static X_MS_AZURE_RESOURCE_HEADER: Lazy<HeaderName> =
    Lazy::new(|| HeaderName::from_static("x-ms-azureresourceid"));
static TIME_GENERATED_FIELD_HEADER: Lazy<HeaderName> =
    Lazy::new(|| HeaderName::from_static("time-generated-field"));
static CONTENT_TYPE_VALUE: Lazy<HeaderValue> = Lazy::new(|| HeaderValue::from_static(CONTENT_TYPE));

/// API endpoint for submitting logs
const RESOURCE: &str = "/api/logs";
/// JSON content type of logs
const CONTENT_TYPE: &str = "application/json";
/// Custom header used for signing logs
const X_MS_DATE: &str = "x-ms-date";
/// Shared key prefix
const SHARED_KEY: &str = "SharedKey";
/// API version
const API_VERSION: &str = "2016-04-01";

#[derive(Debug, Clone)]
pub struct AzureMonitorLogsRequest {
    pub body: Bytes,
    pub finalizers: EventFinalizers,
    pub metadata: RequestMetadata,
}

impl MetaDescriptive for AzureMonitorLogsRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

impl Finalizable for AzureMonitorLogsRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.finalizers.take_finalizers()
    }
}

pub struct AzureMonitorLogsResponse {
    pub http_status: StatusCode,
    pub events_byte_size: GroupedCountByteSize,
    pub raw_byte_size: usize,
}

impl DriverResponse for AzureMonitorLogsResponse {
    fn event_status(&self) -> EventStatus {
        match self.http_status.is_success() {
            true => EventStatus::Delivered,
            false => EventStatus::Rejected,
        }
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.events_byte_size
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.raw_byte_size)
    }
}

/// `AzureMonitorLogsService` is a `Tower` service used to send logs to Azure.
#[derive(Debug, Clone)]
pub struct AzureMonitorLogsService {
    client: HttpClient,
    endpoint: Uri,
    customer_id: String,
    shared_key: pkey::PKey<pkey::Private>,
    default_headers: HeaderMap,
}

impl AzureMonitorLogsService {
    /// Creates a new `AzureMonitorLogsService`.
    pub fn new(
        client: HttpClient,
        endpoint: Uri,
        customer_id: String,
        azure_resource_id: Option<&str>,
        log_type: &str,
        time_generated_key: Option<OwnedValuePath>,
        shared_key: pkey::PKey<pkey::Private>,
    ) -> crate::Result<Self> {
        let mut parts = endpoint.into_parts();
        parts.path_and_query = Some(
            format!("{RESOURCE}?api-version={API_VERSION}")
                .parse()
                .expect("path and query should never fail to parse"),
        );
        let endpoint = Uri::from_parts(parts)?;

        let default_headers = {
            let mut headers = HeaderMap::new();

            if log_type.len() > 100 || !LOG_TYPE_REGEX.is_match(log_type) {
                return Err(format!(
                "invalid log_type \"{}\": log type can only contain letters, numbers, and underscore (_), and may not exceed 100 characters",
                log_type
            ).into());
            }
            let log_type = HeaderValue::from_str(log_type)?;
            headers.insert(LOG_TYPE_HEADER.clone(), log_type);

            if let Some(timestamp_key) = time_generated_key {
                headers.insert(
                    TIME_GENERATED_FIELD_HEADER.clone(),
                    HeaderValue::try_from(timestamp_key.to_string())?,
                );
            }

            if let Some(azure_resource_id) = azure_resource_id {
                if azure_resource_id.is_empty() {
                    return Err("azure_resource_id can't be an empty string".into());
                }
                headers.insert(
                    X_MS_AZURE_RESOURCE_HEADER.clone(),
                    HeaderValue::from_str(azure_resource_id)?,
                );
            }

            headers.insert(header::CONTENT_TYPE, CONTENT_TYPE_VALUE.clone());
            headers
        };

        Ok(Self {
            client,
            endpoint,
            customer_id,
            shared_key,
            default_headers,
        })
    }

    fn build_authorization_header_value(
        &self,
        rfc1123date: &str,
        len: usize,
    ) -> crate::Result<String> {
        let string_to_hash =
            format!("POST\n{len}\n{CONTENT_TYPE}\n{X_MS_DATE}:{rfc1123date}\n{RESOURCE}");
        let mut signer = sign::Signer::new(hash::MessageDigest::sha256(), &self.shared_key)?;
        signer.update(string_to_hash.as_bytes())?;

        let signature = signer.sign_to_vec()?;
        let signature_base64 = base64::encode_block(&signature);

        Ok(format!(
            "{} {}:{}",
            SHARED_KEY, self.customer_id, signature_base64
        ))
    }

    fn build_request(&self, body: Bytes) -> crate::Result<Request<Body>> {
        let len = body.len();

        let mut request = Request::post(&self.endpoint).body(Body::from(body))?;

        let rfc1123date = chrono::Utc::now()
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();
        let authorization = self.build_authorization_header_value(&rfc1123date, len)?;

        *request.headers_mut() = self.default_headers.clone();
        request
            .headers_mut()
            .insert(header::AUTHORIZATION, authorization.parse()?);
        request
            .headers_mut()
            .insert(X_MS_DATE_HEADER.clone(), rfc1123date.parse()?);

        Ok(request)
    }

    pub fn healthcheck(&self) -> Healthcheck {
        let mut client = self.client.clone();
        let request = self.build_request(Bytes::from("[]"));
        Box::pin(async move {
            let request = request?;
            let res = client.call(request).in_current_span().await?;

            if res.status().is_server_error() {
                return Err("Server returned a server error".into());
            }

            if res.status() == StatusCode::FORBIDDEN {
                return Err("The service failed to authenticate the request. Verify that the workspace ID and connection key are valid".into());
            }

            if res.status() == StatusCode::NOT_FOUND {
                return Err(
                    "Either the URL provided is incorrect, or the request is too large".into(),
                );
            }

            if res.status() == StatusCode::BAD_REQUEST {
                return Err("The workspace has been closed or the request was invalid".into());
            }

            Ok(())
        })
    }
}

impl Service<AzureMonitorLogsRequest> for AzureMonitorLogsService {
    type Response = AzureMonitorLogsResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    // Emission of Error internal event is handled upstream by the caller.
    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of Error internal event is handled upstream by the caller.
    fn call(&mut self, request: AzureMonitorLogsRequest) -> Self::Future {
        let mut client = self.client.clone();
        let http_request = self.build_request(request.body);
        Box::pin(async move {
            let http_request = http_request?;
            let response = client.call(http_request).in_current_span().await?;
            Ok(AzureMonitorLogsResponse {
                http_status: response.status(),
                raw_byte_size: request.metadata.request_encoded_size(),
                events_byte_size: request
                    .metadata
                    .into_events_estimated_json_encoded_byte_size(),
            })
        })
    }
}
