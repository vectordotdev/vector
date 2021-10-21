use crate::sinks::datadog::ApiKey;

use crate::sinks::util::http::HttpBatchService;

use crate::event::EventStatus;

use crate::emit;
use crate::internal_events::EventsSent;
use http::Request;
use std::sync::Arc;

use crate::http::HttpClient;
use crate::sinks::datadog::events::request_builder::DatadogEventsRequest;
use crate::sinks::util::sink::Response;
use futures::future;
use futures::future::BoxFuture;
use futures::future::Ready;
use hyper::Body;
use std::task::{Context, Poll};
use tower::{Service, ServiceExt};

pub struct DatadogEventsResponse {
    pub event_status: EventStatus,
    pub http_status: http::StatusCode,
}

impl AsRef<EventStatus> for DatadogEventsResponse {
    fn as_ref(&self) -> &EventStatus {
        &self.event_status
    }
}

#[derive(Clone)]
pub struct DatadogEventsService {
    uri: String,
    default_api_key: ApiKey,
    batch_http_service:
        HttpBatchService<Ready<Result<http::Request<Vec<u8>>, crate::Error>>, DatadogEventsRequest>,
}

impl DatadogEventsService {
    pub fn new(uri: &str, default_api_key: &str, http_client: HttpClient<Body>) -> Self {
        let owned_uri = uri.to_owned();
        let default_api_key = default_api_key.to_owned();
        let default_api_key_clone = default_api_key.clone();
        let batch_http_service = HttpBatchService::new(http_client, move |req| {
            let req: DatadogEventsRequest = req;

            let api_key = match req.metadata.api_key.as_ref() {
                Some(x) => x.as_ref(),
                None => &default_api_key_clone,
            };

            let request = Request::post(owned_uri.as_str())
                .header("Content-Type", "application/json")
                .header("DD-API-KEY", api_key)
                .header("Content-Length", req.body.len())
                .body(req.body)
                .map_err(|x| x.into());
            future::ready(request)
        });
        Self {
            default_api_key: Arc::from(default_api_key),
            uri: uri.to_owned(),
            batch_http_service,
        }
    }
}

impl Service<DatadogEventsRequest> for DatadogEventsService {
    type Response = DatadogEventsResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: DatadogEventsRequest) -> Self::Future {
        let mut http_service = self.batch_http_service.clone();

        Box::pin(async move {
            http_service.ready().await?;
            let event_byte_size = req.metadata.event_byte_size;
            let http_response = http_service.call(req).await?;
            let event_status = if http_response.is_successful() {
                EventStatus::Delivered
            } else if http_response.is_transient() {
                EventStatus::Errored
            } else {
                EventStatus::Failed
            };
            if event_status == EventStatus::Delivered {
                emit!(&EventsSent {
                    count: 1,
                    byte_size: event_byte_size
                });
            }
            Ok(DatadogEventsResponse {
                event_status,
                http_status: http_response.status(),
            })
        })
    }
}
