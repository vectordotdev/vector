#![allow(missing_docs)]
use std::{
    collections::HashMap, error::Error as StdError, future::Future, net::SocketAddr, pin::pin,
    time::Duration,
};

use futures::{Stream, StreamExt, future::BoxFuture};
use headers::{Authorization, HeaderMapExt};
use http::{Request, Response, Version};
use hyper::body::Incoming;
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::{conn::auto::Builder as ConnectionBuilder, graceful::GracefulShutdown},
    service::TowerToHyperService,
};
use serde_with::serde_as;
use tokio::{net::TcpStream, time::Instant};
use tower::{Layer, Service};
use tower_http::{
    classify::{ServerErrorsAsFailures, SharedClassifier},
    trace::TraceLayer,
};
use tracing::Span;
use vector_lib::{
    configurable::configurable_component,
    internal_event::emit,
    sensitive_string::SensitiveString,
    tcp::TcpKeepaliveConfig,
    tls::{MaybeTlsIncomingStream, TlsError},
};

#[cfg(feature = "aws-core")]
use crate::aws::AwsAuthentication;
use crate::internal_events::{HttpServerRequestReceived, HttpServerResponseSent};

pub mod status {
    pub const FORBIDDEN: u16 = 403;
    pub const NOT_FOUND: u16 = 404;
    pub const TOO_MANY_REQUESTS: u16 = 429;
}

/// Configuration of the authentication strategy for HTTP requests.
///
/// HTTP authentication should be used with HTTPS only, as the authentication credentials are passed as an
/// HTTP header without any additional encryption beyond what is provided by the transport itself.
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "strategy")]
#[configurable(metadata(docs::enum_tag_description = "The authentication strategy to use."))]
pub enum Auth {
    /// Basic authentication.
    ///
    /// The username and password are concatenated and encoded using [base64][base64].
    ///
    /// [base64]: https://en.wikipedia.org/wiki/Base64
    Basic {
        /// The basic authentication username.
        #[configurable(metadata(docs::examples = "${USERNAME}"))]
        #[configurable(metadata(docs::examples = "username"))]
        user: String,

        /// The basic authentication password.
        #[configurable(metadata(docs::examples = "${PASSWORD}"))]
        #[configurable(metadata(docs::examples = "password"))]
        password: SensitiveString,
    },

    /// Bearer authentication.
    ///
    /// The bearer token value (OAuth 2.0, JWT, etc.) is passed as-is.
    Bearer {
        /// The bearer authentication token.
        token: SensitiveString,
    },

    #[cfg(feature = "aws-core")]
    /// AWS authentication.
    Aws {
        /// The AWS authentication configuration.
        auth: AwsAuthentication,

        /// The AWS service name to use for signing.
        service: String,
    },

    /// Custom Authorization Header Value, will be inserted into the headers as `Authorization: < value >`
    Custom {
        /// Custom string value of the Authorization header
        #[configurable(metadata(docs::examples = "${AUTH_HEADER_VALUE}"))]
        #[configurable(metadata(docs::examples = "CUSTOM_PREFIX ${TOKEN}"))]
        value: String,
    },
}

pub trait MaybeAuth: Sized {
    /// Returns whichever of `self` and `other` is set.
    ///
    /// # Errors
    ///
    /// Returns an error if both `self` and `other` are set.
    fn choose_one(&self, other: &Self) -> crate::Result<Self>;
}

impl MaybeAuth for Option<Auth> {
    fn choose_one(&self, other: &Self) -> crate::Result<Self> {
        if self.is_some() && other.is_some() {
            Err("Two authorization credentials was provided.".into())
        } else {
            Ok(self.clone().or_else(|| other.clone()))
        }
    }
}

impl Auth {
    pub fn apply<B>(&self, request: &mut Request<B>) {
        self.apply_headers_map(request.headers_mut());
    }

    fn apply_headers_map(&self, map: &mut http::HeaderMap) {
        match &self {
            Auth::Basic { user, password } => {
                let auth = Authorization::basic(user.as_str(), password.inner());
                map.typed_insert(auth);
            }
            Auth::Bearer { token } => match Authorization::bearer(token.inner()) {
                Ok(auth) => map.typed_insert(auth),
                Err(error) => error!(message = "Invalid bearer token.", token = %token, %error),
            },
            Auth::Custom { value } => {
                // The value contains just the value of the Authorization header
                // Expected format: "SSWS token123" or "Bearer token123", etc.
                match http::HeaderValue::from_str(value) {
                    Ok(header_val) => {
                        map.insert(http::header::AUTHORIZATION, header_val);
                    }
                    Err(error) => {
                        error!(message = "Invalid custom auth header value.", value = %value, %error);
                    }
                }
            }
            #[cfg(feature = "aws-core")]
            _ => {}
        }
    }
}

