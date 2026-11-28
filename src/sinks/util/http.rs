use std::{
    collections::BTreeMap,
    marker::PhantomData,
    sync::Arc,
    task::{Context, Poll},
};

#[cfg(feature = "aws-core")]
use aws_credential_types::provider::SharedCredentialsProvider;
#[cfg(feature = "aws-core")]
use aws_types::region::Region;
use bytes::Bytes;
use futures::future::BoxFuture;
use http::{HeaderName, HeaderValue, Request, Response, StatusCode, header};
use http_body_util::BodyExt;
use snafu::{ResultExt, Snafu};
use tower::{Service, service_fn};
use tower_http::decompression::Decompression;
use tracing::debug;
use vector_http::{HttpError, full_body};
use vector_lib::{
    ByteSizeOf, EstimatedJsonEncodedSizeOf,
    configurable::configurable_component,
    event::EventStatus,
    internal_event::emit,
    request_metadata::{GroupedCountByteSize, MetaDescriptive},
    stream::{DriverResponse, batcher::limiter::ItemBatchSize},
};

use super::{
    TowerRequestConfig,
    retries::{RetryAction, RetryLogic},
    sink,
    uri::protocol_endpoint,
};
use crate::{
    event::Event,
    http::HttpClient,
    internal_events::{EndpointBytesSent, SinkRequestBuildError},
    sinks::prelude::*,
};

/// Maximum number of response body bytes to include in a non-retriable error's reason.
pub(crate) const MAX_ERROR_BODY_BYTES: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OrderedHeaderName(HeaderName);

impl OrderedHeaderName {
    pub const fn new(header_name: HeaderName) -> Self {
        Self(header_name)
    }

    pub const fn inner(&self) -> &HeaderName {
        &self.0
    }
}

impl From<HeaderName> for OrderedHeaderName {
    fn from(header_name: HeaderName) -> Self {
        Self(header_name)
    }
}

impl Ord for OrderedHeaderName {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.as_str().cmp(other.0.as_str())
    }
}

impl PartialOrd for OrderedHeaderName {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(feature = "aws-core")]
#[derive(Clone)]
pub struct SigV4Config {
    pub(crate) shared_credentials_provider: SharedCredentialsProvider,
    pub(crate) region: Region,
    pub(crate) service: String,
}

/// Serializes and deserializes a [`Vec<StatusCode>`]
mod status_code_vec {
    use http::StatusCode;
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};

    /// Deserializes a [`Vec<StatusCode>`]
    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<Vec<StatusCode>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Vec::<u16>::deserialize(deserializer)?
            .into_iter()
            .map(StatusCode::from_u16)
            .collect::<Result<Vec<_>, _>>()
            .map_err(Error::custom)
    }

    /// Serializes a [`Vec<StatusCode>`]
    pub(super) fn serialize<S>(
        status_codes: &[StatusCode],
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        status_codes
            .iter()
            .map(StatusCode::as_u16)
            .collect::<Vec<_>>()
            .serialize(serializer)
    }
}

/// Configurable retry strategy for `http` based sinks.
///
/// For more information about error responses, see [Client Error Responses][error_responses].
///
/// [error_responses]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Status#client_error_responses
#[configurable_component]
#[derive(Debug, Clone, Default, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[configurable(metadata(docs::enum_tag_description = "The retry strategy enum."))]
pub enum RetryStrategy {
    /// Don't retry any errors, including request timeouts.
    None,

    /// Default strategy. See [`RetryStrategy::retry_action`] for more details.
    #[default]
    Default,

    /// Retry on *all* HTTP status codes except for success codes (2xx)
    All,

    /// Custom retry strategy
    Custom {
        /// Retry on these specific HTTP status codes
        #[serde(with = "status_code_vec")]
        status_codes: Vec<StatusCode>,
    },
}

impl RetryStrategy {
    /// Returns the name of the retry strategy.
    #[must_use]
    const fn name(&self) -> &str {
        match self {
            Self::None => "Never retry strategy",
            Self::Default => "Default retry strategy",
            Self::All => "Retry all strategy",
            Self::Custom { .. } => "Custom retry strategy",
        }
    }

