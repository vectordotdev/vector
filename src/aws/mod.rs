//! Shared functionality for the AWS components.
use vector_lib::internal_event::emit;
pub mod auth;
pub(crate) mod http_client;
pub mod region;
pub mod timeout;

use std::{
    error::Error,
    pin::Pin,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, ready},
    time::{Duration, Instant, SystemTime},
};

pub use auth::{AwsAuthentication, ImdsAuthentication};
use aws_config::{
    Region, SdkConfig, meta::region::ProvideRegion, retry::RetryConfig, timeout::TimeoutConfig,
};
use aws_credential_types::provider::{ProvideCredentials, SharedCredentialsProvider};
use aws_sigv4::{
    http_request::{PayloadChecksumKind, SignableBody, SignableRequest, SigningSettings},
    sign::v4,
};
use aws_smithy_async::rt::sleep::TokioSleep;
use aws_smithy_runtime_api::client::{
    http::{
        HttpClient, HttpConnector, HttpConnectorFuture, HttpConnectorSettings, SharedHttpConnector,
    },
    identity::Identity,
    orchestrator::{HttpRequest, HttpResponse},
    result::SdkError,
    runtime_components::RuntimeComponents,
};
use aws_smithy_types::body::SdkBody;
use aws_types::sdk_config::SharedHttpClient;
use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue};
use http_body::{Body, Frame, SizeHint};
use pin_project::pin_project;
use regex::RegexSet;
pub use region::RegionOrEndpoint;
use snafu::Snafu;
pub use timeout::AwsTimeout;

use crate::{
    config::ProxyConfig,
    http::status,
    internal_events::{
        AwsBytesSent,
        http_client::{
            AboutToSendHttpRequest, GotHttpResponse, GotHttpWarning, HttpRequestTelemetry,
            HttpResponseTelemetry,
        },
    },
    tls::{MaybeTlsSettings, TlsConfig},
};

static RETRIABLE_CODES: OnceLock<RegexSet> = OnceLock::new();

/// Checks if the request can be retried after the given error was returned.
pub fn is_retriable_error<T>(error: &SdkError<T, HttpResponse>) -> bool {
    match error {
        SdkError::TimeoutError(_) | SdkError::DispatchFailure(_) => true,
        SdkError::ConstructionFailure(_) => false,
        SdkError::ResponseError(err) => check_response(err.raw()),
        SdkError::ServiceError(err) => check_response(err.raw()),
        _ => {
            warn!("AWS returned unknown error, retrying request.");
            true
        }
    }
}

fn check_response(res: &HttpResponse) -> bool {
    // This header is a direct indication that we should retry the request. Eventually it'd
    // be nice to actually schedule the retry after the given delay, but for now we just
    // check that it contains a positive value.
    let retry_header = res.headers().get("x-amz-retry-after").is_some();

    // Certain 400-level responses will contain an error code indicating that the request
    // should be retried. Since we don't retry 400-level responses by default, we'll look
    // for these specifically before falling back to more general heuristics. Because AWS
    // services use a mix of XML and JSON response bodies and the AWS SDK doesn't give us
    // a parsed representation, we resort to a simple string match.
    //
    // S3: RequestTimeout
    // SQS: RequestExpired, ThrottlingException
    // ECS: RequestExpired, ThrottlingException
    // Kinesis: RequestExpired, ThrottlingException
    // Cloudwatch: RequestExpired, ThrottlingException
    //
    // Now just look for those when it's a client_error
    let re = RETRIABLE_CODES.get_or_init(|| {
        RegexSet::new(["RequestTimeout", "RequestExpired", "ThrottlingException"])
            .expect("invalid regex")
    });

    let status = res.status();
    let response_body = String::from_utf8_lossy(res.body().bytes().unwrap_or(&[]));

    retry_header
        || status.is_server_error()
        || status.as_u16() == status::TOO_MANY_REQUESTS
        || (status.is_client_error() && re.is_match(response_body.as_ref()))
}