/// Returns the scheme of `uri` as a static string, defaulting to `"http"` when it has none.
///
/// # Panics
///
/// Panics if `uri` has a scheme other than `http` or `https`.
pub fn get_http_scheme_from_uri(uri: &http::Uri) -> &'static str {
    // If there's no scheme, we just use "http" since it provides the most semantic relevance without inadvertently
    // implying things it can't know i.e. returning "https" when we're not actually sure HTTPS was used.
    uri.scheme_str().map_or("http", |scheme| match scheme {
        "http" => "http",
        "https" => "https",
        // `http::Uri` ensures that we always get "http" or "https" if the URI is created with a well-formed scheme, but
        // it also supports arbitrary schemes, which is where we bomb out down here, since we can't generate a static
        // string for an arbitrary input string... and anything other than "http" and "https" makes no sense for an HTTP
        // client anyways.
        s => panic!("invalid URI scheme for HTTP client: {s}"),
    })
}

/// Builds a [`TraceLayer`] configured for a HTTP server.
///
/// This layer emits HTTP specific telemetry for requests received, responses sent, and handler duration.
#[must_use]
#[expect(
    clippy::type_complexity,
    reason = "The closure types are `impl Fn` and cannot be named in a type alias."
)]
pub fn build_http_trace_layer<T, U>(
    span: Span,
) -> TraceLayer<
    SharedClassifier<ServerErrorsAsFailures>,
    impl Fn(&Request<T>) -> Span + Clone,
    impl Fn(&Request<T>, &Span) + Clone,
    impl Fn(&Response<U>, Duration, &Span) + Clone,
    (),
    (),
    (),
> {
    TraceLayer::new_for_http()
        .make_span_with(move |request: &Request<T>| {
            // This is an error span so that the labels are always present for metrics.
            error_span!(
               parent: &span,
               "http-request",
               method = %request.method(),
               path = %request.uri().path(),
            )
        })
        .on_request(Box::new(|_request: &Request<T>, _span: &Span| {
            emit(HttpServerRequestReceived);
        }))
        .on_response(|response: &Response<U>, latency: Duration, _span: &Span| {
            emit(HttpServerResponseSent { response, latency });
        })
        .on_failure(())
        .on_body_chunk(())
        .on_eos(())
}

/// Configuration of HTTP server keepalive parameters.
#[serde_as]
#[configurable_component]
#[derive(Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct KeepaliveConfig {
    /// The maximum amount of time a connection may exist before it is closed by sending
    /// a `Connection: close` header on the HTTP response. Set this to a large value like
    /// `100000000` to "disable" this feature
    ///
    ///
    /// Only applies to HTTP/0.9, HTTP/1.0, and HTTP/1.1 requests.
    ///
    /// A random jitter configured by `max_connection_age_jitter_factor` is added
    /// to the specified duration to spread out connection storms.
    #[serde(default = "default_max_connection_age")]
    #[configurable(metadata(docs::examples = 600))]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[configurable(metadata(docs::human_name = "Maximum Connection Age"))]
    pub max_connection_age_secs: Option<u64>,

    /// The factor by which to jitter the `max_connection_age_secs` value.
    ///
    /// A value of 0.1 means that the actual duration will be between 90% and 110% of the
    /// specified maximum duration.
    #[serde(default = "default_max_connection_age_jitter_factor")]
    #[configurable(validation(range(min = 0.0, max = 1.0)))]
    pub max_connection_age_jitter_factor: f64,

    /// TCP keepalive settings for accepted connections.
    ///
    /// Configures OS-level TCP keepalive probes on accepted connections. When set, the OS
    /// will send keepalive probes after the specified idle time has elapsed, detecting and
    /// closing connections where the remote peer has disappeared without sending a FIN or
    /// RST packet (for example, due to an abrupt machine failure or network partition).
    #[serde(default)]
    pub tcp_keepalive: Option<TcpKeepaliveConfig>,
}

