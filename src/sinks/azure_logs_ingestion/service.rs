use std::sync::LazyLock;
use std::task::{Context, Poll};
use std::sync::Arc;
use futures::executor;

use azure_core::auth::TokenCredential;

use bytes::Bytes;
use http::{
    header::{self, HeaderMap},
    HeaderValue, Request, StatusCode, Uri,
};
use hyper::Body;
use tracing::Instrument;

use crate::{http::HttpClient, sinks::prelude::*};

/// Log Ingestion API version
const API_VERSION: &str = "2023-01-01";

/// JSON content type of logs
const CONTENT_TYPE: &str = "application/json";

static CONTENT_TYPE_VALUE: LazyLock<HeaderValue> =
    LazyLock::new(|| HeaderValue::from_static(CONTENT_TYPE));
// static X_MS_CLIENT_REQUEST_ID_HEADER: LazyLock<HeaderName> =
//     LazyLock::new(|| HeaderName::from_static("x-ms-client-request-id"));

#[derive(Debug, Clone)]
pub struct AzureLogsIngestionRequest {
    pub body: Bytes,
    pub finalizers: EventFinalizers,
    pub metadata: RequestMetadata,
}

impl MetaDescriptive for AzureLogsIngestionRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

impl Finalizable for AzureLogsIngestionRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.finalizers.take_finalizers()
    }
}

pub struct AzureLogsIngestionResponse {
    pub http_status: StatusCode,
    pub events_byte_size: GroupedCountByteSize,
    pub raw_byte_size: usize,
}

impl DriverResponse for AzureLogsIngestionResponse {
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

/// `AzureLogsIngestionService` is a `Tower` service used to send logs to Azure.
#[derive(Debug, Clone)]
pub struct AzureLogsIngestionService {
    client: HttpClient,
    endpoint: Uri,
    credential: Arc<dyn TokenCredential>,
    token_scope: String,
    default_headers: HeaderMap,
}

impl AzureLogsIngestionService {
    /// Creates a new `AzureLogsIngestionService`.
    pub fn new(
        client: HttpClient,
        endpoint: Uri,
        dcr_immutable_id: String,
        stream_name: String,
        credential: Arc<dyn TokenCredential>,
        token_scope: String,
    ) -> crate::Result<Self> {
        let mut parts = endpoint.into_parts();
        parts.path_and_query = Some(
            // https://my-dce-5kyl.eastus-1.ingest.monitor.azure.com/dataCollectionRules/dcr-000a00a000a00000a000000aa000a0aa/streams/Custom-MyTable?api-version=2023-01-01
            format!("/dataCollectionRules/{dcr_immutable_id}/streams/{stream_name}?api-version={API_VERSION}")
                .parse()
                .expect("path and query should never fail to parse"),
        );
        let endpoint = Uri::from_parts(parts)?;

        let default_headers = {
            let mut headers = HeaderMap::new();

            headers.insert(header::CONTENT_TYPE, CONTENT_TYPE_VALUE.clone());
            headers
        };

        Ok(Self {
            client,
            endpoint,
            credential,
            token_scope,
            default_headers,
        })
    }

    fn build_request(&self, body: Bytes) -> crate::Result<Request<Body>> {
        let mut request = Request::post(&self.endpoint).body(Body::from(body))?;

        // TODO: make this an option, for soverign clouds
        let access_token = executor::block_on(self.credential
            .get_token(&[&self.token_scope]))
            .expect("failed to get access token from credential");
        
        let bearer = format!("Bearer {}", access_token.token.secret());

        *request.headers_mut() = self.default_headers.clone();
        request
            .headers_mut()
            .insert(
                header::AUTHORIZATION,
                HeaderValue::from_str(&bearer).unwrap()
            );

        Ok(request)
    }

    pub fn healthcheck(&self) -> Healthcheck {
        let mut client = self.client.clone();
        let request = self.build_request(Bytes::from("[]"));
        Box::pin(async move {
            let request = request?;
            let res = client.call(request).in_current_span().await?;

            if res.status().is_server_error() {
                return Err("Azure returned a server error".into());
            }

            if res.status() == StatusCode::UNAUTHORIZED {
                return Err("Azure returned 401 Unauthorised. Check that the token_scope matches the soverign cloud endpoint.".into());
            }

            if res.status() == StatusCode::FORBIDDEN {
                return Err("Azure returned 403 Forbidden. Verify that the credential has the Monitoring Metrics Publisher role on the Data Collection Rule.".into());
            }

            if res.status() == StatusCode::NOT_FOUND {
                return Err("Azure returned 404 Not Found. Either the URL provided is incorrect, or the request is too large".into());
            }

            if res.status() == StatusCode::BAD_REQUEST {
                let body_bytes: Bytes = hyper::body::to_bytes(res.into_body()).await.unwrap();
                let body_string: String = String::from_utf8(body_bytes.to_vec()).unwrap();
                let err_string: String = format!("Azure returned 400 Bad Request: {body_string}");
                return Err(err_string.into());
            }

            Ok(())
        })
    }
}

impl Service<AzureLogsIngestionRequest> for AzureLogsIngestionService {
    type Response = AzureLogsIngestionResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    // Emission of Error internal event is handled upstream by the caller.
    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of Error internal event is handled upstream by the caller.
    fn call(&mut self, request: AzureLogsIngestionRequest) -> Self::Future {
        let mut client = self.client.clone();
        let http_request = self.build_request(request.body);
        Box::pin(async move {
            let http_request = http_request?;
            let response = client.call(http_request).in_current_span().await?;
            Ok(AzureLogsIngestionResponse {
                http_status: response.status(),
                raw_byte_size: request.metadata.request_encoded_size(),
                events_byte_size: request
                    .metadata
                    .into_events_estimated_json_encoded_byte_size(),
            })
        })
    }
}