/// Creates the HTTP client that sends AWS requests with Vector's OpenSSL TLS settings and proxy
/// routing. All AWS requests should use it, as the AWS crates by default use `RustTLS`, which we
/// have turned off to consistently use OpenSSL.
fn connector(
    proxy: &ProxyConfig,
    tls_options: Option<&TlsConfig>,
) -> crate::Result<SharedHttpClient> {
    let tls_settings = MaybeTlsSettings::tls_client(tls_options)?;
    Ok(SharedHttpClient::new(http_client::SdkHttpClient::new(
        tls_settings,
        proxy,
    )?))
}

/// Creates the HTTP client that AWS service clients use: [`connector`] behind the connector that
/// emits the request telemetry and counts the bytes sent.
pub(crate) fn service_http_client(
    proxy: &ProxyConfig,
    tls_options: Option<&TlsConfig>,
    region: Region,
) -> crate::Result<SharedHttpClient> {
    Ok(SharedHttpClient::new(AwsHttpClient {
        http: connector(proxy, tls_options)?,
        region,
        emit_bytes_sent: true,
    }))
}

/// Implement for each AWS service to create the appropriate AWS sdk client.
pub trait ClientBuilder {
    /// The type of the client in the SDK.
    type Client;

    /// Build the client using the given config settings.
    fn build(&self, config: &SdkConfig) -> Self::Client;
}

/// Provides the configured AWS region.
///
/// # Errors
///
/// Returns an error if the TLS settings are invalid or the proxy or TLS connector cannot be built.
pub fn region_provider(
    proxy: &ProxyConfig,
    tls_options: Option<&TlsConfig>,
) -> crate::Result<impl ProvideRegion + use<>> {
    // Region is not yet known here, so we cannot wrap with AwsHttpClient for observability.
    let config = aws_config::provider_config::ProviderConfig::default()
        .with_http_client(connector(proxy, tls_options)?);

    Ok(aws_config::meta::region::RegionProviderChain::first_try(
        aws_config::environment::EnvironmentVariableRegionProvider::new(),
    )
    .or_else(aws_config::profile::ProfileFileRegionProvider::builder().build())
    .or_else(
        aws_config::imds::region::ImdsRegionProvider::builder()
            .configure(&config)
            .build(),
    ))
}

async fn resolve_region(
    proxy: &ProxyConfig,
    tls_options: Option<&TlsConfig>,
    region: Option<Region>,
) -> crate::Result<Region> {
    match region {
        Some(region) => Ok(region),
        None => region_provider(proxy, tls_options)?
            .region()
            .await
            .ok_or_else(|| {
                "Could not determine region from Vector configuration or default providers".into()
            }),
    }
}

/// Create the SDK client using the provided settings.
///
/// # Errors
///
/// Returns an error if the client cannot be created. See [`create_client_and_region`] for the
/// specific errors.
pub async fn create_client<T>(
    builder: &T,
    auth: &AwsAuthentication,
    region: Option<Region>,
    endpoint: Option<String>,
    proxy: &ProxyConfig,
    tls_options: Option<&TlsConfig>,
    timeout: Option<&AwsTimeout>,
) -> crate::Result<T::Client>
where
    T: ClientBuilder,
{
    create_client_and_region::<T>(builder, auth, region, endpoint, proxy, tls_options, timeout)
        .await
        .map(|(client, _)| client)
}