// https://github.com/vectordotdev/vector/issues/23659
#[allow(
    clippy::unnecessary_wraps,
    reason = "Preserve return type and caller contracts"
)]
const fn default_max_connection_age() -> Option<u64> {
    Some(300) // 5 minutes
}

const fn default_max_connection_age_jitter_factor() -> f64 {
    0.1
}

impl Default for KeepaliveConfig {
    fn default() -> Self {
        Self {
            max_connection_age_secs: default_max_connection_age(),
            max_connection_age_jitter_factor: default_max_connection_age_jitter_factor(),
            tcp_keepalive: None,
        }
    }
}

/// A layer that limits the maximum duration of a client connection. It does so by adding a
/// `Connection: close` header to the response if `max_connection_duration` time has elapsed
/// since `start_reference`.
///
/// **Notes:**
/// - This is intended to be used in a Hyper server (or similar) that will automatically close
///   the connection after a response with a `Connection: close` header is sent.
/// - This layer assumes that it is instantiated once per connection, which is true within the
///   Hyper framework.
pub struct MaxConnectionAgeLayer {
    start_reference: Instant,
    max_connection_age: Duration,
    peer_addr: SocketAddr,
}

impl MaxConnectionAgeLayer {
    #[must_use]
    pub fn new(max_connection_age: Duration, jitter_factor: f64, peer_addr: SocketAddr) -> Self {
        Self {
            start_reference: Instant::now(),
            max_connection_age: vector_http::jittered_duration(max_connection_age, jitter_factor),
            peer_addr,
        }
    }
}

impl<S> Layer<S> for MaxConnectionAgeLayer {
    type Service = MaxConnectionAgeService<S>;

    fn layer(&self, service: S) -> Self::Service {
        MaxConnectionAgeService {
            service,
            start_reference: self.start_reference,
            max_connection_age: self.max_connection_age,
            peer_addr: self.peer_addr,
        }
    }
}

/// A service that limits the maximum age of a client connection. It does so by adding a
/// `Connection: close` header to the response if `max_connection_age` time has elapsed
/// since `start_reference`.
///
/// **Notes:**
/// - This is intended to be used in a Hyper server (or similar) that will automatically close
///   the connection after a response with a `Connection: close` header is sent.
/// - This service assumes that it is instantiated once per connection, which is true within the
///   Hyper framework.
#[derive(Clone)]
pub struct MaxConnectionAgeService<S> {
    service: S,
    start_reference: Instant,
    max_connection_age: Duration,
    peer_addr: SocketAddr,
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for MaxConnectionAgeService<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, S::Error>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(clippy::match_same_arms, reason = "Preserve branch-specific context")]
    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let start_reference = self.start_reference;
        let max_connection_age = self.max_connection_age;
        let peer_addr = self.peer_addr;
        let version = req.version();
        let future = self.service.call(req);
        Box::pin(async move {
            let mut response = future.await?;
            match version {
                Version::HTTP_09 | Version::HTTP_10 | Version::HTTP_11
                    if start_reference.elapsed() >= max_connection_age =>
                {
                    debug!(
                        message = "Closing connection due to max connection age.",
                        ?max_connection_age,
                        connection_age = ?start_reference.elapsed(),
                        ?peer_addr,
                    );
                    // Tell the client to close this connection.
                    // Hyper will automatically close the connection after the response is sent.
                    response.headers_mut().insert(
                        http::header::CONNECTION,
                        http::HeaderValue::from_static("close"),
                    );
                }
                Version::HTTP_09 | Version::HTTP_10 | Version::HTTP_11 => (),
                // TODO need to send GOAWAY frame
                Version::HTTP_2 => (),
                // TODO need to send GOAWAY frame
                Version::HTTP_3 => (),
                _ => (),
            }
            Ok(response)
        })
    }
}

