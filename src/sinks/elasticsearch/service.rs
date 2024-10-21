use std::{
    sync::Arc,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures::future::BoxFuture;
use http::{Response, Uri};
use hyper::{service::Service, Body, Request};
use tower::ServiceExt;
use vector_lib::stream::DriverResponse;
use vector_lib::ByteSizeOf;
use vector_lib::{
    json_size::JsonSize,
    request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata},
};

use super::{ElasticsearchCommon, ElasticsearchConfig};
use crate::{
    event::{EventFinalizers, EventStatus, Finalizable},
    http::HttpClient,
    sinks::util::{
        auth::Auth,
        http::{HttpBatchService, RequestConfig},
        Compression, ElementCount,
    },
};

#[derive(Clone, Debug)]
pub struct ElasticsearchRequest {
    pub payload: Bytes,
    pub finalizers: EventFinalizers,
    pub batch_size: usize,
    pub events_byte_size: JsonSize,
    pub metadata: RequestMetadata,
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

#[derive(Clone)]
pub struct ElasticsearchService {
    // TODO: `HttpBatchService` has been deprecated for direct use in sinks.
    //       This sink should undergo a refactor to utilize the `HttpService`
    //       instead, which extracts much of the boilerplate code for `Service`.
    batch_service: HttpBatchService<
        BoxFuture<'static, Result<http::Request<Bytes>, crate::Error>>,
        ElasticsearchRequest,
    >,
}

impl ElasticsearchService {
    pub fn new(
        http_client: HttpClient<Body>,
        http_request_builder: HttpRequestBuilder,
    ) -> ElasticsearchService {
        let http_request_builder = Arc::new(http_request_builder);
        let batch_service = HttpBatchService::new(http_client, move |req| {
            let request_builder = Arc::clone(&http_request_builder);
            let future: BoxFuture<'static, Result<http::Request<Bytes>, crate::Error>> =
                Box::pin(async move { request_builder.build_request(req).await });
            future
        });
        ElasticsearchService { batch_service }
    }
}

pub struct HttpRequestBuilder {
    pub bulk_uri: Uri,
    pub auth: Option<Auth>,
    pub compression: Compression,
    pub http_request_config: RequestConfig,
}

impl HttpRequestBuilder {
    pub fn new(common: &ElasticsearchCommon, config: &ElasticsearchConfig) -> HttpRequestBuilder {
        HttpRequestBuilder {
            bulk_uri: common.bulk_uri.clone(),
            http_request_config: config.request.clone(),
            auth: common.auth.clone(),
            compression: config.compression,
        }
    }

    pub async fn build_request(
        &self,
        es_req: ElasticsearchRequest,
    ) -> Result<Request<Bytes>, crate::Error> {
        let mut builder = Request::post(&self.bulk_uri);

        builder = builder.header("Content-Type", "application/x-ndjson");

        if let Some(ce) = self.compression.content_encoding() {
            builder = builder.header("Content-Encoding", ce);
        }

        if let Some(ae) = self.compression.accept_encoding() {
            builder = builder.header("Accept-Encoding", ae);
        }

        for (header, value) in &self.http_request_config.headers {
            builder = builder.header(&header[..], &value[..]);
        }

        let mut request = builder
            .body(es_req.payload)
            .expect("Invalid http request value used");

        if let Some(auth) = &self.auth {
            match auth {
                Auth::Basic(auth) => {
                    auth.apply(&mut request);
                }
                #[cfg(feature = "aws-core")]
                Auth::Aws {
                    credentials_provider: provider,
                    region,
                } => {
                    crate::sinks::elasticsearch::sign_request(
                        &mut request,
                        provider,
                        &Some(region.clone()),
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
    pub events_byte_size: GroupedCountByteSize,
}

impl DriverResponse for ElasticsearchResponse {
    fn event_status(&self) -> EventStatus {
        self.event_status
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.events_byte_size
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
        let mut http_service = self.batch_service.clone();
        Box::pin(async move {
            http_service.ready().await?;
            let events_byte_size =
                std::mem::take(req.metadata_mut()).into_events_estimated_json_encoded_byte_size();
            let http_response = http_service.call(req).await?;

            let event_status = get_event_status(&http_response);
            Ok(ElasticsearchResponse {
                event_status,
                http_response,
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