/// Create the SDK client and resolve the region using the provided settings.
///
/// # Errors
///
/// Returns an error if no region is configured and none can be resolved from the environment,
/// profile files, or IMDS; if the TLS settings are invalid or the proxy or TLS connector cannot
/// be built; or if the credentials provider for `auth` cannot be created.
pub async fn create_client_and_region<T>(
    builder: &T,
    auth: &AwsAuthentication,
    region: Option<Region>,
    endpoint: Option<String>,
    proxy: &ProxyConfig,
    tls_options: Option<&TlsConfig>,
    timeout: Option<&AwsTimeout>,
) -> crate::Result<(T::Client, Region)>
where
    T: ClientBuilder,
{
    let retry_config = RetryConfig::disabled();

    // The default credentials chains will look for a region if not given but we'd like to
    // error up front if later SDK calls will fail due to lack of region configuration
    let region = resolve_region(proxy, tls_options, region).await?;

    let provider_config =
        aws_config::provider_config::ProviderConfig::empty().with_region(Some(region.clone()));

    let connector = service_http_client(proxy, tls_options, region.clone())?;

    // Build the configuration first.
    let mut config_builder = SdkConfig::builder()
        .http_client(connector)
        .sleep_impl(Arc::new(TokioSleep::new()))
        .identity_cache(auth.credentials_cache()?)
        .credentials_provider(
            auth.credentials_provider(region.clone(), proxy, tls_options)
                .await?,
        )
        .region(region.clone())
        .retry_config(retry_config.clone());

    if let Some(endpoint_override) = endpoint {
        config_builder = config_builder.endpoint_url(endpoint_override);
    } else if let Some(endpoint_from_config) =
        aws_config::default_provider::endpoint_url::endpoint_url_provider(&provider_config).await
    {
        config_builder = config_builder.endpoint_url(endpoint_from_config);
    }

    if let Some(use_fips) =
        aws_config::default_provider::use_fips::use_fips_provider(&provider_config).await
    {
        config_builder = config_builder.use_fips(use_fips);
    }

    if let Some(timeout) = timeout {
        let mut timeout_config_builder = TimeoutConfig::builder();

        let operation_timeout = timeout.operation_timeout();
        let connect_timeout = timeout.connect_timeout();
        let read_timeout = timeout.read_timeout();

        timeout_config_builder
            .set_operation_timeout(operation_timeout.map(Duration::from_secs))
            .set_connect_timeout(connect_timeout.map(Duration::from_secs))
            .set_read_timeout(read_timeout.map(Duration::from_secs));

        config_builder = config_builder.timeout_config(timeout_config_builder.build());
    }

    let config = config_builder.build();

    Ok((T::build(builder, &config), region))
}

#[derive(Snafu, Debug)]
enum SigningError {
    #[snafu(display("cannot sign the request because the headers are not valid utf-8"))]
    NotUTF8Header,
}

/// Sign the request prior to sending to AWS, using `http` 1.x request types.
/// The signature is added to the provided `request`.
///
/// # Errors
///
/// Returns an error if a header value is not valid UTF-8, the credentials cannot be
/// provided, or the request cannot be signed.
///
/// # Panics
///
/// Panics if the access key ID, region, or service name contains characters that are not valid
/// in an HTTP header value, such as a newline.
pub async fn sign_request(
    service_name: &str,
    request: &mut http::Request<Bytes>,
    credentials_provider: &SharedCredentialsProvider,
    region: Option<&Region>,
    payload_checksum_sha256: bool,
) -> crate::Result<()> {
    sign_request_at(
        service_name,
        request,
        credentials_provider,
        region,
        payload_checksum_sha256,
        SystemTime::now,
    )
    .await
}

/// Collect a request's headers as `(&str, &str)` pairs for signing, rejecting headers whose
/// values are not valid UTF-8 with [`SigningError::NotUTF8Header`].
fn signable_headers<'a>(
    headers: impl Iterator<Item = (&'a str, &'a [u8])>,
) -> Result<Vec<(&'a str, &'a str)>, SigningError> {
    headers
        .map(|(k, v)| {
            Ok((
                k,
                std::str::from_utf8(v).map_err(|_| SigningError::NotUTF8Header)?,
            ))
        })
        .collect()
}

