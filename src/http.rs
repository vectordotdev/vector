#![allow(missing_docs)]
use async_trait::async_trait;
use bytes::Buf;
use futures::future::BoxFuture;
use headers::{Authorization, HeaderMapExt};
use http::{
    header::HeaderValue, request::Builder, uri::InvalidUri, HeaderMap, Request, Response, Uri,
    Version,
};
use hyper::{
    body::{Body, HttpBody},
    client,
    client::{Client, HttpConnector},
};
use hyper_openssl::HttpsConnector;
use hyper_proxy::ProxyConnector;
use rand::Rng;
use serde::Deserialize;
use serde_with::serde_as;
use snafu::{ResultExt, Snafu};
use std::{
    error::Error,
    fmt,
    net::SocketAddr,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::time::Instant;
use tower::{Layer, Service};
use tower_http::{
    classify::{ServerErrorsAsFailures, SharedClassifier},
    trace::TraceLayer,
};
use tracing::{Instrument, Span};
use vector_lib::sensitive_string::SensitiveString;
use vector_lib::{
    configurable::configurable_component,
    tls::{TlsConfig, TlsSettings},
};

use crate::{
    config::ProxyConfig,
    internal_events::{http_client, HttpServerRequestReceived, HttpServerResponseSent},
    tls::{tls_connector_builder, MaybeTlsSettings, TlsError},
};

pub mod status {
    pub const FORBIDDEN: u16 = 403;
    pub const NOT_FOUND: u16 = 404;
    pub const TOO_MANY_REQUESTS: u16 = 429;
}

#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
pub enum HttpError {
    #[snafu(display("Failed to build TLS connector: {}", source))]
    BuildTlsConnector { source: TlsError },
    #[snafu(display("Failed to build HTTPS connector: {}", source))]
    MakeHttpsConnector { source: openssl::error::ErrorStack },
    #[snafu(display("Failed to build Proxy connector: {}", source))]
    MakeProxyConnector { source: InvalidUri },
    #[snafu(display("Failed to make HTTP(S) request: {}", source))]
    CallRequest { source: hyper::Error },
    #[snafu(display("Failed to build HTTP request: {}", source))]
    BuildRequest { source: http::Error },
    #[snafu(display("Failed to acquire authentication resource."))]
    AuthenticationExtension {
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

impl HttpError {
    pub const fn is_retriable(&self) -> bool {
        match self {
            HttpError::BuildRequest { .. } | HttpError::MakeProxyConnector { .. } => false,
            HttpError::CallRequest { .. }
            | HttpError::BuildTlsConnector { .. }
            | HttpError::AuthenticationExtension { .. }
            | HttpError::MakeHttpsConnector { .. } => true,
        }
    }
}

pub type HttpClientFuture = <HttpClient as Service<http::Request<Body>>>::Future;
type HttpProxyConnector = ProxyConnector<HttpsConnector<HttpConnector>>;

#[async_trait]
trait AuthExtension<B>: Send + Sync
where
    B: fmt::Debug + HttpBody + Send + 'static,
    B::Data: Send,
    B::Error: Into<crate::Error> + Send,
{
    async fn modify_request(&self, req: &mut Request<B>) -> Result<(), vector_lib::Error>;
}

#[derive(Clone)]
struct OAuth2Extension {
    token_endpoint: String,
    client_id: String,
    client_secret: Option<SensitiveString>,
    grace_period: u32,
    client: Client<HttpProxyConnector, Body>,
    token: Arc<Mutex<Option<ExpirableToken>>>,
    get_time_now_fn: Arc<dyn Fn() -> Duration + Send + Sync + 'static>,
}

#[derive(Clone)]
struct BasicAuthExtension {
    user: String,
    password: SensitiveString,
}

#[derive(Debug, Deserialize)]
struct Token {
    access_token: String,
    // This property, according to RFC, is expected to be in seconds.
    expires_in: u32,
}

#[derive(Debug, Clone)]
struct ExpirableToken {
    access_token: String,
    expires_after_ms: u128,
}

impl OAuth2Extension {
    /// Creates a new `OAuth2Extension`.
    fn new(
        token_endpoint: String,
        client_id: String,
        client_secret: Option<SensitiveString>,
        grace_period: u32,
        client: Client<HttpProxyConnector, Body>,
    ) -> OAuth2Extension {
        let get_time_now_fn = || {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
        };

        OAuth2Extension::new_internal(
            token_endpoint,
            client_id,
            client_secret,
            grace_period,
            client,
            Arc::new(get_time_now_fn),
        )
    }

    /// Creates a new `OAuth2Extension` without default get_time_now_fn argument.
    /// This method should be used only in tests.
    fn new_internal(
        token_endpoint: String,
        client_id: String,
        client_secret: Option<SensitiveString>,
        grace_period: u32,
        client: Client<HttpProxyConnector, Body>,
        get_time_now_fn: Arc<dyn Fn() -> Duration + Send + Sync + 'static>,
    ) -> OAuth2Extension {
        let initial_empty_token = Arc::new(Mutex::new(None));

        OAuth2Extension {
            token_endpoint,
            client_id,
            client_secret,
            grace_period,
            client,
            token: initial_empty_token,
            get_time_now_fn,
        }
    }

    fn get_time_now(&self) -> Duration {
        (self.get_time_now_fn)()
    }

    async fn get_token(&self) -> Result<String, vector_lib::Error> {
        if let Some(token) = self.acquire_token_from_cache() {
            return Ok(token.access_token);
        }

        //no valid token in cache (or no token at all)
        let new_token = self.request_token().await?;
        let token_to_return = new_token.access_token.clone();
        self.save_into_cache(new_token);

        Ok(token_to_return)
    }

    fn acquire_token_from_cache(&self) -> Option<ExpirableToken> {
        let maybe_token = self.token.lock().expect("Poisoned token lock");
        match &*maybe_token {
            Some(token) => {
                if self.get_time_now().as_millis() < token.expires_after_ms {
                    //we have token, token is valid for at least 1min, we can use it.
                    return Some(token.clone());
                }

                None
            }
            _ => None,
        }
    }

    fn save_into_cache(&self, token: ExpirableToken) {
        self.token
            .lock()
            .expect("Poisoned token lock")
            .replace(token);
    }

    async fn request_token(
        &self,
    ) -> Result<ExpirableToken, Box<dyn std::error::Error + Send + Sync>> {
        let mut request_body =
            format!("grant_type=client_credentials&client_id={}", self.client_id);

        // in case of oauth2 with mTLS (https://datatracker.ietf.org/doc/html/rfc8705) we only pass client_id,
        // so secret can be considered as optional.
        if let Some(client_secret) = &self.client_secret {
            let secret_param = format!("&client_secret={}", client_secret.inner());
            request_body.push_str(&secret_param);
        }

        let builder = Request::post(self.token_endpoint.clone());
        let builder = builder.header("Content-Type", "application/x-www-form-urlencoded");
        let request = builder
            .body(Body::from(request_body))
            .expect("Error creating request");

        let before = std::time::Instant::now();
        let response_result = self.client.request(request).await;
        let roundtrip = before.elapsed();

        let response = response_result.inspect_err(|error| {
            emit!(http_client::GotHttpWarning { error, roundtrip });
        })?;

        emit!(http_client::GotHttpResponse {
            response: &response,
            roundtrip
        });

        if !response.status().is_success() {
            let body_bytes = hyper::body::aggregate(response).await?;
            let body_str = std::str::from_utf8(body_bytes.chunk())?.to_string();
            return Err(Box::new(AcquireTokenError { message: body_str }));
        }

        let body = hyper::body::aggregate(response).await?;
        let token: Token = serde_json::from_reader(body.reader())?;

        let now = self.get_time_now();
        let token_will_expire_after_ms =
            OAuth2Extension::calculate_valid_until(now, self.grace_period, &token);

        Ok(ExpirableToken {
            access_token: token.access_token,
            expires_after_ms: token_will_expire_after_ms,
        })
    }

    const fn calculate_valid_until(now: Duration, grace_period: u32, token: &Token) -> u128 {
        // 'expires_in' means, in seconds, for how long it will be valid, lets say 5min,
        // to not cause some random 4xx, because token expired in the meantime, we will make some
        // room for token refreshing, this room is a grace_period.
        let (mut grace_period_seconds, overflow) = token.expires_in.overflowing_sub(grace_period);

        // If time for grace period exceed an expire_in, it basically means: always use new token.
        if overflow {
            grace_period_seconds = 0;
        }

        // We are multiplying by 1000 because expires_in field is in seconds(oauth standard), grace_period also,
        // but later we operate on milliseconds.
        let token_is_valid_until_ms: u128 = grace_period_seconds as u128 * 1000;
        let now_millis = now.as_millis();

        now_millis + token_is_valid_until_ms
    }
}

#[derive(Debug)]
pub struct AcquireTokenError {
    message: String,
}

impl fmt::Display for AcquireTokenError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Server error from authentication server: {}",
            self.message
        )
    }
}