/// Serves HTTP/1.1 and HTTP/2 (prior knowledge or ALPN) on the connections from `incoming`
/// (usually `MaybeTlsListener::accept_stream`) until `shutdown` resolves, then drops `incoming`
/// and waits for the open connections to finish.
///
/// `make_service` builds the service for each accepted connection. Connection errors are
/// logged at debug level and do not stop the server.
///
/// HTTP/1 has no header read timeout. HTTP/2 has no limit on concurrent streams and accepts a
/// 16 MiB header list.
///
/// # Errors
///
/// Returns an error if accepting a connection fails. Connections that are already open keep
/// running.
pub async fn serve<I, F, S, B>(
    incoming: I,
    shutdown: impl Future<Output = ()>,
    mut make_service: F,
) -> Result<(), TlsError>
where
    I: Stream<Item = Result<MaybeTlsIncomingStream<TcpStream>, TlsError>>,
    F: FnMut(&MaybeTlsIncomingStream<TcpStream>) -> S,
    S: Service<Request<Incoming>, Response = Response<B>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn StdError + Send + Sync>>,
    B: http_body::Body + Send + 'static,
    B::Data: Send,
    B::Error: Into<Box<dyn StdError + Send + Sync>>,
{
    let mut builder = ConnectionBuilder::new(TokioExecutor::new());
    builder.http1().header_read_timeout(None);
    builder
        .http2()
        .max_concurrent_streams(None)
        .max_header_list_size(vector_common::constants::HTTP2_MAX_HEADER_LIST_SIZE);
    let graceful = GracefulShutdown::new();
    let mut incoming = Box::pin(incoming);
    let mut shutdown = pin!(shutdown);

    loop {
        let stream = tokio::select! {
            biased;

            () = &mut shutdown => break,
            stream = incoming.next() => match stream {
                Some(stream) => stream?,
                None => break,
            },
        };

        let service = TowerToHyperService::new(make_service(&stream));
        let connection = graceful.watch(
            builder
                .serve_connection(TokioIo::new(stream), service)
                .into_owned(),
        );
        tokio::spawn(async move {
            if let Err(error) = connection.await {
                debug!(message = "HTTP connection error.", %error);
            }
        });
    }

    // Stop accepting before waiting for the open connections.
    drop(incoming);
    graceful.shutdown().await;
    Ok(())
}

/// The type of a query parameter's value, determines if it's treated as a plain string or a VRL expression.
#[configurable_component]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ParamType {
    /// The parameter value is a plain string.
    #[default]
    String,
    /// The parameter value is a VRL expression that is evaluated before each request.
    Vrl,
}

impl ParamType {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Represents a query parameter value, which can be a simple string or a typed object
/// indicating whether the value is a string or a VRL expression.
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
#[serde(untagged)]
pub enum ParameterValue {
    /// A simple string value. For backwards compatibility.
    String(String),
    /// A value with an explicit type.
    Typed {
        /// The raw value of the parameter.
        value: String,
        /// The parameter type, indicating how the `value` should be treated.
        #[serde(
            default,
            skip_serializing_if = "ParamType::is_default",
            rename = "type"
        )]
        r#type: ParamType,
    },
}

impl ParameterValue {
    /// Returns true if the parameter is a VRL expression.
    #[must_use]
    pub const fn is_vrl(&self) -> bool {
        match self {
            ParameterValue::String(_) => false,
            ParameterValue::Typed { r#type, .. } => matches!(r#type, ParamType::Vrl),
        }
    }

    /// Returns the raw string value of the parameter.
    #[must_use]
    pub const fn value(&self) -> &str {
        match self {
            ParameterValue::String(value) | ParameterValue::Typed { value, .. } => value.as_str(),
        }
    }

    /// Consumes the `ParameterValue` and returns the owned raw string value.
    #[must_use]
    pub fn into_value(self) -> String {
        match self {
            ParameterValue::String(s) => s,
            ParameterValue::Typed { value, .. } => value,
        }
    }
}

/// Configuration of the query parameter value for HTTP requests.
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
#[serde(untagged)]
#[configurable(metadata(docs::enum_tag_description = "Query parameter value"))]
pub enum QueryParameterValue {
    /// Query parameter with single value
    SingleParam(ParameterValue),
    /// Query parameter with multiple values
    MultiParams(Vec<ParameterValue>),
}

impl QueryParameterValue {
    /// Returns an iterator over the contained `ParameterValue`s.
    pub fn iter(&self) -> impl Iterator<Item = &ParameterValue> {
        match self {
            QueryParameterValue::SingleParam(param) => std::slice::from_ref(param).iter(),
            QueryParameterValue::MultiParams(params) => params.iter(),
        }
    }

    /// Convert to `Vec<ParameterValue>` for owned iteration.
    fn into_vec(self) -> Vec<ParameterValue> {
        match self {
            QueryParameterValue::SingleParam(param) => vec![param],
            QueryParameterValue::MultiParams(params) => params,
        }
    }
}

// Implement IntoIterator for owned QueryParameterValue
impl IntoIterator for QueryParameterValue {
    type Item = ParameterValue;
    type IntoIter = std::vec::IntoIter<ParameterValue>;