/// Signs `request` like [`sign_request`], taking the signing time from `signing_time`.
///
/// `signing_time` is called after the credentials are provided, so a slow credential lookup
/// does not make the signature's `x-amz-date` stale.
async fn sign_request_at(
    service_name: &str,
    request: &mut http::Request<Bytes>,
    credentials_provider: &SharedCredentialsProvider,
    region: Option<&Region>,
    payload_checksum_sha256: bool,
    signing_time: impl FnOnce() -> SystemTime,
) -> crate::Result<()> {
    let headers = signable_headers(
        request
            .headers()
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_bytes())),
    )?;
    let signable_request = SignableRequest::new(
        request.method().as_str(),
        request.uri().to_string(),
        headers.into_iter(),
        SignableBody::Bytes(request.body().as_ref()),
    )?;

    let credentials = credentials_provider.provide_credentials().await?;
    let identity = Identity::new(credentials, None);

    let mut signing_settings = SigningSettings::default();

    // Include the x-amz-content-sha256 header when calculating the AWS v4 signature;
    // this is required by some AWS services, e.g. S3 and OpenSearch Serverless
    if payload_checksum_sha256 {
        signing_settings.payload_checksum_kind = PayloadChecksumKind::XAmzSha256;
    }

    let signing_params_builder = v4::SigningParams::builder()
        .identity(&identity)
        .region(region.as_ref().map_or("", std::convert::AsRef::as_ref))
        .name(service_name)
        .time(signing_time())
        .settings(signing_settings);

    let signing_params = signing_params_builder
        .build()
        .expect("all signing params set");

    let (signing_instructions, _signature) =
        aws_sigv4::http_request::sign(signable_request, &signing_params.into())?.into_parts();
    signing_instructions.apply_to_request_http1x(request);

    Ok(())
}

#[derive(Debug)]
struct AwsHttpClient<T> {
    http: T,
    region: Region,
    /// When `false`, the connector skips `AwsBytesSent` so that control-plane
    /// traffic (STS `AssumeRole`, IMDS, SSO token exchange) does not inflate
    /// `component_sent_bytes_total`.
    emit_bytes_sent: bool,
}

impl<T> HttpClient for AwsHttpClient<T>
where
    T: HttpClient,
{
    fn http_connector(
        &self,
        settings: &HttpConnectorSettings,
        components: &RuntimeComponents,
    ) -> SharedHttpConnector {
        let http_connector = self.http.http_connector(settings, components);

        SharedHttpConnector::new(AwsConnector {
            region: self.region.clone(),
            http: http_connector,
            emit_bytes_sent: self.emit_bytes_sent,
        })
    }
}

#[derive(Clone, Debug)]
struct AwsConnector<T> {
    http: T,
    region: Region,
    emit_bytes_sent: bool,
}

// ── Telemetry trait implementations for the AWS SDK HTTP types ────────────────
//
// `HttpRequest` and `HttpResponse` are the SDK's own structs (not `http::Request`
// / `http::Response`), so they cannot implement the hyper-specific body/version
// accessors.  The required method impls (method/uri and status) are enough for
// metric labels; the optional rich-logging fields fall back to `None`.

impl HttpRequestTelemetry for HttpRequest {
    fn method(&self) -> &str {
        self.method()
    }

    fn uri(&self) -> String {
        self.uri().to_string()
    }

    fn headers(&self) -> HeaderMap<HeaderValue> {
        smithy_headers_to_map(self.headers())
    }

    fn body_size_hint(&self) -> (u64, Option<u64>) {
        let hint = Body::size_hint(self.body());
        (hint.lower(), hint.upper())
    }

    fn extra_sensitive_headers(&self) -> &'static [HeaderName] {
        &AWS_EXTRA_SENSITIVE_HEADERS
    }
}

impl HttpResponseTelemetry for HttpResponse {
    fn status_u16(&self) -> u16 {
        self.status().as_u16()
    }

    fn headers(&self) -> HeaderMap<HeaderValue> {
        smithy_headers_to_map(self.headers())
    }

    fn body_size_hint(&self) -> (u64, Option<u64>) {
        let hint = Body::size_hint(self.body());
        (hint.lower(), hint.upper())
    }

    fn extra_sensitive_headers(&self) -> &'static [HeaderName] {
        &AWS_EXTRA_SENSITIVE_HEADERS
    }
}