    /// Determines if the given status code should be retried.
    ///
    /// For the `Default` strategy, the following status codes will be retried:
    /// - 429 (Too Many Requests)
    /// - 408 (Request Timeout)
    /// - 5xx (Server Error)
    ///
    /// For the `Custom` strategy, the status codes specified in the `status_codes` field will be retried.
    ///
    /// For the `All` strategy, all non-success status codes will be retried.
    #[must_use]
    pub fn retry_action<Req>(&self, status: StatusCode) -> RetryAction<Req> {
        if status.is_success() {
            return RetryAction::Successful;
        }

        let reason = format!(
            "{}: {}",
            self.name(),
            status.canonical_reason().unwrap_or_else(|| status.as_str())
        )
        .into();

        match self {
            Self::None => RetryAction::DontRetry(reason),
            Self::Default => match status {
                StatusCode::TOO_MANY_REQUESTS | StatusCode::REQUEST_TIMEOUT => {
                    RetryAction::Retry(reason)
                }
                StatusCode::NOT_IMPLEMENTED => RetryAction::DontRetry(reason),
                _ => {
                    if status.is_server_error() {
                        RetryAction::Retry(reason)
                    } else {
                        RetryAction::DontRetry(reason)
                    }
                }
            },
            Self::All => RetryAction::Retry(reason),
            Self::Custom { status_codes } => {
                if status_codes.contains(&status) {
                    RetryAction::Retry(reason)
                } else {
                    RetryAction::DontRetry(reason)
                }
            }
        }
    }
}

/// Retry logic driven by a status code extracted from each response.
#[derive(Debug)]
pub struct HttpStatusRetryLogic<F, Req, Res> {
    func: F,
    request: PhantomData<Req>,
    response: PhantomData<Res>,
    retry_strategy: RetryStrategy,
}

impl<F, Req, Res> HttpStatusRetryLogic<F, Req, Res>
where
    F: Fn(&Res) -> StatusCode + Clone + Send + Sync + 'static,
    Req: Send + Sync + 'static,
    Res: Send + Sync + 'static,
{
    pub const fn new(func: F, retry_strategy: RetryStrategy) -> Self {
        HttpStatusRetryLogic {
            func,
            request: PhantomData,
            response: PhantomData,
            retry_strategy,
        }
    }
}

impl<F, Req, Res> RetryLogic for HttpStatusRetryLogic<F, Req, Res>
where
    F: Fn(&Res) -> StatusCode + Clone + Send + Sync + 'static,
    Req: Send + Sync + 'static,
    Res: Send + Sync + 'static,
{
    type Error = HttpError;
    type Request = Req;
    type Response = Res;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        if self.retry_strategy == RetryStrategy::None {
            false
        } else {
            error.is_retriable()
        }
    }

    fn is_retriable_timeout(&self) -> bool {
        self.retry_strategy != RetryStrategy::None
    }

    fn should_retry_response(&self, response: &Res) -> RetryAction<Req> {
        let status = (self.func)(response);
        self.retry_strategy.retry_action(status)
    }
}

impl<F, Req, Res> Clone for HttpStatusRetryLogic<F, Req, Res>
where
    F: Clone,
{
    fn clone(&self) -> Self {
        Self {
            func: self.func.clone(),
            request: PhantomData,
            response: PhantomData,
            retry_strategy: self.retry_strategy.clone(),
        }
    }
}

/// Outbound HTTP request settings.
#[configurable_component]
#[derive(Clone, Debug, Default)]
pub struct RequestConfig {
    #[serde(flatten)]
    pub tower: TowerRequestConfig,

    /// Additional HTTP headers to add to every HTTP request.
    #[serde(default)]
    #[configurable(metadata(
        docs::additional_props_description = "An HTTP request header and its value. Both header names and values support templating with event data."
    ))]
    #[configurable(metadata(docs::examples = "headers_examples()"))]
    pub headers: BTreeMap<String, String>,
}

fn headers_examples() -> BTreeMap<String, String> {
    btreemap! {
        "Accept" => "text/plain",
        "X-My-Custom-Header" => "A-Value",
        "X-Event-Level" => "{{level}}",
        "X-Event-Timestamp" => "{{timestamp}}",
    }
}

impl RequestConfig {
    /// Split headers into static (non-dynamic) and template (dynamic) maps.
    #[must_use]
    pub fn split_headers(&self) -> (BTreeMap<String, String>, BTreeMap<String, Template>) {
        let mut static_headers = BTreeMap::new();
        let mut template_headers = BTreeMap::new();

        for (name, value) in &self.headers {
            match Template::try_from(value.as_str()) {
                Ok(template) if !template.is_dynamic() => {
                    static_headers.insert(name.clone(), value.clone());
                }
                Ok(template) => {
                    template_headers.insert(name.clone(), template);
                }
                Err(_) => {
                    static_headers.insert(name.clone(), value.clone());
                }
            }
        }

        (static_headers, template_headers)
    }
}