    fn into_iter(self) -> Self::IntoIter {
        self.into_vec().into_iter()
    }
}

pub type QueryParameters = HashMap<String, QueryParameterValue>;

mod client;
pub use client::HttpClient;

#[cfg(test)]
mod transport_tests;

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use proptest::prelude::*;
    use tower::ServiceBuilder;
    use vector_http::empty_body;

    use super::*;
    use crate::{config::ProxyConfig, test_util::addr::next_addr, tls::MaybeTlsSettings};

    #[test]
    fn test_auth_apply() {
        let mut basic = http::Request::get("http://example.com").body(()).unwrap();
        Auth::Basic {
            user: "user".to_owned(),
            password: SensitiveString::from("pass".to_owned()),
        }
        .apply(&mut basic);
        assert_eq!(
            basic.headers().get(http::header::AUTHORIZATION),
            Some(&http::HeaderValue::from_static("Basic dXNlcjpwYXNz"))
        );

        let mut bearer = http::Request::get("http://example.com").body(()).unwrap();
        Auth::Bearer {
            token: SensitiveString::from("token".to_owned()),
        }
        .apply(&mut bearer);
        assert_eq!(
            bearer.headers().get(http::header::AUTHORIZATION),
            Some(&http::HeaderValue::from_static("Bearer token"))
        );

        let mut custom = http::Request::get("http://example.com").body(()).unwrap();
        Auth::Custom {
            value: "SSWS token123".to_owned(),
        }
        .apply(&mut custom);
        assert_eq!(
            custom.headers().get(http::header::AUTHORIZATION),
            Some(&http::HeaderValue::from_static("SSWS token123"))
        );
    }

    proptest! {
        #[test]
        fn test_jittered_duration(duration_in_secs in 0u64..120, jitter_factor in 0.0..1.0) {
            let duration = Duration::from_secs(duration_in_secs);
            let jittered_duration = vector_http::jittered_duration(duration, jitter_factor);

            // Check properties based on the range of inputs
            if jitter_factor == 0.0 {
                // When jitter_factor is 0, jittered_duration should be equal to the original duration
                prop_assert_eq!(
                    jittered_duration,
                    duration,
                    "jittered_duration {:?} should be equal to duration {:?}",
                    jittered_duration,
                    duration,
                );
            } else if duration_in_secs > 0 {
                // Check the bounds when duration is non-zero and jitter_factor is non-zero
                let lower_bound = duration.mul_f64(1.0 - jitter_factor);
                let upper_bound = duration.mul_f64(1.0 + jitter_factor);
                prop_assert!(
                    jittered_duration >= lower_bound && jittered_duration <= upper_bound,
                    "jittered_duration {:?} should be between {:?} and {:?}",
                    jittered_duration,
                    lower_bound,
                    upper_bound,
                );
            } else {
                // When duration is zero, jittered_duration should also be zero
                prop_assert_eq!(
                    jittered_duration,
                    Duration::from_secs(0),
                    "jittered_duration {:?} should be equal to zero",
                    jittered_duration,
                );
            }
        }
    }

    #[tokio::test]
    async fn test_max_connection_age_service() {
        tokio::time::pause();

        let start_reference = Instant::now();
        let max_connection_age = Duration::from_secs(1);
        let mut service = MaxConnectionAgeService {
            service: tower::service_fn(|_req: Request<()>| async {
                Ok::<_, Infallible>(Response::new(()))
            }),
            start_reference,
            max_connection_age,
            peer_addr: "1.2.3.4:1234".parse().unwrap(),
        };

        let req = Request::get("http://example.com").body(()).unwrap();
        let response = service.call(req).await.unwrap();
        assert_eq!(response.headers().get("Connection"), None);

        tokio::time::advance(Duration::from_millis(500)).await;
        let req = Request::get("http://example.com").body(()).unwrap();
        let response = service.call(req).await.unwrap();
        assert_eq!(response.headers().get("Connection"), None);

        tokio::time::advance(Duration::from_millis(500)).await;
        let req = Request::get("http://example.com").body(()).unwrap();
        let response = service.call(req).await.unwrap();
        assert_eq!(
            response.headers().get("Connection"),
            Some(&http::HeaderValue::from_static("close"))
        );
    }