/// AWS-specific headers that carry credential material and must never appear in
/// logs.
///
/// - `x-amz-security-token`    — STS temporary session token
/// - `x-amz-sso_bearer_token`  — IAM Identity Center access token (exchangeable for role credentials)
/// - `x-aws-ec2-metadata-token` — `IMDSv2` session token (valid for up to 6 h)
static AWS_EXTRA_SENSITIVE_HEADERS: [HeaderName; 3] = [
    HeaderName::from_static("x-amz-security-token"),
    HeaderName::from_static("x-amz-sso_bearer_token"),
    HeaderName::from_static("x-aws-ec2-metadata-token"),
];

/// Converts the AWS SDK's string-pair header iterator into an `http::HeaderMap`.
/// Sanitization (marking sensitive headers) is handled by the trait's default
/// `sanitized_headers` method.
fn smithy_headers_to_map(
    headers: &aws_smithy_runtime_api::http::Headers,
) -> HeaderMap<HeaderValue> {
    let mut map = HeaderMap::with_capacity(headers.len());
    for (name, value) in headers {
        let Ok(header_name) = HeaderName::from_bytes(name.as_bytes()) else {
            continue;
        };
        let Ok(header_value) = HeaderValue::from_str(value) else {
            continue;
        };
        map.insert(header_name, header_value);
    }
    map
}

// ─────────────────────────────────────────────────────────────────────────────

impl<T> AwsConnector<T>
where
    T: HttpConnector,
{
    /// Calls the inner HTTP connector and emits common telemetry.
    /// Does not emit `AwsBytesSent` telemetry.
    fn call_inner(&self, req: HttpRequest) -> HttpConnectorFuture {
        emit(AboutToSendHttpRequest { request: &req });

        let fut: HttpConnectorFuture = self.http.call(req);

        HttpConnectorFuture::new(async move {
            let before = Instant::now();
            let result = fut.await;
            let roundtrip = before.elapsed();

            match &result {
                Ok(response) => {
                    emit(GotHttpResponse {
                        response,
                        roundtrip,
                    });
                }
                Err(error) => {
                    emit(GotHttpWarning { error, roundtrip });
                }
            }

            result
        })
    }
}

impl<T> HttpConnector for AwsConnector<T>
where
    T: HttpConnector,
{
    fn call(&self, req: HttpRequest) -> HttpConnectorFuture {
        if !self.emit_bytes_sent {
            return HttpConnectorFuture::new(self.call_inner(req));
        }

        let bytes_sent = Arc::new(AtomicUsize::new(0));

        let req = req.map(|body| {
            let bytes_sent = Arc::clone(&bytes_sent);
            body.map_preserve_contents(move |body| {
                let body = MeasuredBody::new(body, Arc::clone(&bytes_sent));
                SdkBody::from_body_1_x(body)
            })
        });

        let fut = self.call_inner(req);
        let region = self.region.clone();

        HttpConnectorFuture::new(async move {
            let result = fut.await;

            if let Ok(response) = &result
                && response.status().is_success()
            {
                let byte_size = bytes_sent.load(Ordering::Relaxed);

                emit(AwsBytesSent {
                    byte_size,
                    region: Some(region),
                });
            }

            result
        })
    }
}

/// Counts the data bytes of an SDK request body and forwards every frame, the end-of-stream state
/// and the size hint unchanged, so that the transport frames the body as the SDK built it.
#[pin_project]
struct MeasuredBody {
    #[pin]
    inner: SdkBody,
    shared_bytes_sent: Arc<AtomicUsize>,
}

impl MeasuredBody {
    const fn new(body: SdkBody, shared_bytes_sent: Arc<AtomicUsize>) -> Self {
        Self {
            inner: body,
            shared_bytes_sent,
        }
    }
}

impl Body for MeasuredBody {
    type Data = Bytes;
    type Error = Box<dyn Error + Send + Sync>;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.project();

        let frame = ready!(this.inner.poll_frame(cx));
        if let Some(Ok(frame)) = &frame
            && let Some(data) = frame.data_ref()
        {
            this.shared_bytes_sent
                .fetch_add(data.len(), Ordering::Release);
        }
        Poll::Ready(frame)
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use aws_credential_types::Credentials;
    use http_body_util::{BodyExt, Full};
    use hyper::body::Incoming;
    use tokio::{io::AsyncReadExt, net::TcpListener, sync::mpsc};