impl Error for AcquireTokenError {}

#[async_trait]
impl<B> AuthExtension<B> for OAuth2Extension
where
    B: fmt::Debug + HttpBody + Send + 'static,
    B::Data: Send,
    B::Error: Into<crate::Error> + Send,
{
    async fn modify_request(&self, req: &mut Request<B>) -> Result<(), vector_lib::Error> {
        let token = self.get_token().await?;
        let auth = Auth::Bearer {
            token: SensitiveString::from(token),
        };
        auth.apply(req);

        Ok(())
    }
}

#[async_trait]
impl<B> AuthExtension<B> for BasicAuthExtension
where
    B: fmt::Debug + HttpBody + Send + 'static,
    B::Data: Send,
    B::Error: Into<crate::Error> + Send,
{
    async fn modify_request(&self, req: &mut Request<B>) -> Result<(), vector_lib::Error> {
        let user = self.user.clone();
        let password = self.password.clone();

        let auth = Auth::Basic { user, password };
        auth.apply(req);

        Ok(())
    }
}

pub struct HttpClient<B = Body> {
    client: Client<HttpProxyConnector, B>,
    user_agent: HeaderValue,
    proxy_connector: HttpProxyConnector,
    auth_extension: Option<Arc<dyn AuthExtension<B>>>,
}

impl<B> HttpClient<B>
where
    B: fmt::Debug + HttpBody + Send + 'static,
    B::Data: Send,
    B::Error: Into<crate::Error> + Send,
{
    pub fn new(
        tls_settings: impl Into<MaybeTlsSettings>,
        proxy_config: &ProxyConfig,
    ) -> Result<HttpClient<B>, HttpError> {
        HttpClient::new_with_custom_client(tls_settings, proxy_config, &mut Client::builder(), None)
    }

    pub fn new_with_auth_extension(
        tls_settings: impl Into<MaybeTlsSettings>,
        proxy_config: &ProxyConfig,
        auth_config: Option<AuthorizationConfig>,
    ) -> Result<HttpClient<B>, HttpError> {
        HttpClient::new_with_custom_client(
            tls_settings,
            proxy_config,
            &mut Client::builder(),
            auth_config,
        )
    }

    pub fn new_with_custom_client(
        tls_settings: impl Into<MaybeTlsSettings>,
        proxy_config: &ProxyConfig,
        client_builder: &mut client::Builder,
        auth_config: Option<AuthorizationConfig>,
    ) -> Result<HttpClient<B>, HttpError> {
        let proxy_connector = build_proxy_connector(tls_settings.into(), proxy_config)?;
        let auth_extension = build_auth_extension(auth_config, proxy_config, client_builder);
        let client = client_builder.build(proxy_connector.clone());

        let app_name = crate::get_app_name();
        let version = crate::get_version();
        let user_agent = HeaderValue::from_str(&format!("{}/{}", app_name, version))
            .expect("Invalid header value for user-agent!");

        Ok(HttpClient {
            client,
            user_agent,
            proxy_connector,
            auth_extension,
        })
    }

    pub fn send(
        &self,
        mut request: Request<B>,
    ) -> BoxFuture<'static, Result<http::Response<Body>, HttpError>> {
        let span = tracing::info_span!("http");
        let _enter = span.enter();

        default_request_headers(&mut request, &self.user_agent);
        self.maybe_add_proxy_headers(&mut request);

        let client = self.client.clone();
        let auth_extension = self.auth_extension.clone();

        let fut = async move {
            if let Some(auth_extension) = auth_extension {
                let auth_span = tracing::info_span!("auth_extension");
                auth_extension
                    .modify_request(&mut request)
                    .instrument(auth_span.clone().or_current())
                    .await
                    .inspect_err(|error| {
                        // Emit the error into the internal events system.
                        emit!(http_client::AuthExtensionError { error });
                    })
                    .context(AuthenticationExtensionSnafu)?;
            }

            emit!(http_client::AboutToSendHttpRequest { request: &request });
            let response: client::ResponseFuture = client.request(request);

            // Capture the time right before we issue the request.
            // Request doesn't start the processing until we start polling it.
            let before = std::time::Instant::now();

            // Send request and wait for the result.
            let response_result = response.await;

            // Compute the roundtrip time it took to send the request and get
            // the response or error.
            let roundtrip = before.elapsed();

            // Handle the errors and extract the response.
            let response = response_result
                .inspect_err(|error| {
                    // Emit the error into the internal events system.
                    emit!(http_client::GotHttpWarning { error, roundtrip });
                })
                .context(CallRequestSnafu)?;

            // Emit the response into the internal events system.
            emit!(http_client::GotHttpResponse {
                response: &response,
                roundtrip
            });
            Ok(response)
        }
        .instrument(span.clone().or_current());

        Box::pin(fut)
    }

    fn maybe_add_proxy_headers(&self, request: &mut Request<B>) {
        if let Some(proxy_headers) = self.proxy_connector.http_headers(request.uri()) {
            for (k, v) in proxy_headers {
                let request_headers = request.headers_mut();
                if !request_headers.contains_key(k) {
                    request_headers.insert(k, v.into());
                }
            }
        }
    }
}

