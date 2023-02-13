use std::{
    collections::HashMap,
    sync::Arc,
    task::{Context, Poll},
};

use aws_credential_types::provider::SharedCredentialsProvider;
use aws_types::region::Region;
use bytes::Bytes;
use futures::future::BoxFuture;
use http::{Response, Uri};
use hyper::{service::Service, Body, Request};
use tower::ServiceExt;
use vector_common::request_metadata::{MetaDescriptive, RequestMetadata};
use vector_core::{internal_event::CountByteSize, stream::DriverResponse, ByteSizeOf};

use crate::sinks::elasticsearch::sign_request;
use crate::{
    event::{EventFinalizers, EventStatus, Finalizable},
    http::{Auth, HttpClient},
    sinks::util::{
        http::{HttpBatchService, RequestConfig},
        Compression, ElementCount,
    },
};

use super::{ElasticsearchCommon, ElasticsearchConfig};

#[derive(Clone, Debug)]
pub struct ElasticsearchRequest {
    pub payload: Bytes,
    pub finalizers: EventFinalizers,
    pub batch_size: usize,
    pub events_byte_size: usize,
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
    fn get_metadata(&self) -> RequestMetadata {
        self.metadata
    }
}

#[derive(Clone)]
pub struct ElasticsearchService {
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
    pub query_params: HashMap<String, String>,
    pub region: Option<Region>,
    pub compression: Compression,
    pub http_request_config: RequestConfig,
    pub http_auth: Option<Auth>,
    pub credentials_provider: Option<SharedCredentialsProvider>,
}

impl HttpRequestBuilder {
    pub fn new(common: &ElasticsearchCommon, config: &ElasticsearchConfig) -> HttpRequestBuilder {
        HttpRequestBuilder {
            bulk_uri: common.bulk_uri.clone(),
            http_request_config: config.request.clone(),
            http_auth: common.http_auth.clone(),
            query_params: common.query_params.clone(),
            region: common.region.clone(),
            compression: config.compression,
            credentials_provider: common.aws_auth.clone(),
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

        if let Some(auth) = &self.http_auth {
            builder = auth.apply_builder(builder);
        }

        let mut request = builder
            .body(es_req.payload)
            .expect("Invalid http request value used");

        if let Some(credentials_provider) = &self.credentials_provider {
            sign_request(&mut request, credentials_provider, &self.region).await?;
        }

        Ok(request)
    }
}

pub struct ElasticsearchResponse {
    pub http_response: Response<Bytes>,
    pub event_status: EventStatus,
    pub batch_size: usize,
    pub events_byte_size: usize,
}

impl DriverResponse for ElasticsearchResponse {
    fn event_status(&self) -> EventStatus {
        self.event_status
    }

    fn events_sent(&self) -> CountByteSize {
        CountByteSize(self.batch_size, self.events_byte_size)
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
    fn call(&mut self, req: ElasticsearchRequest) -> Self::Future {
        let mut http_service = self.batch_service.clone();
        Box::pin(async move {
            http_service.ready().await?;
            let batch_size = req.batch_size;
            let events_byte_size = req.events_byte_size;
            let http_response = http_service.call(req).await?;

            let event_status = get_event_status(&http_response);
            Ok(ElasticsearchResponse {
                event_status,
                http_response,
                batch_size,
                events_byte_size,
            })
        })
    }
}

fn get_event_status(response: &Response<Bytes>) -> EventStatus {
    let status = response.status();
    if status.is_success() {
        let body = String::from_utf8_lossy(response.body());
        if body.contains("\"errors\":true") {
            EventStatus::Rejected
        } else {
            EventStatus::Delivered
        }
    } else if status.is_server_error() {
        EventStatus::Errored
    } else {
        EventStatus::Rejected
    }
}
