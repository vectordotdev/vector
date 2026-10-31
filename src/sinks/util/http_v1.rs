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
use vector_http::{HttpError, full_body};
use vector_lib::{
    event::EventStatus,
    request_metadata::{GroupedCountByteSize, MetaDescriptive},
    stream::DriverResponse,
};

#[cfg(feature = "aws-core")]
use super::http::SigV4Config;
use crate::{
    http::client_v1::HttpClient,
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

/// Generic `Service` implementation for HTTP stream sinks.
#[derive(Clone)]
pub struct HttpService<B, T: Send> {
    client: HttpClient,
    request_builder: Arc<B>,
    response_decompression: bool,
    #[cfg(feature = "aws-core")]
    sig_v4_config: Option<Arc<SigV4Config>>,
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
            #[cfg(feature = "aws-core")]
            sig_v4_config: None,
            _phantom: PhantomData,
        }
    }

    /// Creates a service that signs every built request with AWS `SigV4` before sending it.
    ///
    /// The signature covers the method, URI, headers and body produced by `request_builder`,
    /// without adding an `x-amz-content-sha256` header. A request that cannot be signed fails
    /// with a non-retriable [`HttpError`].
    #[cfg(feature = "aws-core")]
    pub fn new_with_sig_v4(
        client: HttpClient,
        request_builder: B,
        sig_v4_config: SigV4Config,
    ) -> Self {
        Self {
            sig_v4_config: Some(Arc::new(sig_v4_config)),
            ..Self::new(client, request_builder)
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

#[cfg(feature = "aws-core")]
async fn sign_request(
    mut request: Request<Bytes>,
    sig_v4_config: Option<&SigV4Config>,
) -> Result<Request<Bytes>, HttpError> {
    if let Some(sig_v4_config) = sig_v4_config {
        crate::aws::sign_request_v1(
            &sig_v4_config.service,
            &mut request,
            &sig_v4_config.shared_credentials_provider,
            Some(&sig_v4_config.region),
            false,
        )
        .await
        .map_err(HttpError::from)?;
    }
    Ok(request)
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
        #[cfg(feature = "aws-core")]
        let sig_v4_config = self.sig_v4_config.clone();
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
            #[cfg(feature = "aws-core")]
            let request = sign_request(request, sig_v4_config.as_deref()).await?;
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
> {
    super::http::HttpStatusRetryLogic::new(
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

pub(crate) fn protocol_endpoint(uri: &Uri) -> (String, String) {
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
        HttpService::new(client(), request_builder(uri)).with_response_decompression()
    }

    fn client() -> HttpClient {
        HttpClient::new(
            MaybeTlsSettings::from_config(None, false).unwrap(),
            &ProxyConfig::default(),
        )
        .unwrap()
    }

    fn request_builder(uri: http::Uri) -> RequestBuilder {
        RequestBuilder {
            uri: crate::sinks::util::HttpEndpoint::new(uri)
                .unwrap()
                .into_v1(),
        }
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

    #[cfg(feature = "aws-core")]
    mod sig_v4 {
        use std::sync::atomic::{AtomicUsize, Ordering};

        use aws_credential_types::{
            Credentials,
            provider::{
                ProvideCredentials, SharedCredentialsProvider, error::CredentialsError, future,
            },
        };
        use aws_types::region::Region;

        use super::*;

        fn sig_v4_service(
            uri: http::Uri,
            credentials: impl ProvideCredentials + 'static,
        ) -> HttpService<RequestBuilder, ()> {
            HttpService::new_with_sig_v4(
                client(),
                request_builder(uri),
                SigV4Config {
                    shared_credentials_provider: SharedCredentialsProvider::new(credentials),
                    region: Region::new("us-east-1"),
                    service: "example-service".to_owned(),
                },
            )
            .with_response_decompression()
        }

        #[tokio::test]
        async fn signs_request_before_response_decompression() {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let uri = spawn_blackhole_http_server(move |request| {
                let tx = tx.clone();
                async move {
                    let (parts, body) = request.into_parts();
                    let body = http_body::Body::collect(body).await.unwrap().to_bytes();
                    tx.send((parts.headers, body)).unwrap();
                    Ok::<_, Infallible>(http::Response::new(hyper::Body::empty()))
                }
            })
            .await;
            let mut service = sig_v4_service(uri, Credentials::from_keys("AKID", "SECRET", None));

            let response = service.call(request()).await.unwrap();
            assert_eq!(response.response.status(), http_1::StatusCode::OK);

            let (headers, body) = rx.recv().await.unwrap();
            assert_eq!(body, "events");
            assert_eq!(headers["accept-encoding"], "zstd,gzip,deflate,br");
            assert!(headers.contains_key("x-amz-date"));
            assert!(!headers.contains_key("x-amz-content-sha256"));
            let authorization = headers["authorization"].to_str().unwrap();
            assert!(
                authorization.starts_with("AWS4-HMAC-SHA256 Credential=AKID/")
                    && authorization.contains("/us-east-1/example-service/aws4_request,"),
                "unexpected authorization: {authorization}"
            );
            let signed_headers = authorization
                .split_once("SignedHeaders=")
                .and_then(|(_, rest)| rest.split_once(','))
                .map(|(signed_headers, _)| signed_headers)
                .unwrap();
            assert!(
                signed_headers
                    .split(';')
                    .all(|name| name != "accept-encoding"),
                "response decompression headers must not be signed: {signed_headers}"
            );
        }

        #[derive(Debug)]
        struct MissingCredentials;

        impl ProvideCredentials for MissingCredentials {
            fn provide_credentials<'a>(&'a self) -> future::ProvideCredentials<'a>
            where
                Self: 'a,
            {
                future::ProvideCredentials::ready(Err(CredentialsError::not_loaded(
                    "no credentials",
                )))
            }
        }

        #[tokio::test]
        async fn unsigned_request_is_not_sent_or_retried() {
            let requests = Arc::new(AtomicUsize::new(0));
            let received = Arc::clone(&requests);
            let uri = spawn_blackhole_http_server(move |_| {
                received.fetch_add(1, Ordering::Relaxed);
                async { Ok::<_, Infallible>(http::Response::new(hyper::Body::empty())) }
            })
            .await;
            let mut service = sig_v4_service(uri, MissingCredentials);

            let error = service.call(request()).await.unwrap_err();
            let retry_logic = http_response_retry_logic::<HttpRequest<()>>(RetryStrategy::Default);
            assert!(!retry_logic.is_retriable_error(&error));
            assert_eq!(requests.load(Ordering::Relaxed), 0);
        }
    }
}