    use super::*;
    use crate::test_util::http::spawn_blackhole_http_server;

    /// A request sent through the SDK connector reaches the server with its method, URI, headers,
    /// and body, and the response status, headers, and body reach the SDK.
    #[tokio::test]
    async fn connector_sends_request_and_returns_response() {
        let (observed_tx, mut observed_rx) = mpsc::unbounded_channel();
        let uri = spawn_blackhole_http_server(move |request: http::Request<Incoming>| {
            let observed_tx = observed_tx.clone();
            async move {
                let (parts, body) = request.into_parts();
                let body = body.collect().await.expect("request body").to_bytes();
                observed_tx
                    .send((parts, body))
                    .expect("test receives the request");
                Ok::<_, Infallible>(
                    http::Response::builder()
                        .header("x-amz-request-id", "request-1")
                        .body(Full::new(Bytes::from_static(b"response body")))
                        .expect("valid response"),
                )
            }
        })
        .await;

        let http_connector = connector(&ProxyConfig::default(), None)
            .expect("connector builds")
            .http_connector(
                &HttpConnectorSettings::default(),
                &http_client::test_runtime_components(),
            );
        let request = http::Request::post(format!("{uri}path?query=1"))
            .header("content-type", "application/json")
            .header("x-amz-target", "Test.Operation")
            .body(SdkBody::from("request body"))
            .expect("valid request");
        let request = HttpRequest::try_from(request).expect("valid SDK request");

        let response = http_connector
            .call(request)
            .await
            .expect("request succeeds");

        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(
            response.headers().get("x-amz-request-id"),
            Some("request-1")
        );
        let body = response
            .into_body()
            .collect()
            .await
            .expect("response body")
            .to_bytes();
        assert_eq!(body, Bytes::from_static(b"response body"));

        let (parts, body) = observed_rx.recv().await.expect("server saw the request");
        assert_eq!(parts.method, http::Method::POST);
        assert_eq!(parts.uri.path(), "/path");
        assert_eq!(parts.uri.query(), Some("query=1"));
        assert_eq!(parts.headers["content-type"], "application/json");
        assert_eq!(parts.headers["x-amz-target"], "Test.Operation");
        assert!(
            !parts.headers.contains_key("user-agent")
                && !parts.headers.contains_key("accept-encoding"),
            "the transport must not add Vector's default headers: {:?}",
            parts.headers
        );
        assert_eq!(body, Bytes::from_static(b"request body"));
    }

    /// The byte-counting connector used by AWS service clients sends a body of known length with
    /// `Content-Length`, like the connector without byte telemetry, instead of chunked encoding.
    #[tokio::test]
    async fn byte_counting_connector_keeps_content_length() {
        let (observed_tx, mut observed_rx) = mpsc::unbounded_channel();
        let uri = spawn_blackhole_http_server(move |request: http::Request<Incoming>| {
            let observed_tx = observed_tx.clone();
            async move {
                let (parts, body) = request.into_parts();
                let body = body.collect().await.expect("request body").to_bytes();
                observed_tx
                    .send((parts, body))
                    .expect("test receives the request");
                Ok::<_, Infallible>(http::Response::new(Full::new(Bytes::new())))
            }
        })
        .await;
        let client = AwsHttpClient {
            http: connector(&ProxyConfig::default(), None).expect("connector builds"),
            region: Region::new("us-east-1"),
            emit_bytes_sent: true,
        };
        let http_connector = client.http_connector(
            &HttpConnectorSettings::default(),
            &http_client::test_runtime_components(),
        );
        let request = http::Request::post(format!("{uri}path"))
            .body(SdkBody::from("request body"))
            .expect("valid request");
        let request = HttpRequest::try_from(request).expect("valid SDK request");

        http_connector
            .call(request)
            .await
            .expect("request succeeds");

        let (parts, body) = observed_rx.recv().await.expect("server saw the request");
        assert_eq!(parts.headers["content-length"], "12");
        assert!(
            !parts.headers.contains_key("transfer-encoding"),
            "a body of known length must not be chunked: {:?}",
            parts.headers
        );
        assert_eq!(body, Bytes::from_static(b"request body"));
    }

