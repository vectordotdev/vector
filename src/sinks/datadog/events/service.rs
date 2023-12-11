use std::task::{Context, Poll};

use bytes::Bytes;
use futures::{
    future,
    future::{BoxFuture, Ready},
};
use http::Request;
use hyper::Body;
use tower::{Service, ServiceExt};
use vector_lib::stream::DriverResponse;
use vector_lib::{
    request_metadata::{GroupedCountByteSize, MetaDescriptive},
    sensitive_string::SensitiveString,
};

use crate::{
    event::EventStatus,
    http::HttpClient,
    sinks::{
        datadog::events::request_builder::DatadogEventsRequest,
        util::{http::HttpBatchService, sink::Response},
    },
};

pub struct DatadogEventsResponse {
    pub(self) event_status: EventStatus,
    pub http_status: http::StatusCode,
    pub event_byte_size: GroupedCountByteSize,
}

impl DriverResponse for DatadogEventsResponse {
    fn event_status(&self) -> EventStatus {
        self.event_status
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.event_byte_size
    }

    fn bytes_sent(&self) -> Option<usize> {
        // HttpBatchService emits EndpointBytesSend
        None
    }
}

#[derive(Clone)]
pub struct DatadogEventsService {
    // TODO: `HttpBatchService` has been deprecated for direct use in sinks.
    //       This sink should undergo a refactor to utilize the `HttpService`
    //       instead, which extracts much of the boilerplate code for `Service`.
    batch_http_service:
        HttpBatchService<Ready<Result<http::Request<Bytes>, crate::Error>>, DatadogEventsRequest>,
}

impl DatadogEventsService {
    pub fn new(
        endpoint: http::Uri,
        default_api_key: SensitiveString,
        http_client: HttpClient<Body>,
    ) -> Self {
        let batch_http_service = HttpBatchService::new(http_client, move |req| {
            let req: DatadogEventsRequest = req;

            let api_key = match req.metadata.api_key.as_ref() {
                Some(x) => x.as_ref(),
                None => default_api_key.inner(),
            };

            let request = Request::post(&endpoint)
                .header("Content-Type", "application/json")
                .header("DD-API-KEY", api_key)
                .header("Content-Length", req.body.len())
                .body(req.body)
                .map_err(|x| x.into());
            future::ready(request)
        });

        Self { batch_http_service }
    }
}

impl Service<DatadogEventsRequest> for DatadogEventsService {
    type Response = DatadogEventsResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    // Emission of Error internal event is handled upstream by the caller
    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of Error internal event is handled upstream by the caller
    fn call(&mut self, mut req: DatadogEventsRequest) -> Self::Future {
        let mut http_service = self.batch_http_service.clone();

        Box::pin(async move {
            let metadata = std::mem::take(req.metadata_mut());
            http_service.ready().await?;
            let event_byte_size = metadata.into_events_estimated_json_encoded_byte_size();
            let http_response = http_service.call(req).await?;
            let event_status = if http_response.is_successful() {
                EventStatus::Delivered
            } else if http_response.is_transient() {
                EventStatus::Errored
            } else {
                EventStatus::Rejected
            };
            Ok(DatadogEventsResponse {
                event_status,
                http_status: http_response.status(),
                event_byte_size,
            })
        })
    }
}