#[derive(Debug, Snafu)]
pub enum HeaderValidationError {
    #[snafu(display("{}: {}", source, name))]
    InvalidHeaderName {
        name: String,
        source: header::InvalidHeaderName,
    },
    #[snafu(display("{}: {}", source, value))]
    InvalidHeaderValue {
        value: String,
        source: header::InvalidHeaderValue,
    },
}

/// Validates HTTP header names and values.
///
/// # Errors
///
/// Returns an error if a header name or value is invalid.
pub fn validate_headers(
    headers: &BTreeMap<String, String>,
) -> crate::Result<BTreeMap<OrderedHeaderName, HeaderValue>> {
    let mut validated_headers = BTreeMap::new();
    for (name, value) in headers {
        let name = HeaderName::from_bytes(name.as_bytes())
            .with_context(|_| InvalidHeaderNameSnafu { name })?;
        let value = HeaderValue::from_bytes(value.as_bytes())
            .with_context(|_| InvalidHeaderValueSnafu { value })?;

        validated_headers.insert(name.into(), value);
    }

    Ok(validated_headers)
}

/// Request type for use in the `Service` implementation of HTTP stream sinks.
#[derive(Debug, Clone)]
pub struct HttpRequest<T: Send> {
    payload: Bytes,
    finalizers: EventFinalizers,
    request_metadata: RequestMetadata,
    additional_metadata: T,
}

impl<T: Send> HttpRequest<T> {
    /// Creates a new `HttpRequest`.
    pub const fn new(
        payload: Bytes,
        finalizers: EventFinalizers,
        request_metadata: RequestMetadata,
        additional_metadata: T,
    ) -> Self {
        Self {
            payload,
            finalizers,
            request_metadata,
            additional_metadata,
        }
    }

    pub const fn get_additional_metadata(&self) -> &T {
        &self.additional_metadata
    }

    pub fn take_payload(&mut self) -> Bytes {
        std::mem::take(&mut self.payload)
    }
}

impl<T: Send> Finalizable for HttpRequest<T> {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.finalizers.take_finalizers()
    }
}

impl<T: Send> MetaDescriptive for HttpRequest<T> {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.request_metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.request_metadata
    }
}

impl<T: Send> ByteSizeOf for HttpRequest<T> {
    fn allocated_bytes(&self) -> usize {
        self.payload.allocated_bytes() + self.finalizers.allocated_bytes()
    }
}

/// Uses the estimated json encoded size to determine batch sizing.
#[derive(Default)]
pub struct HttpJsonBatchSizer;

impl ItemBatchSize<Event> for HttpJsonBatchSizer {
    fn size(&self, item: &Event) -> usize {
        item.estimated_json_encoded_size_of().get()
    }
}

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
///
/// Requests advertise `Accept-Encoding: zstd,gzip,deflate,br` unless the request builder set the
/// header, and compressed responses are decoded before retry logic reads them.
#[derive(Clone)]
pub struct HttpService<B, T: Send> {
    client: HttpClient,
    request_builder: Arc<B>,
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
}

