use std::{
    marker::PhantomData,
    sync::Arc,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures::future::BoxFuture;
use http_1::{Request, Response, Uri};
use http_body_util::BodyExt;
use tower::{Service, service_fn};
use tower_http_1::decompression::Decompression;
use tracing::debug;
use vector_lib::{
    event::EventStatus,
    request_metadata::{GroupedCountByteSize, MetaDescriptive},
    stream::DriverResponse,
};

use crate::{
    http::client_v1::{HttpClient, HttpError, full_body},
    internal_events::{EndpointBytesSent, SinkRequestBuildError},
    sinks::util::{http::HttpRequest, sink},
};

/// Builds a native `http 1` request from a buffered HTTP sink request.
pub trait HttpServiceRequestBuilder<T: Send> {
    /// Builds the request to send for `request`.
    ///
    /// # Errors
    ///
    /// Returns an error if the request cannot be built, for example because a header value
    /// is invalid.
    fn build(&self, request: HttpRequest<T>) -> Result<Request<Bytes>, crate::Error>;
}

/// The response returned by the native `http 1` sink service.
#[derive(Debug)]
pub struct HttpResponse {
    pub response: Response<Bytes>,
    pub events_byte_size: GroupedCountByteSize,
    pub raw_byte_size: usize,
}

impl sink::Response for HttpResponse {
    fn is_successful(&self) -> bool {
        self.response.status().is_success()
    }

    fn is_transient(&self) -> bool {
        self.response.status().is_server_error()
    }
}

impl DriverResponse for HttpResponse {
    fn event_status(&self) -> EventStatus {
        if self.response.status().is_success() {
            EventStatus::Delivered
        } else if self.response.status().is_server_error() {
            EventStatus::Errored
        } else {
            EventStatus::Rejected
        }
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.events_byte_size
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.raw_byte_size)
    }
}

/// Native `http 1` equivalent of the legacy `HttpService`.
#[derive(Clone)]
pub struct HttpService<B, T: Send> {
    client: HttpClient,
    request_builder: Arc<B>,
    response_decompression: bool,
    _phantom: PhantomData<fn() -> T>,
}

impl<B, T> HttpService<B, T>
where
    B: HttpServiceRequestBuilder<T> + Send + Sync + 'static,
    T: Send + 'static,
{
    pub fn new(client: HttpClient, request_builder: B) -> Self {
        Self {
            client,
            request_builder: Arc::new(request_builder),
            response_decompression: false,
            _phantom: PhantomData,
        }
    }

    /// Requests and decodes compressed responses, matching legacy HTTP batch services.
    ///
    /// Other services continue to request identity encoding.
    #[must_use]
    pub const fn with_response_decompression(mut self) -> Self {
        self.response_decompression = true;
        self
    }
}

impl<B, T> Service<HttpRequest<T>> for HttpService<B, T>
where
    B: HttpServiceRequestBuilder<T> + Send + Sync + 'static,
    T: Send + 'static,
{
    type Response = HttpResponse;
    type Error = HttpError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut request: HttpRequest<T>) -> Self::Future {
        let request_builder = Arc::clone(&self.request_builder);
        let client = self.client.clone();
        let response_decompression = self.response_decompression;
        let metadata = std::mem::take(request.metadata_mut());
        let raw_byte_size = metadata.request_encoded_size();
        let events_byte_size = metadata.into_events_estimated_json_encoded_byte_size();

        Box::pin(async move {
            let request = request_builder
                .build(request)
                .inspect_err(|error| {
                    emit!(SinkRequestBuildError { error });
                })
                .map_err(HttpError::from)?;
            let byte_size = request.body().len();
            let (protocol, endpoint) = protocol_endpoint(request.uri());
            let request = request.map(full_body);
            let response = if response_decompression {
                let mut service = Decompression::new(service_fn(|request| client.send(request)));
                let response = service.call(request).await?;
                collect_response(response, byte_size, &protocol, &endpoint)
                    .await
                    .map_err(HttpError::from)?
            } else {
                let response = client.send(request).await?;
                collect_response(response, byte_size, &protocol, &endpoint)
                    .await
                    .map_err(HttpError::new)?
            };
            Ok(HttpResponse {
                response,
                events_byte_size,
                raw_byte_size,
            })
        })
    }
}

async fn collect_response<B>(
    response: Response<B>,
    byte_size: usize,
    protocol: &str,
    endpoint: &str,
) -> Result<Response<Bytes>, B::Error>
where
    B: http_body_1::Body<Data = Bytes>,
{
    if response.status().is_success() {
        emit!(EndpointBytesSent {
            byte_size,
            protocol,
            endpoint,
        });
    }

    let (parts, body) = response.into_parts();
    let body = body.collect().await?.to_bytes();
    Ok(Response::from_parts(parts, body))
}