    /// `MeasuredBody` counts only data bytes and forwards data, trailers, errors, the size hint
    /// and the end-of-stream state of the SDK body.
    #[tokio::test]
    async fn measured_body_forwards_the_body_contract() {
        let mut trailers = HeaderMap::new();
        trailers.insert("x-amz-checksum-crc32", HeaderValue::from_static("AAAAAA=="));
        let frames: Vec<Result<Frame<Bytes>, Box<dyn Error + Send + Sync>>> = vec![
            Ok(Frame::data(Bytes::from_static(b"abc"))),
            Ok(Frame::data(Bytes::from_static(b"de"))),
            Ok(Frame::trailers(trailers.clone())),
        ];
        let bytes_sent = Arc::new(AtomicUsize::new(0));
        let body = MeasuredBody::new(
            SdkBody::from_body_1_x(http_body_util::StreamBody::new(futures::stream::iter(
                frames,
            ))),
            Arc::clone(&bytes_sent),
        );
        let collected = body.collect().await.expect("body is readable");
        assert_eq!(collected.trailers(), Some(&trailers));
        assert_eq!(collected.to_bytes(), Bytes::from_static(b"abcde"));
        assert_eq!(bytes_sent.load(Ordering::Acquire), 5);

        let body = MeasuredBody::new(SdkBody::from("abc"), Arc::new(AtomicUsize::new(0)));
        assert_eq!(body.size_hint().exact(), Some(3));
        assert!(!body.is_end_stream());
        let body = MeasuredBody::new(SdkBody::empty(), Arc::new(AtomicUsize::new(0)));
        assert!(body.is_end_stream());

        let frames: Vec<Result<Frame<Bytes>, Box<dyn Error + Send + Sync>>> =
            vec![Err("stream failed".into())];
        let body = MeasuredBody::new(
            SdkBody::from_body_1_x(http_body_util::StreamBody::new(futures::stream::iter(
                frames,
            ))),
            Arc::new(AtomicUsize::new(0)),
        );
        let error = body
            .collect()
            .await
            .expect_err("the error reaches the reader");
        assert_eq!(error.to_string(), "stream failed");
    }