fn build_auth_extension<B>(
    authorization_config: Option<AuthorizationConfig>,
    proxy_config: &ProxyConfig,
    client_builder: &mut client::Builder,
) -> Option<Arc<dyn AuthExtension<B>>>
where
    B: fmt::Debug + HttpBody + Send + 'static,
    B::Data: Send,
    B::Error: Into<crate::Error> + Send,
{
    if let Some(authorization_config) = authorization_config {
        match authorization_config.strategy {
            HttpClientAuthorizationStrategy::Basic { user, password } => {
                let basic_auth_extension = BasicAuthExtension { user, password };
                return Some(Arc::new(basic_auth_extension));
            }
            HttpClientAuthorizationStrategy::OAuth2 {
                token_endpoint,
                client_id,
                client_secret,
                grace_period,
            } => {
                let tls_for_auth = authorization_config.tls.clone();
                let tls_for_auth: TlsSettings = TlsSettings::from_options(&tls_for_auth).unwrap();

                let auth_proxy_connector =
                    build_proxy_connector(tls_for_auth.into(), proxy_config).unwrap();
                let auth_client = client_builder.build(auth_proxy_connector.clone());

                let oauth2_extension = OAuth2Extension::new(
                    token_endpoint,
                    client_id,
                    client_secret,
                    grace_period,
                    auth_client,
                );
                return Some(Arc::new(oauth2_extension));
            }
        }
    }

    None
}

pub fn build_proxy_connector(
    tls_settings: MaybeTlsSettings,
    proxy_config: &ProxyConfig,
) -> Result<ProxyConnector<HttpsConnector<HttpConnector>>, HttpError> {
    // Create dedicated TLS connector for the proxied connection with user TLS settings.
    let tls = tls_connector_builder(&tls_settings)
        .context(BuildTlsConnectorSnafu)?
        .build();
    let https = build_tls_connector(tls_settings)?;
    let mut proxy = ProxyConnector::new(https).unwrap();
    // Make proxy connector aware of user TLS settings by setting the TLS connector:
    // https://github.com/vectordotdev/vector/issues/13683
    proxy.set_tls(Some(tls));
    proxy_config
        .configure(&mut proxy)
        .context(MakeProxyConnectorSnafu)?;
    Ok(proxy)
}

pub fn build_tls_connector(
    tls_settings: MaybeTlsSettings,
) -> Result<HttpsConnector<HttpConnector>, HttpError> {
    let mut http = HttpConnector::new();
    http.enforce_http(false);

    let tls = tls_connector_builder(&tls_settings).context(BuildTlsConnectorSnafu)?;
    let mut https = HttpsConnector::with_connector(http, tls).context(MakeHttpsConnectorSnafu)?;

    let settings = tls_settings.tls().cloned();
    https.set_callback(move |c, _uri| {
        if let Some(settings) = &settings {
            settings.apply_connect_configuration(c)
        } else {
            Ok(())
        }
    });
    Ok(https)
}

fn default_request_headers<B>(request: &mut Request<B>, user_agent: &HeaderValue) {
    if !request.headers().contains_key("User-Agent") {
        request
            .headers_mut()
            .insert("User-Agent", user_agent.clone());
    }

    if !request.headers().contains_key("Accept-Encoding") {
        // hardcoding until we support compressed responses:
        // https://github.com/vectordotdev/vector/issues/5440
        request
            .headers_mut()
            .insert("Accept-Encoding", HeaderValue::from_static("identity"));
    }
}

impl<B> Service<Request<B>> for HttpClient<B>
where
    B: fmt::Debug + HttpBody + Send + 'static,
    B::Data: Send,
    B::Error: Into<crate::Error> + Send,
{
    type Response = http::Response<Body>;
    type Error = HttpError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        self.send(request)
    }
}

impl<B> Clone for HttpClient<B> {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            user_agent: self.user_agent.clone(),
            proxy_connector: self.proxy_connector.clone(),
            auth_extension: self.auth_extension.clone(),
        }
    }
}

impl<B> fmt::Debug for HttpClient<B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpClient")
            .field("client", &self.client)
            .field("user_agent", &self.user_agent)
            .finish()
    }
}

/// Configuration for HTTP client providing an authentication mechanism.
#[configurable_component]
#[configurable(metadata(docs::advanced))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationConfig {
    /// Define how to authorize against an upstream.
    #[configurable]
    strategy: HttpClientAuthorizationStrategy,

    /// The TLS settings for the http client's connection.
    ///
    /// Optional, constrains TLS settings for this http client.
    #[configurable(derived)]
    tls: Option<TlsConfig>,
}