/// Retry logic for sinks that use [`HttpService`], based on the response status code.
///
/// ClickHouse implements its own retry logic; other native http 1 sinks use this.
#[must_use]
pub fn http_response_retry_logic<Request: Clone + Send + Sync + 'static>(
    retry_strategy: super::http::RetryStrategy,
) -> super::http::HttpStatusRetryLogic<
    impl Fn(&HttpResponse) -> http_1::StatusCode + Clone + Send + Sync + 'static,
    Request,
    HttpResponse,
    HttpError,
> {
    super::http::HttpStatusRetryLogic::new_with_error(
        |response: &HttpResponse| {
            let status = response.response.status();
            if !status.is_success() {
                debug!(
                    message = "HTTP response.",
                    %status,
                    body = %String::from_utf8_lossy(
                        &response.response.body()[..response
                            .response
                            .body()
                            .len()
                            .min(super::http::MAX_ERROR_BODY_BYTES)],
                    ),
                );
            }
            status
        },
        retry_strategy,
    )
}

fn protocol_endpoint(uri: &Uri) -> (String, String) {
    let uri = uri
        .to_string()
        .parse::<http::Uri>()
        .expect("a valid HTTP/1 URI is valid as an HTTP URI");
    super::uri::protocol_endpoint(uri)
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, io::Write};

    use flate2::write::GzEncoder;
    use vector_lib::{finalization::EventFinalizers, request_metadata::RequestMetadata};

    use super::*;
    use crate::{
        config::ProxyConfig,
        sinks::util::{
            http::RetryStrategy,
            retries::{RetryAction, RetryLogic},
        },
        test_util::http::spawn_blackhole_http_server,
        tls::MaybeTlsSettings,
    };

    #[derive(Clone)]
    struct RequestBuilder {
        uri: Uri,
    }

    impl HttpServiceRequestBuilder<()> for RequestBuilder {
        fn build(&self, mut request: HttpRequest<()>) -> Result<Request<Bytes>, crate::Error> {
            Ok(Request::post(self.uri.clone()).body(request.take_payload())?)
        }
    }

    async fn compressed_response_service(body: Vec<u8>) -> HttpService<RequestBuilder, ()> {
        let uri = spawn_blackhole_http_server(move |request| {
            let body = body.clone();
            async move {
                assert_eq!(request.headers()["accept-encoding"], "zstd,gzip,deflate,br");
                let request_body = http_body::Body::collect(request.into_body())
                    .await
                    .unwrap()
                    .to_bytes();
                assert_eq!(request_body, "events");
                Ok::<_, Infallible>(
                    http::Response::builder()
                        .status(http::StatusCode::SERVICE_UNAVAILABLE)
                        .header("content-encoding", "gzip")
                        .header("content-length", body.len())
                        .body(hyper::Body::from(body))
                        .unwrap(),
                )
            }
        })
        .await;
        let client = HttpClient::new(
            MaybeTlsSettings::from_config(None, false).unwrap(),
            &ProxyConfig::default(),
        )
        .unwrap();
        let builder = RequestBuilder {
            uri: crate::sinks::util::HttpEndpoint::new(uri)
                .unwrap()
                .into_v1(),
        };
        HttpService::new(client, builder).with_response_decompression()
    }

    fn request() -> HttpRequest<()> {
        HttpRequest::new(
            Bytes::from_static(b"events"),
            EventFinalizers::default(),
            RequestMetadata::default(),
            (),
        )
    }

    #[tokio::test]
    async fn decompresses_retry_response() {
        let message = b"retry after the service has recovered";
        let mut encoder = GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(message).unwrap();
        let mut service = compressed_response_service(encoder.finish().unwrap()).await;
        let response = service.call(request()).await.unwrap();

        assert_eq!(
            response.response.status(),
            http_1::StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(response.response.body().as_ref(), message);
        assert!(!response.response.headers().contains_key("content-encoding"));
        assert!(!response.response.headers().contains_key("content-length"));
        let retry_logic = http_response_retry_logic::<HttpRequest<()>>(RetryStrategy::Default);
        assert!(matches!(
            retry_logic.should_retry_response(&response),
            RetryAction::Retry(_)
        ));
    }

    #[tokio::test]
    async fn rejects_invalid_compressed_response() {
        let mut service = compressed_response_service(b"not a gzip response".to_vec()).await;
        let error = service.call(request()).await.unwrap_err();
        let retry_logic = http_response_retry_logic::<HttpRequest<()>>(RetryStrategy::Default);
        assert!(!retry_logic.is_retriable_error(&error));
    }
}