    /// A server that reads the request but never responds makes the SDK connector fail with a
    /// timeout once the read timeout elapses.
    #[tokio::test]
    async fn connector_times_out_waiting_for_response() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("local address");
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let mut buffer = [0; 1024];
            // Keeps the connection open, without responding, until the client closes it.
            while stream.read(&mut buffer).await.is_ok_and(|read| read > 0) {}
        });

        let settings = HttpConnectorSettings::builder()
            .read_timeout(Duration::from_millis(200))
            .build();
        let http_connector = connector(&ProxyConfig::default(), None)
            .expect("connector builds")
            .http_connector(&settings, &http_client::test_runtime_components());
        let request = http::Request::get(format!("http://{address}/"))
            .body(SdkBody::empty())
            .expect("valid request");
        let request = HttpRequest::try_from(request).expect("valid SDK request");

        let error = tokio::time::timeout(Duration::from_secs(5), http_connector.call(request))
            .await
            .expect("the read timeout elapses first")
            .expect_err("the server never responds");

        assert!(error.is_timeout(), "expected a timeout error: {error:?}");
    }

    /// A stream the server refuses over HTTP/2 was not processed, so the SDK connector reports
    /// it as an I/O error that the SDK treats as transient.
    #[tokio::test]
    async fn connector_reports_refused_http2_stream_as_io_error() {
        use crate::tls::{
            TEST_PEM_CA_PATH, TEST_PEM_CRT_PATH, TEST_PEM_KEY_PATH, TlsEnableableConfig,
        };

        let server_tls = MaybeTlsSettings::from_config(
            Some(&TlsEnableableConfig {
                enabled: Some(true),
                options: TlsConfig {
                    crt_file: Some(TEST_PEM_CRT_PATH.into()),
                    key_file: Some(TEST_PEM_KEY_PATH.into()),
                    alpn_protocols: Some(vec!["h2".to_owned()]),
                    ..Default::default()
                },
            }),
            true,
        )
        .expect("server TLS settings");
        let mut listener = server_tls
            .bind(&"127.0.0.1:0".parse().expect("address"))
            .await
            .expect("bind");
        let port = listener.local_addr().expect("local address").port();
        tokio::spawn(async move {
            let mut stream = listener.accept().await.expect("accept");
            stream.handshake().await.expect("TLS handshake");
            let mut connection = h2::server::handshake(stream).await.expect("h2 handshake");
            while let Some(Ok((_request, mut respond))) = connection.accept().await {
                respond.send_reset(h2::Reason::REFUSED_STREAM);
            }
        });

        let client_tls = TlsConfig {
            ca_file: Some(TEST_PEM_CA_PATH.into()),
            alpn_protocols: Some(vec!["h2".to_owned()]),
            ..Default::default()
        };
        let http_connector = connector(&ProxyConfig::default(), Some(&client_tls))
            .expect("connector builds")
            .http_connector(
                &HttpConnectorSettings::default(),
                &http_client::test_runtime_components(),
            );
        let request = http::Request::get(format!("https://localhost:{port}/"))
            .body(SdkBody::empty())
            .expect("valid request");
        let request = HttpRequest::try_from(request).expect("valid SDK request");

        let error = tokio::time::timeout(Duration::from_secs(5), http_connector.call(request))
            .await
            .expect("the server answers")
            .expect_err("the server refuses the stream");

        assert!(error.is_io(), "expected an I/O error: {error:?}");
    }

    /// Signing with a fixed time must produce the exact AWS v4 signature headers for the request,
    /// credentials, region, and service, and leave the URI unchanged.
    #[tokio::test]
    async fn sign_request_signs_with_fixed_time() {
        const SERVICE_NAME: &str = "example-service";
        const URI: &str =
            "https://example.us-east-1.amazonaws.com/path/to/resource?query=param&value=42";
        const BODY: &[u8] = b"some non-empty request body";
        let signing_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);

        let credentials_provider =
            SharedCredentialsProvider::new(Credentials::from_keys("AKID", "SECRET", None));
        let region = Region::new("us-east-1");

        let mut request = http::Request::post(URI)
            .header("content-type", "application/json")
            .body(Bytes::from_static(BODY))
            .expect("valid request");

        sign_request_at(
            SERVICE_NAME,
            &mut request,
            &credentials_provider,
            Some(&region),
            true,
            || signing_time,
        )
        .await
        .expect("signing should succeed");

        let header = |name: &str| {
            request
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_else(|| panic!("`{name}` header must be set by signing"))
        };

        let authorization = header("authorization");
        assert!(
            authorization.starts_with("AWS4-HMAC-SHA256 Credential=AKID/"),
            "`authorization` header must be an AWS v4 signature for the test credentials: {authorization}"
        );
        assert_eq!(
            authorization,
            "AWS4-HMAC-SHA256 \
             Credential=AKID/20231114/us-east-1/example-service/aws4_request, \
             SignedHeaders=content-type;host;x-amz-content-sha256;x-amz-date, \
             Signature=f486a00bdba23adbad2fa7ef1467edd40078469c99f77a83773fa3444fb69144"
        );
        assert_eq!(header("x-amz-date"), "20231114T221320Z");
        assert_eq!(
            header("x-amz-content-sha256"),
            "3ec836486ef3ca1ae62a9438c68c7c08659ea6913b487fdfbec70056a8457539"
        );
        assert_eq!(
            request.uri().to_string(),
            URI,
            "URI (including signed query) must be unchanged by signing"
        );
    }
}
