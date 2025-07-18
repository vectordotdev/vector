use std::task::Poll;

use bytes::Bytes;
use futures::{
    future,
    future::{BoxFuture, Ready},
};
use http::{header::AUTHORIZATION, Request, StatusCode, Uri};
use hyper::Body;
use tower::{Service, ServiceExt};

use vector_lib::stream::DriverResponse;
use vector_lib::{
    finalization::EventStatus, request_metadata::GroupedCountByteSize,
    request_metadata::MetaDescriptive, sensitive_string::SensitiveString,
};

use crate::{
    http::HttpClient,
    sinks::util::{http::HttpBatchService, sink::Response, Compression},
};

use super::request_builder::AppsignalRequest;

#[derive(Clone)]
pub(super) struct AppsignalService {
    // TODO: `HttpBatchService` has been deprecated for direct use in sinks.
    //       This sink should undergo a refactor to utilize the `HttpService`
    //       instead, which extracts much of the boilerplate code for `Service`.
    pub(super) batch_service:
        HttpBatchService<Ready<Result<http::Request<Bytes>, crate::Error>>, AppsignalRequest>,
}

impl AppsignalService {
    pub fn new(
        http_client: HttpClient<Body>,
        endpoint: Uri,
        push_api_key: SensitiveString,
        compression: Compression,
    ) -> Self {
        let batch_service = HttpBatchService::new(http_client, move |req| {
            let req: AppsignalRequest = req;

            let mut request = Request::post(&endpoint)
                .header("Content-Type", "application/json")
                .header(AUTHORIZATION, format!("Bearer {}", push_api_key.inner()))
                .header("Content-Length", req.payload.len());
            if let Some(ce) = compression.content_encoding() {
                request = request.header("Content-Encoding", ce)
            }
            let result = request.body(req.payload).map_err(|x| x.into());
            future::ready(result)
        });
        Self { batch_service }
    }
}

impl Service<AppsignalRequest> for AppsignalService {
    type Response = AppsignalResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut request: AppsignalRequest) -> Self::Future {
        let mut http_service = self.batch_service.clone();

        Box::pin(async move {
            let metadata = std::mem::take(request.metadata_mut());
            http_service.ready().await?;
            let bytes_sent = metadata.request_encoded_size();
            let event_byte_size = metadata.into_events_estimated_json_encoded_byte_size();
            let http_response = http_service.call(request).await?;
            let event_status = if http_response.is_successful() {
                EventStatus::Delivered
            } else if http_response.is_transient() {
                EventStatus::Errored
            } else {
                EventStatus::Rejected
            };
            Ok(AppsignalResponse {
                event_status,
                http_status: http_response.status(),
                event_byte_size,
                bytes_sent,
            })
        })
    }
}

pub struct AppsignalResponse {
    pub(super) event_status: EventStatus,
    pub(super) http_status: StatusCode,
    pub(super) event_byte_size: GroupedCountByteSize,
    pub(super) bytes_sent: usize,
}

impl DriverResponse for AppsignalResponse {
    fn event_status(&self) -> EventStatus {
        self.event_status
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.event_byte_size
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.bytes_sent)
    }
}