/// Configuration of the authentication strategy for HTTP requests.
///
/// HTTP authentication should be used with HTTPS only, as the authentication credentials are passed as an
/// HTTP header without any additional encryption beyond what is provided by the transport itself.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "strategy")]
#[configurable(metadata(docs::enum_tag_description = "The authentication strategy to use."))]
pub enum HttpClientAuthorizationStrategy {
    /// Basic authentication.
    ///
    /// The username and password are concatenated and encoded via [base64][base64].
    ///
    /// [base64]: https://en.wikipedia.org/wiki/Base64
    Basic {
        /// The basic authentication username.
        #[configurable(metadata(docs::examples = "username"))]
        user: String,

        /// The basic authentication password.
        #[configurable(metadata(docs::examples = "password"))]
        password: SensitiveString,
    },

    /// Authentication based on OAuth 2.0 protocol.
    ///
    /// This strategy allows to dynamically acquire and use token based on provided parameters.
    /// Both standard client_credentials and mTLS extension is supported, for standard client_credentials just provide both
    /// client_id and client_secret parameters:
    ///
    /// # Example
    ///
    /// ```yaml
    /// strategy:
    ///  strategy: "o_auth2"
    ///  client_id: "client.id"
    ///  client_secret: "secret-value"
    ///  token_endpoint: "https://yourendpoint.com/oauth/token"
    /// ```
    /// In case you want to use mTLS extension [rfc8705](https://datatracker.ietf.org/doc/html/rfc8705), provide desired key and certificate,
    /// together with client_id (with no client_secret parameter).
    ///
    /// # Example
    ///
    /// ```yaml
    /// strategy:
    ///  strategy: "o_auth2"
    ///  client_id: "client.id"
    ///  token_endpoint: "https://yourendpoint.com/oauth/token"
    /// tls:
    ///  crt_path: cert.pem
    ///  key_file: key.pem
    /// ```
    OAuth2 {
        /// Token endpoint location, required for token acquisition.
        #[configurable(metadata(docs::examples = "https://auth.provider/oauth/token"))]
        token_endpoint: String,

        /// The client id.
        #[configurable(metadata(docs::examples = "client_id"))]
        client_id: String,

        /// The sensitive client secret.
        #[configurable(metadata(docs::examples = "client_secret"))]
        client_secret: Option<SensitiveString>,

        /// The grace period configuration for a bearer token.
        /// To avoid random authorization failures caused by expired token exception,
        /// we will acquire new token, some time (grace period) before current token will be expired,
        /// because of that, we will always execute request with fresh enough token.
        #[serde(default = "default_oauth2_token_grace_period")]
        #[configurable(metadata(docs::examples = 300))]
        #[configurable(metadata(docs::type_unit = "seconds"))]
        #[configurable(metadata(docs::human_name = "Grace period for bearer token."))]
        grace_period: u32,
    },
}

const fn default_oauth2_token_grace_period() -> u32 {
    300 // 5 minutes
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
    /// The username and password are concatenated and encoded via [base64][base64].
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
    /// The bearer token value (OAuth2, JWT, etc.) is passed as-is.
    Bearer {
        /// The bearer authentication token.
        token: SensitiveString,
    },
}

pub trait MaybeAuth: Sized {
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
    pub fn apply<B>(&self, req: &mut Request<B>) {
        self.apply_headers_map(req.headers_mut())
    }

    pub fn apply_builder(&self, mut builder: Builder) -> Builder {
        if let Some(map) = builder.headers_mut() {
            self.apply_headers_map(map)
        }
        builder
    }

    pub fn apply_headers_map(&self, map: &mut HeaderMap) {
        match &self {
            Auth::Basic { user, password } => {
                let auth = Authorization::basic(user.as_str(), password.inner());
                map.typed_insert(auth);
            }
            Auth::Bearer { token } => match Authorization::bearer(token.inner()) {
                Ok(auth) => map.typed_insert(auth),
                Err(error) => error!(message = "Invalid bearer token.", token = %token, %error),
            },
        }
    }
}

pub fn get_http_scheme_from_uri(uri: &Uri) -> &'static str {
    // If there's no scheme, we just use "http" since it provides the most semantic relevance without inadvertently
    // implying things it can't know i.e. returning "https" when we're not actually sure HTTPS was used.
    uri.scheme_str().map_or("http", |scheme| match scheme {
        "http" => "http",
        "https" => "https",
        // `http::Uri` ensures that we always get "http" or "https" if the URI is created with a well-formed scheme, but
        // it also supports arbitrary schemes, which is where we bomb out down here, since we can't generate a static
        // string for an arbitrary input string... and anything other than "http" and "https" makes no sense for an HTTP
        // client anyways.
        s => panic!("invalid URI scheme for HTTP client: {}", s),
    })
}

/// Builds a [TraceLayer] configured for a HTTP server.
///
/// This layer emits HTTP specific telemetry for requests received, responses sent, and handler duration.
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
            emit!(HttpServerRequestReceived);
        }))
        .on_response(|response: &Response<U>, latency: Duration, _span: &Span| {
            emit!(HttpServerResponseSent { response, latency });
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
}

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
    pub fn new(max_connection_age: Duration, jitter_factor: f64, peer_addr: SocketAddr) -> Self {
        Self {
            start_reference: Instant::now(),
            max_connection_age: Self::jittered_duration(max_connection_age, jitter_factor),
            peer_addr,
        }
    }

    fn jittered_duration(duration: Duration, jitter_factor: f64) -> Duration {
        // Ensure the jitter_factor is between 0.0 and 1.0
        let jitter_factor = jitter_factor.clamp(0.0, 1.0);
        // Generate a random jitter factor between `1 - jitter_factor`` and `1 + jitter_factor`.
        let mut rng = rand::thread_rng();
        let random_jitter_factor = rng.gen_range(-jitter_factor..=jitter_factor) + 1.;
        duration.mul_f64(random_jitter_factor)
    }
}

impl<S> Layer<S> for MaxConnectionAgeLayer
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
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

