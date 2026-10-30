use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
};

use bytes::Bytes;
use futures::future::BoxFuture;
use http_1::{Request, Response, Uri};
use http_body_util::BodyExt;
use tower::{Service, service_fn};
use tower_http_1::decompression::Decompression;
use vector_http::full_body;
use vector_lib::{
    ByteSizeOf,
    json_size::JsonSize,
    request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata},
    stream::DriverResponse,
};

use super::{ElasticsearchCommon, ElasticsearchConfig, retry::successful_events};
use crate::{
    event::{EventFinalizers, EventStatus, Finalizable},
    http::client_v1::HttpClient,
    internal_events::{EndpointBytesSent, SinkRequestBuildError},
    sinks::{
        elasticsearch::{encoder::ProcessedEvent, request_builder::ElasticsearchRequestBuilder},
        util::{
            Compression, ElementCount, auth::Auth, http::RequestConfig, http_v1::protocol_endpoint,
        },
    },
};

#[derive(Clone, Debug)]
pub struct ElasticsearchRequest {
    pub payload: Bytes,
    pub finalizers: EventFinalizers,
    pub batch_size: usize,
    pub events_byte_size: JsonSize,
    pub metadata: RequestMetadata,
    pub original_events: Vec<ProcessedEvent>, //store original_events for reconstruct request when retrying
    pub elasticsearch_request_builder: ElasticsearchRequestBuilder,
    // Shared by retry clones, counting successful events removed by partial retries.
    pub confirmed_events: Arc<AtomicUsize>,
}

impl ByteSizeOf for ElasticsearchRequest {
    fn allocated_bytes(&self) -> usize {
        self.payload.allocated_bytes() + self.finalizers.allocated_bytes()
    }
}

impl ElementCount for ElasticsearchRequest {
    fn element_count(&self) -> usize {
        self.batch_size
    }
}

impl Finalizable for ElasticsearchRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl MetaDescriptive for ElasticsearchRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

/// Sends bulk requests through the shared native HTTP client.
///
/// Bulk requests may need asynchronous AWS signing with a service-specific payload hash, so
/// this service builds requests itself instead of using the generic `http_v1::HttpService`.
/// Like the legacy HTTP batch service, it requests and decodes compressed responses.
#[derive(Clone)]
pub struct ElasticsearchService {
    client: HttpClient,
    http_request_builder: Arc<HttpRequestBuilder>,
    protocol: Arc<str>,
    endpoint: Arc<str>,
}

impl ElasticsearchService {
    pub fn new(
        http_client: HttpClient,
        http_request_builder: HttpRequestBuilder,
    ) -> ElasticsearchService {
        let (protocol, endpoint) = protocol_endpoint(&http_request_builder.bulk_uri);
        ElasticsearchService {
            client: http_client,
            http_request_builder: Arc::new(http_request_builder),
            protocol: protocol.into(),
            endpoint: endpoint.into(),
        }
    }
}

pub struct HttpRequestBuilder {
    pub bulk_uri: Uri,
    pub auth: Option<Auth>,
    pub service_type: crate::sinks::elasticsearch::OpenSearchServiceType,
    pub compression: Compression,
    pub http_request_config: RequestConfig,
}

impl HttpRequestBuilder {
    pub fn new(common: &ElasticsearchCommon, config: &ElasticsearchConfig) -> HttpRequestBuilder {
        HttpRequestBuilder {
            bulk_uri: common.bulk_uri.clone(),
            auth: common.auth.clone(),
            service_type: common.service_type.clone(),
            compression: config.compression,
            http_request_config: config.request.clone(),
        }
    }

    /// Builds and authenticates a bulk request.
    ///
    /// # Errors
    /// Returns an error if AWS credentials cannot be loaded or the request cannot be signed.
    ///
    /// # Panics
    /// Panics if the configured URI or headers are invalid for an HTTP request.
    #[cfg_attr(
        not(feature = "aws-core"),
        expect(
            clippy::unused_async,
            reason = "AWS request signing requires await when aws-core is enabled."
        )
    )]
    pub async fn build_request(
        &self,
        es_req: ElasticsearchRequest,
    ) -> Result<Request<Bytes>, crate::Error> {
        let mut builder = Request::post(self.bulk_uri.clone());

        builder = builder.header("Content-Type", "application/x-ndjson");

        if let Some(ce) = self.compression.content_encoding() {
            builder = builder.header("Content-Encoding", ce);
        }

        if let Some(ae) = self.compression.accept_encoding() {
            builder = builder.header("Accept-Encoding", ae);
        }

        for (header, value) in &self.http_request_config.headers {
            builder = builder.header(header.as_str(), value.as_str());
        }

        let mut request = builder
            .body(es_req.payload)
            .expect("Invalid http request value used");

        if let Some(auth) = &self.auth {
            match auth {
                Auth::Basic(auth) => {
                    auth.apply_v1(&mut request);
                }
                #[cfg(feature = "aws-core")]
                Auth::Aws {
                    credentials_provider: provider,
                    region,
                } => {
                    crate::sinks::elasticsearch::sign_request(
                        &self.service_type,
                        &mut request,
                        provider,
                        Some(region),
                    )
                    .await?;
                }
            }
        }

        Ok(request)
    }
}

