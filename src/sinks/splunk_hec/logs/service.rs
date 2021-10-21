use std::{
    sync::Arc,
    task::{Context, Poll},
};

use crate::{
    internal_events::EventsSent,
    sinks::{
        splunk_hec::common::build_request,
        util::{http::HttpBatchService, ElementCount},
    },
};
use bytes::Bytes;
use futures_util::future::BoxFuture;
use http::{Request, Response};
use tower::{Service, ServiceExt};
use vector_core::{
    buffers::Ackable,
    event::{EventFinalizers, EventStatus, Finalizable},
    ByteSizeOf,
};

use crate::{http::HttpClient, sinks::util::Compression};

#[derive(Clone)]
pub struct HecLogsService {
    pub batch_service: HttpBatchService<
        BoxFuture<'static, Result<Request<Vec<u8>>, crate::Error>>,
        HecLogsRequest,
    >,
}

impl HecLogsService {
    pub fn new(client: HttpClient, http_request_builder: HttpRequestBuilder) -> Self {
        let http_request_builder = Arc::new(http_request_builder);
        let batch_service = HttpBatchService::new(client, move |req| {
            let request_builder = Arc::clone(&http_request_builder);
            let future: BoxFuture<'static, Result<http::Request<Vec<u8>>, crate::Error>> =
                Box::pin(async move { request_builder.build_request(req).await });
            future
        });
        Self { batch_service }
    }
}

impl Service<HecLogsRequest> for HecLogsService {
    type Response = HecLogsResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> std::task::Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: HecLogsRequest) -> Self::Future {
        let mut http_service = self.batch_service.clone();
        Box::pin(async move {
            http_service.ready().await?;
            let events_count = req.events_count;
            let byte_size = req.events_byte_size;
            let response = http_service.call(req).await?;
            let event_status = if response.status().is_success() {
                emit!(&EventsSent {
                    count: events_count,
                    byte_size: byte_size,
                });
                EventStatus::Delivered
            } else if response.status().is_server_error() {
                EventStatus::Errored
            } else {
                EventStatus::Failed
            };

            Ok(HecLogsResponse {
                http_response: response,
                event_status,
            })
        })
    }
}

#[derive(Clone)]
pub struct HecLogsRequest {
    pub body: Vec<u8>,
    pub events_count: usize,
    pub events_byte_size: usize,
    pub finalizers: EventFinalizers,
}

impl ByteSizeOf for HecLogsRequest {
    fn allocated_bytes(&self) -> usize {
        self.body.allocated_bytes() + self.finalizers.allocated_bytes()
    }
}

impl ElementCount for HecLogsRequest {
    fn element_count(&self) -> usize {
        self.events_count
    }
}

impl Ackable for HecLogsRequest {
    fn ack_size(&self) -> usize {
        self.events_count
    }
}

impl Finalizable for HecLogsRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

pub struct HecLogsResponse {
    pub http_response: Response<Bytes>,
    event_status: EventStatus,
}

impl AsRef<EventStatus> for HecLogsResponse {
    fn as_ref(&self) -> &EventStatus {
        &self.event_status
    }
}

pub struct HttpRequestBuilder {
    pub endpoint: String,
    pub token: String,
    pub compression: Compression,
}

impl HttpRequestBuilder {
    pub async fn build_request(
        &self,
        req: HecLogsRequest,
    ) -> Result<Request<Vec<u8>>, crate::Error> {
        build_request(
            self.endpoint.as_str(),
            self.token.as_str(),
            self.compression,
            req.body,
        )
        .await
    }
}
