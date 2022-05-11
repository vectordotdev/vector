use std::{
    collections::HashMap,
    sync::Arc,
    task::{Context, Poll},
};

use aws_types::credentials::SharedCredentialsProvider;
use aws_types::region::Region;
use bytes::Bytes;
use futures::future::BoxFuture;
use http::{Response, Uri};
use hyper::{service::Service, Body, Request};
use tower::ServiceExt;
use vector_core::{
    buffers::Ackable, internal_event::EventsSent, stream::DriverResponse, ByteSizeOf,
};

use crate::sinks::elasticsearch::sign_request;
use crate::{
    event::{EventFinalizers, EventStatus, Finalizable},
    http::{Auth, HttpClient},
    internal_events::ElasticsearchResponseError,
    sinks::util::{
        http::{HttpBatchService, RequestConfig},
        Compression, ElementCount,
    },
};

#[derive(Clone)]
pub struct ElasticsearchRequest {
    pub payload: Bytes,
    pub finalizers: EventFinalizers,
    pub batch_size: usize,
    pub events_byte_size: usize,
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

impl Ackable for ElasticsearchRequest {
    fn ack_size(&self) -> usize {
        self.batch_size
    }
}

impl Finalizable for ElasticsearchRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
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
    pub async fn build_request(
        &self,
        es_req: ElasticsearchRequest,
    ) -> Result<Request<Bytes>, crate::Error> {
        let mut builder = Request::post(&self.bulk_uri);

        builder = builder.header("Content-Type", "application/x-ndjson");

        if let Some(ce) = self.compression.content_encoding() {
            builder = builder.header("Content-Encoding", ce);
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

    fn events_sent(&self) -> EventsSent {
        EventsSent {
            count: self.batch_size,
            byte_size: self.events_byte_size,
            output: None,
        }
    }
}

impl Service<ElasticsearchRequest> for ElasticsearchService {
    type Response = ElasticsearchResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

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
            emit!(ElasticsearchResponseError::new(
                "Response containerd errors.",
                response
            ));
            EventStatus::Rejected
        } else {
            EventStatus::Delivered
        }
    } else if status.is_server_error() {
        emit!(ElasticsearchResponseError::new(
            "Response wasn't successful.",
            response,
        ));
        EventStatus::Errored
    } else {
        emit!(ElasticsearchResponseError::new(
            "Response failed.",
            response,
        ));
        EventStatus::Rejected
    }
}