pub struct ElasticsearchResponse {
    pub http_response: Response<Bytes>,
    pub event_status: EventStatus,
    pub event_count: usize,
    pub events_byte_size: GroupedCountByteSize,
}

pub struct ElasticsearchDriverResponse {
    event_status: EventStatus,
    events_byte_size: GroupedCountByteSize,
    events_dropped: usize,
    error: Option<crate::Error>,
}

impl DriverResponse for ElasticsearchDriverResponse {
    fn event_status(&self) -> EventStatus {
        self.event_status
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.events_byte_size
    }

    fn events_dropped(&self) -> Option<usize> {
        Some(self.events_dropped)
    }

    fn error(&self) -> Option<&dyn fmt::Debug> {
        self.error.as_ref().map(|error| error as &dyn fmt::Debug)
    }
}

/// Accounts for delivery across the entire retry stack, including terminal service errors.
pub struct ElasticsearchResponseService<S> {
    inner: S,
}

impl<S> ElasticsearchResponseService<S> {
    pub const fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S> Service<ElasticsearchRequest> for ElasticsearchResponseService<S>
where
    S: Service<ElasticsearchRequest, Response = ElasticsearchResponse, Error = crate::Error>,
    S::Future: Send + 'static,
{
    type Response = ElasticsearchDriverResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: ElasticsearchRequest) -> Self::Future {
        let event_count = req.metadata.event_count();
        let confirmed_events = Arc::clone(&req.confirmed_events);
        let future = self.inner.call(req);
        Box::pin(async move {
            let result = future.await;
            let remaining = event_count.saturating_sub(confirmed_events.load(Ordering::Relaxed));
            Ok(match result {
                Ok(response) => {
                    let confirmed = if response.event_status == EventStatus::Delivered {
                        response.event_count
                    } else if response.http_response.status().is_success() {
                        successful_events(response.http_response.body(), response.event_count)
                            .unwrap_or_default()
                    } else {
                        0
                    };
                    ElasticsearchDriverResponse {
                        event_status: response.event_status,
                        events_byte_size: response.events_byte_size,
                        events_dropped: remaining.saturating_sub(confirmed),
                        error: None,
                    }
                }
                Err(error) => ElasticsearchDriverResponse {
                    event_status: EventStatus::Rejected,
                    events_byte_size: GroupedCountByteSize::default(),
                    events_dropped: remaining,
                    error: Some(error),
                },
            })
        })
    }
}

impl Service<ElasticsearchRequest> for ElasticsearchService {
    type Response = ElasticsearchResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    // Emission of an internal event in case of errors is handled upstream by the caller.
    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of internal events for errors and dropped events is handled upstream by the caller.
    fn call(&mut self, mut req: ElasticsearchRequest) -> Self::Future {
        let client = self.client.clone();
        let http_request_builder = Arc::clone(&self.http_request_builder);
        let protocol = Arc::clone(&self.protocol);
        let endpoint = Arc::clone(&self.endpoint);
        Box::pin(async move {
            let metadata = std::mem::take(req.metadata_mut());
            let event_count = metadata.event_count();
            let events_byte_size = metadata.into_events_estimated_json_encoded_byte_size();
            let request = http_request_builder
                .build_request(req)
                .await
                .inspect_err(|error| {
                    emit!(SinkRequestBuildError { error });
                })?;
            let byte_size = request.body().len();

            // Transport errors are emitted as `GotHttpWarning` by `HttpClient::send`; the driver
            // counts the component error once retries are exhausted.
            let mut service = Decompression::new(service_fn(|request| client.send(request)));
            let response = service.call(request.map(full_body)).await?;

            if response.status().is_success() {
                emit!(EndpointBytesSent {
                    byte_size,
                    protocol: &protocol,
                    endpoint: &endpoint,
                });
            }

            let (parts, body) = response.into_parts();
            let http_response = Response::from_parts(parts, body.collect().await?.to_bytes());

            let event_status = get_event_status(&http_response);
            Ok(ElasticsearchResponse {
                http_response,
                event_status,
                event_count,
                events_byte_size,
            })
        })
    }
}

// This event is not part of the event framework but is kept because some users were depending on it
// to identify the number of errors returned by Elasticsearch. It can be dropped when we have better
// telemetry. Ref: #15886
fn emit_bad_response_error(response: &Response<Bytes>) {
    let error_code = format!("http_response_{}", response.status().as_u16());

    error!(
        message =  "Response contained errors.",
        error_code = error_code,
        response = ?response,
    );
}

fn get_event_status(response: &Response<Bytes>) -> EventStatus {
    let status = response.status();
    if status.is_success() {
        let body = String::from_utf8_lossy(response.body());
        if body.contains("\"errors\":true") {
            emit_bad_response_error(response);
            EventStatus::Rejected
        } else {
            EventStatus::Delivered
        }
    } else if status.is_server_error() {
        emit_bad_response_error(response);
        EventStatus::Errored
    } else {
        emit_bad_response_error(response);
        EventStatus::Rejected
    }
}