    #[tokio::test]
    async fn test_max_connection_age_service_http2() {
        tokio::time::pause();

        let start_reference = Instant::now();
        let max_connection_age = Duration::from_secs(0);
        let mut service = MaxConnectionAgeService {
            service: tower::service_fn(|_req: Request<()>| async {
                Ok::<_, Infallible>(Response::new(()))
            }),
            start_reference,
            max_connection_age,
            peer_addr: "1.2.3.4:1234".parse().unwrap(),
        };

        let mut req = Request::get("http://example.com").body(()).unwrap();
        *req.version_mut() = Version::HTTP_2;
        let response = service.call(req).await.unwrap();
        assert_eq!(response.headers().get("Connection"), None);
    }

    #[tokio::test]
    async fn test_max_connection_age_service_http3() {
        tokio::time::pause();

        let start_reference = Instant::now();
        let max_connection_age = Duration::from_secs(0);
        let mut service = MaxConnectionAgeService {
            service: tower::service_fn(|_req: Request<()>| async {
                Ok::<_, Infallible>(Response::new(()))
            }),
            start_reference,
            max_connection_age,
            peer_addr: "1.2.3.4:1234".parse().unwrap(),
        };

        let mut req = Request::get("http://example.com").body(()).unwrap();
        *req.version_mut() = Version::HTTP_3;
        let response = service.call(req).await.unwrap();
        assert_eq!(response.headers().get("Connection"), None);
    }

    #[tokio::test]
    async fn test_max_connection_age_service_zero_duration() {
        tokio::time::pause();

        let start_reference = Instant::now();
        let max_connection_age = Duration::from_millis(0);
        let mut service = MaxConnectionAgeService {
            service: tower::service_fn(|_req: Request<()>| async {
                Ok::<_, Infallible>(Response::new(()))
            }),
            start_reference,
            max_connection_age,
            peer_addr: "1.2.3.4:1234".parse().unwrap(),
        };

        let req = Request::get("http://example.com").body(()).unwrap();
        let response = service.call(req).await.unwrap();
        assert_eq!(
            response.headers().get("Connection"),
            Some(&http::HeaderValue::from_static("close"))
        );
    }

    // Note that we unfortunately cannot mock the time in this test because the client calls
    // sleep internally, which advances the clock.  However, this test shouldn't be flakey given
    // the time bounds provided.
    #[tokio::test]
    async fn test_max_connection_age_service_with_hyper_server() {
        // Create a hyper server with the max connection age layer.
        let max_connection_age = Duration::from_secs(1);
        let (_guard, addr) = next_addr();
        let listener = MaybeTlsSettings::Raw(()).bind(&addr).await.unwrap();
        tokio::spawn(serve(
            listener.accept_stream(),
            std::future::pending(),
            move |conn| {
                ServiceBuilder::new()
                    .layer(MaxConnectionAgeLayer::new(
                        max_connection_age,
                        0.,
                        conn.peer_addr(),
                    ))
                    .service(tower::service_fn(|_req: Request<Incoming>| async {
                        Ok::<_, Infallible>(Response::new(empty_body()))
                    }))
            },
        ));

        // Wait for the server to start.
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Create a client, which has its own connection pool.
        let client = HttpClient::new(MaybeTlsSettings::Raw(()), &ProxyConfig::default()).unwrap();

        // Responses generated before the client's max connection age has elapsed do not
        // include a `Connection: close` header in the response.
        let req = Request::get(format!("http://{addr}/"))
            .body(empty_body())
            .unwrap();
        let response = client.send(req).await.unwrap();
        assert_eq!(response.headers().get("Connection"), None);

        let req = Request::get(format!("http://{addr}/"))
            .body(empty_body())
            .unwrap();
        let response = client.send(req).await.unwrap();
        assert_eq!(response.headers().get("Connection"), None);

        // The first response generated after the client's max connection age has elapsed should
        // include the `Connection: close` header.
        tokio::time::sleep(Duration::from_secs(1)).await;
        let req = Request::get(format!("http://{addr}/"))
            .body(empty_body())
            .unwrap();
        let response = client.send(req).await.unwrap();
        assert_eq!(
            response.headers().get("Connection"),
            Some(&http::HeaderValue::from_static("close")),
        );

        // The next request should establish a new connection.
        // Importantly, this also confirms that each connection has its own independent
        // connection age timer.
        let req = Request::get(format!("http://{addr}/"))
            .body(empty_body())
            .unwrap();
        let response = client.send(req).await.unwrap();
        assert_eq!(response.headers().get("Connection"), None);
    }
}
