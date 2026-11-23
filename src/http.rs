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
    rt::TokioIo,
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
use vector_http::CpuTimedExecutor;
use vector_lib::{
    configurable::configurable_component,
    cpu_time::spawn_timed_without_span,
    internal_event::emit,
    sensitive_string::SensitiveString,
    tcp::TcpKeepaliveConfig,
    tls::{MaybeTlsIncomingStream, TlsConfig, TlsError},
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

/// The authentication strategy of the `http` sink.
///
/// The `basic`, `bearer`, `custom` and `aws` strategies are the same as in [`Auth`]. The `oauth2`
/// strategy fetches an access token asynchronously. [`Auth`] cannot do this because each other HTTP
/// component applies [`Auth`] synchronously.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "strategy")]
#[configurable(metadata(docs::enum_tag_description = "The authentication strategy to use."))]
#[cfg_attr(
    not(feature = "aws-core"),
    expect(
        clippy::large_enum_variant,
        reason = "Only the OAuth 2.0 variant is large. Boxing it would complicate the config schema."
    )
)]
pub enum HttpSinkAuth {
    /// Basic authentication.
    ///
    /// The username and password are concatenated and encoded using [base64][base64].
    ///
    /// [base64]: https://en.wikipedia.org/wiki/Base64
    Basic {
        /// The basic authentication username.
        #[configurable(metadata(docs::examples = "${USERNAME}"))]
        user: String,

        /// The basic authentication password.
        #[configurable(metadata(docs::examples = "${PASSWORD}"))]
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
        /// Custom string value of the Authorization header.
        #[configurable(metadata(docs::examples = "${AUTH_HEADER_VALUE}"))]
        value: String,
    },

    /// OAuth 2.0 client credentials.
    ///
    /// Vector requests an access token from `token_endpoint`. Vector caches the access token and
    /// refreshes it before it expires. Vector sends the access token as a bearer token. Set
    /// `client_secret` to use the standard grant. Set `tls` with a client certificate to use mutual
    /// TLS ([RFC 8705][rfc8705]).
    ///
    /// [rfc8705]: https://datatracker.ietf.org/doc/html/rfc8705
    // `rename_all` alone turns `OAuth2` into `o_auth2`.
    #[serde(rename = "oauth2")]
    OAuth2(OAuth2Config),
}

/// The settings of the OAuth 2.0 client credentials grant.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct OAuth2Config {
    /// The client identifier from the authorization server.
    #[configurable(metadata(docs::examples = "${CLIENT_ID}"))]
    pub client_id: String,

    /// The client secret.
    ///
    /// When this field is set, Vector authenticates to the token endpoint with HTTP Basic
    /// authentication (RFC 6749, section 2.3.1). When this field is unset, Vector authenticates to
    /// the token endpoint with a client certificate.
    #[configurable(metadata(docs::examples = "${CLIENT_SECRET}"))]
    pub client_secret: Option<SensitiveString>,

    /// The token endpoint that issues the access token.
    ///
    /// Vector sends the client credential to this endpoint. Use a URI with the `https` scheme
    /// unless the endpoint is on a trusted network.
    #[configurable(metadata(docs::examples = "https://auth.example.com/oauth/token"))]
    pub token_endpoint: String,

    /// The scope to request from the authorization server.
    ///
    /// Some authorization servers require a scope. Vector sends this value as the `scope`
    /// parameter of the token request.
    #[configurable(metadata(docs::examples = "${SCOPE}"))]
    pub scope: Option<String>,

    /// The audience to request from the authorization server.
    ///
    /// Vector sends this value as the `audience` parameter of the token request. Some
    /// authorization servers reject a token request without it.
    #[configurable(metadata(docs::examples = "${AUDIENCE}"))]
    pub audience: Option<String>,

    /// The resource to request from the authorization server.
    ///
    /// Vector sends this value as the `resource` parameter of the token request
    /// ([RFC 8707][rfc8707]).
    ///
    /// [rfc8707]: https://datatracker.ietf.org/doc/html/rfc8707
    #[configurable(metadata(docs::examples = "${RESOURCE}"))]
    pub resource: Option<String>,

    /// The number of seconds to refresh the access token before it expires.
    ///
    /// When this value is equal to or larger than the access token lifetime, Vector requests a new
    /// access token for each request.
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[serde(default = "default_expiry_buffer_secs")]
    pub expiry_buffer_secs: u32,

    /// The time limit for a complete token request.
    ///
    /// A stalled authorization server cannot hold the refresh lock and block the requests of the
    /// sink for longer than this value. A value of `0` is rejected.
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[serde(default = "default_token_timeout_secs")]
    pub timeout_secs: u64,

    /// The TLS settings for the connection to the token endpoint.
    ///
    /// Set this field when the token endpoint needs different TLS settings than the sink `tls`
    /// field. A client certificate is an example. When this field is unset, the sink `tls` field
    /// applies.
    #[serde(default)]
    pub tls: Option<TlsConfig>,
}

const fn default_expiry_buffer_secs() -> u32 {
    60
}

const fn default_token_timeout_secs() -> u64 {
    10
}