impl<S, E> Service<Request<Body>> for MaxConnectionAgeService<S>
where
    S: Service<Request<Body>, Response = Response<Body>, Error = E> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = E;
    type Future = BoxFuture<'static, Result<Self::Response, E>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let start_reference = self.start_reference;
        let max_connection_age = self.max_connection_age;
        let peer_addr = self.peer_addr;
        let version = req.version();
        let future = self.service.call(req);
        Box::pin(async move {
            let mut response = future.await?;
            match version {
                Version::HTTP_09 | Version::HTTP_10 | Version::HTTP_11 => {
                    if start_reference.elapsed() >= max_connection_age {
                        debug!(
                            message = "Closing connection due to max connection age.",
                            ?max_connection_age,
                            connection_age = ?start_reference.elapsed(),
                            ?peer_addr,
                        );
                        // Tell the client to close this connection.
                        // Hyper will automatically close the connection after the response is sent.
                        response.headers_mut().insert(
                            hyper::header::CONNECTION,
                            hyper::header::HeaderValue::from_static("close"),
                        );
                    }
                }
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

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, fs::File, io::BufReader};

    use hyper::{
        server::conn::AddrStream,
        service::{make_service_fn, service_fn},
        Server,
    };
    use proptest::prelude::*;
    use rand::distributions::DistString;
    use rand_distr::Alphanumeric;
    use rustls::{Certificate, PrivateKey, RootCertStore, ServerConfig};
    use tokio::net::TcpListener;
    use tokio_rustls::TlsAcceptor;
    use tower::ServiceBuilder;

    use crate::test_util::next_addr;

    use super::*;

    #[test]
    fn test_default_request_headers_defaults() {
        let user_agent = HeaderValue::from_static("vector");
        let mut request = Request::post("http://example.com").body(()).unwrap();
        default_request_headers(&mut request, &user_agent);
        assert_eq!(
            request.headers().get("Accept-Encoding"),
            Some(&HeaderValue::from_static("identity")),
        );
        assert_eq!(request.headers().get("User-Agent"), Some(&user_agent));
    }

    #[test]
    fn test_default_request_headers_does_not_overwrite() {
        let mut request = Request::post("http://example.com")
            .header("Accept-Encoding", "gzip")
            .header("User-Agent", "foo")
            .body(())
            .unwrap();
        default_request_headers(&mut request, &HeaderValue::from_static("vector"));
        assert_eq!(
            request.headers().get("Accept-Encoding"),
            Some(&HeaderValue::from_static("gzip")),
        );
        assert_eq!(
            request.headers().get("User-Agent"),
            Some(&HeaderValue::from_static("foo"))
        );
    }

    proptest! {
        #[test]
        fn test_jittered_duration(duration_in_secs in 0u64..120, jitter_factor in 0.0..1.0) {
            let duration = Duration::from_secs(duration_in_secs);
            let jittered_duration = MaxConnectionAgeLayer::jittered_duration(duration, jitter_factor);

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
            service: tower::service_fn(|_req: Request<Body>| async {
                Ok::<Response<Body>, hyper::Error>(Response::new(Body::empty()))
            }),
            start_reference,
            max_connection_age,
            peer_addr: "1.2.3.4:1234".parse().unwrap(),
        };

        let req = Request::get("http://example.com")
            .body(Body::empty())
            .unwrap();
        let response = service.call(req).await.unwrap();
        assert_eq!(response.headers().get("Connection"), None);

        tokio::time::advance(Duration::from_millis(500)).await;
        let req = Request::get("http://example.com")
            .body(Body::empty())
            .unwrap();
        let response = service.call(req).await.unwrap();
        assert_eq!(response.headers().get("Connection"), None);

        tokio::time::advance(Duration::from_millis(500)).await;
        let req = Request::get("http://example.com")
            .body(Body::empty())
            .unwrap();
        let response = service.call(req).await.unwrap();
        assert_eq!(
            response.headers().get("Connection"),
            Some(&HeaderValue::from_static("close"))
        );
    }

    #[tokio::test]
    async fn test_max_connection_age_service_http2() {
        tokio::time::pause();

        let start_reference = Instant::now();
        let max_connection_age = Duration::from_secs(0);
        let mut service = MaxConnectionAgeService {
            service: tower::service_fn(|_req: Request<Body>| async {
                Ok::<Response<Body>, hyper::Error>(Response::new(Body::empty()))
            }),
            start_reference,
            max_connection_age,
            peer_addr: "1.2.3.4:1234".parse().unwrap(),
        };

        let mut req = Request::get("http://example.com")
            .body(Body::empty())
            .unwrap();
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
            service: tower::service_fn(|_req: Request<Body>| async {
                Ok::<Response<Body>, hyper::Error>(Response::new(Body::empty()))
            }),
            start_reference,
            max_connection_age,
            peer_addr: "1.2.3.4:1234".parse().unwrap(),
        };

        let mut req = Request::get("http://example.com")
            .body(Body::empty())
            .unwrap();
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
            service: tower::service_fn(|_req: Request<Body>| async {
                Ok::<Response<Body>, hyper::Error>(Response::new(Body::empty()))
            }),
            start_reference,
            max_connection_age,
            peer_addr: "1.2.3.4:1234".parse().unwrap(),
        };

        let req = Request::get("http://example.com")
            .body(Body::empty())
            .unwrap();
        let response = service.call(req).await.unwrap();
        assert_eq!(
            response.headers().get("Connection"),
            Some(&HeaderValue::from_static("close"))
        );
    }

    // Note that we unfortunately cannot mock the time in this test because the client calls
    // sleep internally, which advances the clock.  However, this test shouldn't be flakey given
    // the time bounds provided.
    #[tokio::test]
    async fn test_max_connection_age_service_with_hyper_server() {
        // Create a hyper server with the max connection age layer.
        let max_connection_age = Duration::from_secs(1);
        let addr = next_addr();
        let make_svc = make_service_fn(move |conn: &AddrStream| {
            let svc = ServiceBuilder::new()
                .layer(MaxConnectionAgeLayer::new(
                    max_connection_age,
                    0.,
                    conn.remote_addr(),
                ))
                .service(tower::service_fn(|_req: Request<Body>| async {
                    Ok::<Response<Body>, hyper::Error>(Response::new(Body::empty()))
                }));
            futures_util::future::ok::<_, Infallible>(svc)
        });

        tokio::spawn(async move {
            Server::bind(&addr).serve(make_svc).await.unwrap();
        });

        // Wait for the server to start.
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Create a client, which has its own connection pool.
        let client = HttpClient::new(None, &ProxyConfig::default()).unwrap();

        // Responses generated before the client's max connection age has elapsed do not
        // include a `Connection: close` header in the response.
        let req = Request::get(format!("http://{}/", addr))
            .body(Body::empty())
            .unwrap();
        let response = client.send(req).await.unwrap();
        assert_eq!(response.headers().get("Connection"), None);

        let req = Request::get(format!("http://{}/", addr))
            .body(Body::empty())
            .unwrap();
        let response = client.send(req).await.unwrap();
        assert_eq!(response.headers().get("Connection"), None);

        // The first response generated after the client's max connection age has elapsed should
        // include the `Connection: close` header.
        tokio::time::sleep(Duration::from_secs(1)).await;
        let req = Request::get(format!("http://{}/", addr))
            .body(Body::empty())
            .unwrap();
        let response = client.send(req).await.unwrap();
        assert_eq!(
            response.headers().get("Connection"),
            Some(&HeaderValue::from_static("close")),
        );

        // The next request should establish a new connection.
        // Importantly, this also confirms that each connection has its own independent
        // connection age timer.
        let req = Request::get(format!("http://{}/", addr))
            .body(Body::empty())
            .unwrap();
        let response = client.send(req).await.unwrap();
        assert_eq!(response.headers().get("Connection"), None);
    }

    #[tokio::test]
    async fn test_caching_of_tokens_in_oauth2extension_with_hyper_server() {
        let addr: SocketAddr = next_addr();
        // This hyper service expose a fake oauth2 server, each request will return a response with new
        // bearer token, where expires_in property is 5seconds.
        let make_svc = make_service_fn(move |_: &AddrStream| {
            let svc = ServiceBuilder::new()
                .service(tower::service_fn(|req: Request<Body>| async move {
                    assert_eq!(
                        req.headers().get("Content-Type"),
                        Some(&HeaderValue::from_static("application/x-www-form-urlencoded")),
                    );

                    let body_bytes = hyper::body::to_bytes(req.into_body()).await.unwrap();
                    let request_body = String::from_utf8(body_bytes.to_vec()).unwrap();

                    assert_eq!(
                        // Based on the (later) OAuth2Extension configuration.
                        "grant_type=client_credentials&client_id=some_client_secret&client_secret=some_secret",
                        request_body,
                    );

                    let token_valid_for_seconds: u32 = 5;
                    let random_token = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);
                    let token = format!(r#"
                    {{
                        "access_token": "{}",
                        "token_type": "bearer",
                        "expires_in": {},
                        "scope": "some-scope"
                    }}
                    "#, random_token, token_valid_for_seconds);
                    Ok::<Response<Body>, hyper::Error>(Response::new(Body::from(token)))
                }));
            futures_util::future::ok::<_, Infallible>(svc)
        });

        tokio::spawn(async move {
            Server::bind(&addr).serve(make_svc).await.unwrap();
        });

        // Wait for the server to start.
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Simplest possible configuration for oauth's client connector.
        let tls: vector_lib::tls::MaybeTls<(), TlsSettings> =
            MaybeTlsSettings::from_config(&None, false).unwrap();
        let proxy_connector = build_proxy_connector(tls, &ProxyConfig::default()).unwrap();
        let auth_client = Client::builder().build(proxy_connector);

        let token_endpoint = format!("http://{}", addr);
        let client_id = String::from("some_client_secret");
        let client_secret = Some(SensitiveString::from(String::from("some_secret")));
        // Fake oauth server returns token which expires in 5sec, with grace period equal 2 seconds
        // we will have each token cached for next 3seconds (after this time token will be treat as expired).
        let two_seconds_grace_period: u32 = 2;

        // That can looks tricky for the first time, but idea is simple, we mock get_now_fn,
        // which is used internally by OAuth2Extension to decidy whether token is eligible for refreshing.
        // In real life Duration since epoch in seconds, can be for example 1730460289 (November 1, 2024),
        // but to simplify understanding we wil start with 11 seconds sicne epoch, and can progress.
        // Each value (index) in vec, means invocation of get_now_fn by OAuth2Extension, so
        // first call returns Duration::from_secs(11), second, Duration::from_secs(12) and so on,
        // because of that we have full controll over time here.
        let mocked_seconds_since_epoch = [11, 12, 20, 21, 22, 23];
        let counter = Arc::new(Mutex::new(0));
        let get_now_fn = move || {
            let counter = Arc::clone(&counter);
            let mut counter = counter.lock().unwrap();
            let i = *counter;
            *counter += 1;
            Duration::from_secs(mocked_seconds_since_epoch[i])
        };

        // Setup an OAuth2Extension and mocked time function
        let get_now_fn = Arc::new(get_now_fn);
        let extension = OAuth2Extension::new_internal(
            token_endpoint,
            client_id,
            client_secret,
            two_seconds_grace_period,
            auth_client,
            get_now_fn,
        );

        // First token is acquired because cache is empty.
        let first_acquisition = extension.get_token().await.unwrap();
        // Seconds will be taken from cache because first valid until (in ms) is
        // 14000ms = (11000ms + (5000ms - 2000ms))
        // where 5000ms because of token is valid 5seconds,
        // and grace period is 2seconds.
        let second_acquisition = extension.get_token().await.unwrap();
        assert_eq!(first_acquisition, second_acquisition,);

        // This time 20000ms since epoch is after 14000ms (until token is valid)
        // so we expect new token acquired.
        let third_acquisition = extension.get_token().await.unwrap();
        let fourth_acquisition = extension.get_token().await.unwrap();
        // Ensure new token requested.
        assert_ne!(first_acquisition, third_acquisition,);
        assert_eq!(third_acquisition, fourth_acquisition,);

        // Becuase third token is valid until 24000ms all acquisitions should return from cache.
        let fifth_acquisition = extension.get_token().await.unwrap();
        assert_eq!(fourth_acquisition, fifth_acquisition,);
    }

    #[tokio::test]
    async fn test_oauth2extension_handle_errors_gently_with_hyper_server() {
        let addr: SocketAddr = next_addr();
        // Simplest possible configuration for oauth's client connector.
        let tls: vector_lib::tls::MaybeTls<(), TlsSettings> =
            MaybeTlsSettings::from_config(&None, false).unwrap();
        let proxy_connector = build_proxy_connector(tls, &ProxyConfig::default()).unwrap();
        let auth_client = Client::builder().build(proxy_connector);

        let token_endpoint = format!("http://{}", addr);
        let client_id = String::from("some_client_secret");
        let client_secret = Some(SensitiveString::from(String::from("some_secret")));
        let two_seconds_grace_period: u32 = 2;

        // Setup an OAuth2Extension.
        let extension = OAuth2Extension::new(
            token_endpoint,
            client_id,
            client_secret,
            two_seconds_grace_period,
            auth_client,
        );

        // First token is acquired because cache is empty.
        let failed_acquisition = extension.get_token().await;
        assert!(failed_acquisition.is_err());
        let err_msg = failed_acquisition.err().unwrap().to_string();
        assert!(err_msg.contains("Connection refused"));

        let make_svc = make_service_fn(move |_: &AddrStream| {
            let svc = ServiceBuilder::new().service(tower::service_fn(
                |_req: Request<Body>| async move {
                    let not_a_valid_token = r#"
                    {
                        "definetly" : "not a vald response"
                    }
                    "#;

                    Ok::<Response<Body>, hyper::Error>(Response::new(Body::from(not_a_valid_token)))
                },
            ));
            futures_util::future::ok::<_, Infallible>(svc)
        });

        tokio::spawn(async move {
            Server::bind(&addr).serve(make_svc).await.unwrap();
        });

        // Wait for the server to start.
        tokio::time::sleep(Duration::from_millis(10)).await;

        let failed_acquisition = extension.get_token().await;
        assert!(failed_acquisition.is_err());
        let err_msg = failed_acquisition.err().unwrap().to_string();
        assert!(err_msg.contains("missing field"));
    }

    #[tokio::test]
    async fn test_oauth2_strategy_with_hyper_server() {
        let oauth_addr: SocketAddr = next_addr();
        let oauth_make_svc = make_service_fn(move |_: &AddrStream| {
            let svc = ServiceBuilder::new()
                .service(tower::service_fn(|req: Request<Body>| async move {
                    assert_eq!(
                        req.headers().get("Content-Type"),
                        Some(&HeaderValue::from_static("application/x-www-form-urlencoded")),
                    );

                    let body_bytes = hyper::body::to_bytes(req.into_body()).await.unwrap();
                    let request_body = String::from_utf8(body_bytes.to_vec()).unwrap();

                    assert_eq!(
                        // Based on the (later) OAuth2Extension configuration.
                        "grant_type=client_credentials&client_id=some_client_secret&client_secret=some_secret",
                        request_body,
                    );

                    let token = r#"
                    {
                        "access_token": "some.jwt.token",
                        "token_type": "bearer",
                        "expires_in": 60,
                        "scope": "some-scope"
                    }
                    "#;

                    Ok::<Response<Body>, hyper::Error>(Response::new(Body::from(token)))
                }));
            futures_util::future::ok::<_, Infallible>(svc)
        });

        // Server a Http client will request together with acquired bearer token.
        let addr: SocketAddr = next_addr();
        let make_svc = make_service_fn(move |_conn: &AddrStream| {
            let svc =
                ServiceBuilder::new().service(tower::service_fn(|req: Request<Body>| async move {
                    assert_eq!(
                        req.headers().get("authorization"),
                        Some(&HeaderValue::from_static("Bearer some.jwt.token")),
                    );

                    Ok::<Response<Body>, hyper::Error>(Response::new(Body::empty()))
                }));
            futures_util::future::ok::<_, Infallible>(svc)
        });

        tokio::spawn(async move {
            Server::bind(&oauth_addr)
                .serve(oauth_make_svc)
                .await
                .unwrap();
        });

        tokio::spawn(async move {
            Server::bind(&addr).serve(make_svc).await.unwrap();
        });

        // Wait for the server to start.
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Http client to test
        let token_endpoint = format!("http://{}", oauth_addr);
        let client_id: String = String::from("some_client_secret");
        let client_secret = Some(SensitiveString::from(String::from("some_secret")));
        let grace_period = 5;

        let oauth2_strategy = HttpClientAuthorizationStrategy::OAuth2 {
            token_endpoint,
            client_id,
            client_secret,
            grace_period,
        };

        let auth_config = AuthorizationConfig {
            strategy: oauth2_strategy,
            tls: None,
        };

        let client =
            HttpClient::new_with_auth_extension(None, &ProxyConfig::default(), Some(auth_config))
                .unwrap();

        let req = Request::get(format!("http://{}/", addr))
            .body(Body::empty())
            .unwrap();

        let response = client.send(req).await.unwrap();
        assert!(response.status().is_success());
    }

    #[tokio::test]
    async fn test_oauth2_with_mtls_strategy_with_hyper_server() {
        let oauth_addr: SocketAddr = next_addr();
        let addr: SocketAddr = next_addr();
        let make_svc = make_service_fn(move |_conn: &AddrStream| {
            let svc =
                ServiceBuilder::new().service(tower::service_fn(|req: Request<Body>| async move {
                    assert_eq!(
                        req.headers().get("authorization"),
                        Some(&HeaderValue::from_static("Bearer some.jwt.token")),
                    );

                    Ok::<Response<Body>, hyper::Error>(Response::new(Body::empty()))
                }));
            futures_util::future::ok::<_, Infallible>(svc)
        });

        // Load a certificates.
        fn load_certs(path: &str) -> Vec<Certificate> {
            let certfile = File::open(path).unwrap();
            let mut reader = BufReader::new(certfile);
            rustls_pemfile::certs(&mut reader)
                .unwrap()
                .into_iter()
                .map(Certificate)
                .collect()
        }

        // Load a private key.
        fn load_private_key(path: &str) -> PrivateKey {
            let keyfile = File::open(path).unwrap();
            let mut reader = BufReader::new(keyfile);
            let keys = rustls_pemfile::rsa_private_keys(&mut reader).unwrap();
            PrivateKey(keys[0].clone())
        }

        // Load a server tls context to validate client.
        let certs = load_certs("tests/data/ca/certs/ca.cert.pem");
        let key = load_private_key("tests/data/ca/private/ca.key.pem");
        let client_certs = load_certs("tests/data/ca/intermediate_client/certs/ca-chain.cert.pem");
        let mut root_store = RootCertStore::empty();
        for cert in client_certs {
            root_store.add(&cert).unwrap();
        }

        tokio::spawn(async move {
            let tls_config = ServerConfig::builder()
                .with_safe_defaults()
                .with_client_cert_verifier(rustls::server::AllowAnyAuthenticatedClient::new(
                    root_store,
                ))
                .with_single_cert(certs, key)
                .unwrap();

            let tls_acceptor = TlsAcceptor::from(Arc::new(tls_config));
            let acceptor = Arc::new(tls_acceptor);
            let http = hyper::server::conn::Http::new();
            let listener: TcpListener = TcpListener::bind(&oauth_addr).await.unwrap();

            loop {
                let (conn, _) = listener.accept().await.unwrap();
                let acceptor = Arc::<tokio_rustls::TlsAcceptor>::clone(&acceptor);
                let http = http.clone();
                let fut = async move {
                    let stream = acceptor.accept(conn).await.unwrap();
                    let service = service_fn(|req: Request<Body>| async {
                        assert_eq!(
                            req.headers().get("Content-Type"),
                            Some(&HeaderValue::from_static(
                                "application/x-www-form-urlencoded"
                            )),
                        );

                        let body_bytes = hyper::body::to_bytes(req.into_body()).await.unwrap();
                        let request_body = String::from_utf8(body_bytes.to_vec()).unwrap();

                        assert_eq!(
                            // Based on the (later) OAuth2Extension configuration.
                            "grant_type=client_credentials&client_id=some_client_secret",
                            request_body,
                        );

                        let token = r#"
                        {
                            "access_token": "some.jwt.token",
                            "token_type": "bearer",
                            "expires_in": 60,
                            "scope": "some-scope"
                        }
                        "#;

                        Ok::<_, hyper::Error>(Response::new(Body::from(token)))
                    });

                    http.serve_connection(stream, service).await.unwrap();
                };
                tokio::spawn(fut);
            }
        });

        tokio::spawn(async move {
            Server::bind(&addr).serve(make_svc).await.unwrap();
        });

        // Wait for the server to start.
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Http client to test
        let token_endpoint = format!("https://{}", oauth_addr);
        let client_id: String = String::from("some_client_secret");
        let grace_period = 5;

        let oauth2_strategy = HttpClientAuthorizationStrategy::OAuth2 {
            token_endpoint,
            client_id,
            client_secret: None,
            grace_period,
        };

        let auth_config = AuthorizationConfig {
            strategy: oauth2_strategy,
            tls: Some(TlsConfig {
                verify_hostname: Some(false),
                ca_file: Some("tests/data/ca/certs/ca.cert.pem".into()),
                crt_file: Some("tests/data/ca/intermediate_client/certs/localhost.cert.pem".into()),
                key_file: Some(
                    "tests/data/ca/intermediate_client/private/localhost.key.pem".into(),
                ),
                ..Default::default()
            }),
        };

        let client =
            HttpClient::new_with_auth_extension(None, &ProxyConfig::default(), Some(auth_config))
                .unwrap();

        let req = Request::get(format!("http://{}/", addr))
            .body(Body::empty())
            .unwrap();

        let response = client.send(req).await.unwrap();
        assert!(response.status().is_success());
    }

    #[tokio::test]
    async fn test_basic_auth_strategy_with_hyper_server() {
        // Server a Http client will request together with acquired bearer token.
        let addr: SocketAddr = next_addr();
        let make_svc = make_service_fn(move |_conn: &AddrStream| {
            let svc =
                ServiceBuilder::new().service(tower::service_fn(|req: Request<Body>| async move {
                    assert_eq!(
                        req.headers().get("authorization"),
                        Some(&HeaderValue::from_static("Basic dXNlcjpwYXNzd29yZA==")),
                    );

                    Ok::<Response<Body>, hyper::Error>(Response::new(Body::empty()))
                }));
            futures_util::future::ok::<_, Infallible>(svc)
        });

        tokio::spawn(async move {
            Server::bind(&addr).serve(make_svc).await.unwrap();
        });

        // Wait for the server to start.
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Http client to test
        let user = String::from("user");
        let password = SensitiveString::from(String::from("password"));

        let basic_strategy = HttpClientAuthorizationStrategy::Basic { user, password };

        let auth_config = AuthorizationConfig {
            strategy: basic_strategy,
            tls: None,
        };

        let client =
            HttpClient::new_with_auth_extension(None, &ProxyConfig::default(), Some(auth_config))
                .unwrap();

        let req = Request::get(format!("http://{}/", addr))
            .body(Body::empty())
            .unwrap();

        let response = client.send(req).await.unwrap();
        assert!(response.status().is_success());
    }

    #[tokio::test]
    async fn test_grace_period_calculation() {
        let now = Duration::from_secs(100);
        let grace_period_seconds = 5;
        let fake_token = Token {
            access_token: String::from("some-jwt"),
            expires_in: 20,
        };

        let expires_after_ms =
            OAuth2Extension::calculate_valid_until(now, grace_period_seconds, &fake_token);

        assert_eq!(115000, expires_after_ms);
    }

    #[tokio::test]
    async fn test_grace_period_calculation_with_overflow() {
        let now = Duration::from_secs(100);
        let grace_period_seconds = 30;
        let fake_token = Token {
            access_token: String::from("some-jwt"),
            expires_in: 20,
        };

        let expires_after_ms =
            OAuth2Extension::calculate_valid_until(now, grace_period_seconds, &fake_token);

        // When overflow, we expect grace_period be 0 (so, now + grace = now)
        assert_eq!(100000, expires_after_ms);
    }
}