#[cfg(feature = "aws-core")]
async fn sign_request(
    mut request: Request<Bytes>,
    sig_v4_config: Option<&SigV4Config>,
) -> Result<Request<Bytes>, HttpError> {
    if let Some(sig_v4_config) = sig_v4_config {
        crate::aws::sign_request(
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
        #[cfg(feature = "aws-core")]
        let sig_v4_config = self.sig_v4_config.clone();
        let metadata = std::mem::take(request.metadata_mut());
        let raw_byte_size = metadata.request_encoded_size();
        let events_byte_size = metadata.into_events_estimated_json_encoded_byte_size();

        Box::pin(async move {
            let request = request_builder
                .build(request)
                .inspect_err(|error| {
                    emit(SinkRequestBuildError { error });
                })
                .map_err(HttpError::from)?;
            #[cfg(feature = "aws-core")]
            let request = sign_request(request, sig_v4_config.as_deref()).await?;
            let byte_size = request.body().len();
            let (protocol, endpoint) = protocol_endpoint(request.uri().clone());
            let request = request.map(full_body);
            let mut service = Decompression::new(service_fn(|request| client.send(request)));
            let response = service.call(request).await?;
            let response = collect_response(response, byte_size, &protocol, &endpoint)
                .await
                .map_err(|error| {
                    // Decompression preserves transport errors but boxes them alongside decoder errors.
                    if error.is::<hyper::Error>() {
                        HttpError::new(error)
                    } else {
                        HttpError::from(error)
                    }
                })?;
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
    B: http_body::Body<Data = Bytes>,
{
    if response.status().is_success() {
        emit(EndpointBytesSent {
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
    retry_strategy: RetryStrategy,
) -> HttpStatusRetryLogic<
    impl Fn(&HttpResponse) -> http::StatusCode + Clone + Send + Sync + 'static,
    Request,
    HttpResponse,
> {
    HttpStatusRetryLogic::new(
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
                            .min(MAX_ERROR_BODY_BYTES)],
                    ),
                );
            }
            status
        },
        retry_strategy,
    )
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, io::Write};

    use flate2::write::GzEncoder;
    use http_body_util::Full;
    use tokio::{
        io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
        net::TcpListener,
    };
    use vector_lib::{finalization::EventFinalizers, request_metadata::RequestMetadata};

    use super::*;
    use crate::{
        config::ProxyConfig, test_util::http::spawn_blackhole_http_server, tls::MaybeTlsSettings,
    };

    #[derive(Clone)]
    struct RequestBuilder {
        uri: http::Uri,
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
                let request_body = http_body_util::BodyExt::collect(request.into_body())
                    .await
                    .unwrap()
                    .to_bytes();
                assert_eq!(request_body, "events");
                Ok::<_, Infallible>(
                    http::Response::builder()
                        .status(http::StatusCode::SERVICE_UNAVAILABLE)
                        .header("content-encoding", "gzip")
                        .header("content-length", body.len())
                        .body(Full::new(Bytes::from(body)))
                        .unwrap(),
                )
            }
        })
        .await;
        HttpService::new(client(), request_builder(&uri))
    }

    fn client() -> HttpClient {
        HttpClient::new(
            MaybeTlsSettings::from_config(None, false).unwrap(),
            &ProxyConfig::default(),
        )
        .unwrap()
    }

    fn request_builder(uri: &http::Uri) -> RequestBuilder {
        RequestBuilder {
            uri: crate::sinks::util::HttpEndpoint::new(uri.clone())
                .unwrap()
                .into_uri(),
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
            http::StatusCode::SERVICE_UNAVAILABLE
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

    #[rstest::rstest]
    #[case::plain(false)]
    #[case::gzip(true)]
    #[tokio::test]
    async fn retries_incomplete_response_body(#[case] compressed: bool) {
        let message = b"retry after the service has recovered";
        let body = if compressed {
            let mut encoder = GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(message).unwrap();
            encoder.finish().unwrap()
        } else {
            message.to_vec()
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let uri = format!("http://{}/", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut stream = BufReader::new(stream);
            let mut line = String::new();
            loop {
                line.clear();
                assert_ne!(stream.read_line(&mut line).await.unwrap(), 0);
                if line == "\r\n" {
                    break;
                }
            }
            let mut request_body = [0; 6];
            stream.read_exact(&mut request_body).await.unwrap();
            assert_eq!(&request_body, b"events");

            let encoding = if compressed {
                "content-encoding: gzip\r\n"
            } else {
                ""
            };
            let headers = format!(
                "HTTP/1.1 200 OK\r\ncontent-length: {}\r\n{encoding}connection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(headers.as_bytes()).await.unwrap();
            stream.write_all(&body[..body.len() / 2]).await.unwrap();
            stream.shutdown().await.unwrap();
        });
        let mut service = HttpService::new(client(), request_builder(&uri));
        let error =
            tokio::time::timeout(std::time::Duration::from_secs(5), service.call(request()))
                .await
                .unwrap()
                .unwrap_err();
        server.await.unwrap();

        let retry_logic = http_response_retry_logic::<HttpRequest<()>>(RetryStrategy::Default);
        assert!(retry_logic.is_retriable_error(&error), "{error:?}");
        let no_retry_logic = http_response_retry_logic::<HttpRequest<()>>(RetryStrategy::None);
        assert!(!no_retry_logic.is_retriable_error(&error));
    }

    #[tokio::test]
    async fn rejects_invalid_compressed_response() {
        let mut service = compressed_response_service(b"not a gzip response".to_vec()).await;
        let error = service.call(request()).await.unwrap_err();
        let retry_logic = http_response_retry_logic::<HttpRequest<()>>(RetryStrategy::Default);
        assert!(!retry_logic.is_retriable_error(&error));
    }

    #[test]
    fn retry_strategy_none_preserves_success_and_rejects_failures() {
        let strategy = RetryStrategy::None;

        assert!(strategy.retry_action::<()>(StatusCode::OK).is_successful());
        assert!(
            strategy
                .retry_action::<()>(StatusCode::INTERNAL_SERVER_ERROR)
                .is_not_retryable()
        );
    }

    #[test]
    fn retry_strategy_none_disables_timeout_retries() {
        let status_logic =
            HttpStatusRetryLogic::<_, (), ()>::new(|(): &()| StatusCode::OK, RetryStrategy::None);

        assert!(!status_logic.is_retriable_timeout());
    }

    #[test]
    fn retry_strategy_all_preserves_success_and_retries_failures() {
        let strategy = RetryStrategy::All;

        assert!(strategy.retry_action::<()>(StatusCode::OK).is_successful());
        assert!(
            strategy
                .retry_action::<()>(StatusCode::BAD_REQUEST)
                .is_retryable()
        );
        assert!(
            strategy
                .retry_action::<()>(StatusCode::INTERNAL_SERVER_ERROR)
                .is_retryable()
        );
    }

    #[test]
    fn retry_strategy_custom_only_retries_configured_statuses() {
        let strategy = RetryStrategy::Custom {
            status_codes: vec![StatusCode::BAD_REQUEST],
        };

        assert!(strategy.retry_action::<()>(StatusCode::OK).is_successful());
        assert!(
            strategy
                .retry_action::<()>(StatusCode::BAD_REQUEST)
                .is_retryable()
        );
        assert!(
            strategy
                .retry_action::<()>(StatusCode::INTERNAL_SERVER_ERROR)
                .is_not_retryable()
        );
    }

    #[test]
    fn retry_strategy_custom_serde_roundtrips_status_codes() {
        let json = r#"{"type":"custom","status_codes":[400,503]}"#;
        let strategy: RetryStrategy = serde_json::from_str(json).unwrap();
        assert_eq!(
            strategy,
            RetryStrategy::Custom {
                status_codes: vec![StatusCode::BAD_REQUEST, StatusCode::SERVICE_UNAVAILABLE],
            }
        );
        let encoded = serde_json::to_string(&strategy).unwrap();
        let roundtrip: RetryStrategy = serde_json::from_str(&encoded).unwrap();
        assert_eq!(roundtrip, strategy);
    }

    #[test]
    fn retry_strategy_custom_serde_rejects_invalid_status_codes() {
        // `StatusCode::from_u16` only accepts 100–999; 1000 is out of range.
        let json = r#"{"type":"custom","status_codes":[1000]}"#;
        let result = serde_json::from_str::<RetryStrategy>(json);
        assert!(
            result.is_err(),
            "expected invalid status code to fail deserialization"
        );
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
            uri: &http::Uri,
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
        }

        #[tokio::test]
        async fn signs_request_before_response_decompression() {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let uri = spawn_blackhole_http_server(move |request| {
                let tx = tx.clone();
                async move {
                    let (parts, body) = request.into_parts();
                    let body = http_body_util::BodyExt::collect(body)
                        .await
                        .unwrap()
                        .to_bytes();
                    tx.send((parts.headers, body)).unwrap();
                    Ok::<_, Infallible>(http::Response::new(Full::default()))
                }
            })
            .await;
            let mut service = sig_v4_service(&uri, Credentials::from_keys("AKID", "SECRET", None));

            let response = service.call(request()).await.unwrap();
            assert_eq!(response.response.status(), http::StatusCode::OK);

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
                async { Ok::<_, Infallible>(http::Response::new(Full::default())) }
            })
            .await;
            let mut service = sig_v4_service(&uri, MissingCredentials);

            let error = service.call(request()).await.unwrap_err();
            let retry_logic = http_response_retry_logic::<HttpRequest<()>>(RetryStrategy::Default);
            assert!(!retry_logic.is_retriable_error(&error));
            assert_eq!(requests.load(Ordering::Relaxed), 0);
        }
    }
}