impl HttpSinkAuth {
    /// Returns the credentials that a request builder applies synchronously.
    ///
    /// Returns `None` for OAuth 2.0. [`HttpClient`] applies the OAuth 2.0 access token
    /// asynchronously.
    #[must_use]
    pub fn to_auth(&self) -> Option<Auth> {
        match self {
            Self::Basic { user, password } => Some(Auth::Basic {
                user: user.clone(),
                password: password.clone(),
            }),
            Self::Bearer { token } => Some(Auth::Bearer {
                token: token.clone(),
            }),
            #[cfg(feature = "aws-core")]
            Self::Aws { auth, service } => Some(Auth::Aws {
                auth: auth.clone(),
                service: service.clone(),
            }),
            Self::Custom { value } => Some(Auth::Custom {
                value: value.clone(),
            }),
            Self::OAuth2(_) => None,
        }
    }
}

impl OAuth2Config {
    /// Checks these settings.
    ///
    /// # Errors
    ///
    /// Returns an error when `token_endpoint` is not an absolute `http` or `https` URI, or when
    /// `timeout_secs` is `0`.
    pub fn validate(&self) -> crate::Result<()> {
        let uri: http::Uri = self.token_endpoint.parse().map_err(|error| {
            format!(
                "`token_endpoint` is not a valid URI, got `{}`: {error}",
                self.token_endpoint
            )
        })?;

        match uri.scheme_str() {
            Some("http" | "https") if uri.authority().is_some() => {}
            _ => {
                return Err(format!(
                    "`token_endpoint` must be an absolute `http` or `https` URI, got `{}`",
                    self.token_endpoint
                )
                .into());
            }
        }

        if self.timeout_secs == 0 {
            return Err("`timeout_secs` must be greater than `0`".into());
        }

        Ok(())
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
    let mut builder = ConnectionBuilder::new(CpuTimedExecutor);
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
        spawn_timed_without_span(async move {
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

mod oauth2;
pub use oauth2::{OAuth2Extension, TokenError};

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
    fn auth2_deserializes_flat_oauth2() {
        let auth: HttpSinkAuth = serde_yaml::from_str(indoc::indoc! {r"
            strategy: oauth2
            client_id: my-client
            client_secret: my-secret
            token_endpoint: https://auth.example.com/oauth/token
        "})
        .unwrap();

        let HttpSinkAuth::OAuth2(config) = auth else {
            panic!("expected the OAuth 2.0 variant");
        };
        assert_eq!(config.client_id, "my-client");
        assert_eq!(config.expiry_buffer_secs, 60);
        assert_eq!(config.timeout_secs, 10);
        assert_eq!(
            config.token_endpoint,
            "https://auth.example.com/oauth/token"
        );
        assert!(config.validate().is_ok());
    }

    #[test]
    fn auth2_rejects_the_snake_case_oauth2_tag() {
        let error = serde_yaml::from_str::<HttpSinkAuth>(indoc::indoc! {r"
            strategy: o_auth2
            client_id: my-client
            token_endpoint: https://auth.example.com/oauth/token
        "})
        .expect_err("`o_auth2` must not be accepted");
        assert!(error.to_string().contains("oauth2"), "{error}");
    }

    #[test]
    fn auth2_rejects_unknown_fields() {
        assert!(
            serde_yaml::from_str::<HttpSinkAuth>(indoc::indoc! {r"
                strategy: oauth2
                client_id: my-client
                token_endpoint: https://auth.example.com/oauth/token
                bogus: true
            "})
            .is_err()
        );
    }

    #[test]
    fn auth2_maps_only_static_variants() {
        assert_eq!(
            HttpSinkAuth::Basic {
                user: "user".to_owned(),
                password: SensitiveString::from("pass".to_owned()),
            }
            .to_auth(),
            Some(Auth::Basic {
                user: "user".to_owned(),
                password: SensitiveString::from("pass".to_owned()),
            })
        );
        assert!(
            HttpSinkAuth::OAuth2(crate::http::OAuth2Config {
                client_id: "id".to_owned(),
                client_secret: None,
                token_endpoint: "https://auth.example.com/oauth/token".to_owned(),
                scope: None,
                audience: None,
                resource: None,
                expiry_buffer_secs: 60,
                timeout_secs: 10,
                tls: None,
            })
            .to_auth()
            .is_none()
        );
    }

    #[test]
    fn oauth2_validate_rejects_relative_token_endpoint() {
        let config = crate::http::OAuth2Config {
            client_id: "id".to_owned(),
            client_secret: None,
            token_endpoint: "auth/token".to_owned(),
            scope: None,
            audience: None,
            resource: None,
            expiry_buffer_secs: 60,
            timeout_secs: 10,
            tls: None,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn oauth2_validate_rejects_a_zero_timeout() {
        let config = crate::http::OAuth2Config {
            client_id: "id".to_owned(),
            client_secret: None,
            token_endpoint: "https://auth.example.com/oauth/token".to_owned(),
            scope: None,
            audience: None,
            resource: None,
            expiry_buffer_secs: 60,
            timeout_secs: 0,
            tls: None,
        };
        assert!(config.validate().is_err());
    }

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
