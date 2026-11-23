//! OAuth 2.0 client credentials for [`HttpClient`].
//!
//! [`OAuth2Extension`] fetches an access token from `token_endpoint`. It caches the access token
//! and applies it to each request as a bearer token.

use std::{
    fmt,
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use bytes::Bytes;
use headers::{Authorization, HeaderMapExt};
use http::{Request, StatusCode, header};
use http_body_util::BodyExt as _;
use tokio::sync::Mutex as AsyncMutex;
use vector_http::{ConnectionOptions, RequestBody};
use vector_lib::{sensitive_string::SensitiveString, tls::TlsSettings};

use super::OAuth2Config;
use crate::{
    config::ProxyConfig,
    sinks::util::{http::RetryStrategy, retries::RetryAction},
    tls::TlsConfig,
};

/// The cache lifetime that Vector uses when the authorization server sends no `expires_in`.
///
/// RFC 6749 section 4.4.3 recommends `expires_in`. It is not mandatory. This value is larger than
/// the default `expiry_buffer_secs`. Thus Vector still caches the access token of such a
/// server.
const DEFAULT_TOKEN_LIFETIME_SECS: u64 = 300;

/// The longest error body that is kept in a [`TokenError`] message.
const MAX_ERROR_BODY: usize = 512;

/// An error during OAuth 2.0 access token acquisition. Vector can retry some of these errors.
#[derive(Debug)]
pub struct TokenError {
    message: String,
    retriable: bool,
}

impl TokenError {
    fn permanent(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retriable: false,
        }
    }

    fn transient(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retriable: true,
        }
    }

    /// Returns whether a retry can succeed.
    #[must_use]
    pub const fn is_retriable(&self) -> bool {
        self.retriable
    }
}

impl fmt::Display for TokenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "OAuth 2.0 access token acquisition failed: {}",
            self.message
        )
    }
}

impl std::error::Error for TokenError {}

/// An access token and the time when Vector stops using it.
#[derive(Clone, Debug)]
struct ExpirableToken {
    access_token: SensitiveString,
    valid_until: Instant,
}

/// The access token response from the authorization server.
#[derive(Debug, serde::Deserialize)]
struct TokenResponse {
    access_token: String,
    /// The token type. RFC 6749 section 5.1 requires this field.
    #[serde(default)]
    token_type: Option<String>,
    /// The access token lifetime. Authorization servers send this value as a number or as a
    /// string.
    #[serde(default, deserialize_with = "deserialize_expires_in")]
    expires_in: Option<u64>,
}

/// Reads `expires_in` as an integer, a float or a string.
///
/// A value that Vector cannot read becomes `None`, so Vector uses the default lifetime. This
/// behavior must not fail the request: a retriable error would repeat a permanent condition.
fn deserialize_expires_in<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    /// The forms that authorization servers use for `expires_in`.
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Integer(u64),
        Float(f64),
        Text(String),
    }

    fn seconds_from_float(seconds: f64) -> Option<u64> {
        Duration::try_from_secs_f64(seconds)
            .ok()
            .map(|duration| duration.as_secs())
    }

    Ok(
        match <Option<Raw> as serde::Deserialize>::deserialize(deserializer)? {
            None => None,
            Some(Raw::Integer(seconds)) => Some(seconds),
            Some(Raw::Float(seconds)) => seconds_from_float(seconds),
            Some(Raw::Text(seconds)) => {
                let seconds = seconds.trim();
                seconds
                    .parse::<u64>()
                    .ok()
                    .or_else(|| seconds.parse::<f64>().ok().and_then(seconds_from_float))
            }
        },
    )
}

/// Applies an OAuth 2.0 access token to each request.
///
/// Clones share the token cache and the refresh lock. Thus the sink fetches one access token for
/// all its requests.
#[derive(Clone)]
pub struct OAuth2Extension {
    config: OAuth2Config,
    /// The client for the token endpoint. This client has no extension. Thus it cannot
    /// authenticate a token request with itself.
    client: Arc<vector_http::HttpClient>,
    /// The cached access token. No lock guard is held across an `await`.
    cache: Arc<Mutex<Option<ExpirableToken>>>,
    /// Vector holds this lock for a complete token fetch. Concurrent requests wait for the same
    /// fetch.
    refresh: Arc<AsyncMutex<()>>,
}

impl OAuth2Extension {
    /// Builds an extension for `config`.
    ///
    /// The token endpoint uses `config.tls` when it is set. Otherwise it uses `fallback_tls`, the
    /// sink TLS settings.
    ///
    /// # Errors
    ///
    /// Returns an error when the TLS settings cannot build a connector. Returns an error when a
    /// configured proxy URL is invalid.
    pub fn new(
        config: OAuth2Config,
        fallback_tls: Option<&TlsConfig>,
        proxy: &ProxyConfig,
    ) -> crate::Result<Self> {
        let tls = TlsSettings::from_options(config.tls.as_ref().or(fallback_tls))?;
        let options = ConnectionOptions::default()
            .connect_timeout(Some(super::client::DEFAULT_CONNECT_TIMEOUT));
        let client = vector_http::HttpClient::new_with_options(tls.into(), proxy, options)?;

        Ok(Self {
            config,
            client: Arc::new(client),
            cache: Arc::new(Mutex::new(None)),
            refresh: Arc::new(AsyncMutex::new(())),
        })
    }

    /// Adds the `Authorization` header with a valid access token to `request` and returns the
    /// access token that it applied.
    ///
    /// # Errors
    ///
    /// Returns an error when Vector cannot get an access token. Returns an error when the access
    /// token is not a valid header value.
    pub async fn apply(
        &self,
        request: &mut Request<RequestBody>,
    ) -> Result<SensitiveString, TokenError> {
        let token = self.token().await?;
        let authorization = Authorization::bearer(token.inner()).map_err(|error| {
            TokenError::permanent(format!(
                "the access token is not a valid header value: {error}"
            ))
        })?;
        request.headers_mut().typed_insert(authorization);
        Ok(token)
    }

    /// Removes `token` from the cache when it is still the cached access token.
    ///
    /// Vector calls this function when the target returns `401 Unauthorized`. The authorization
    /// server may revoke an access token, or expire it before the announced lifetime. The next
    /// request then fetches a new access token. A request that finds a different cached token does
    /// nothing, so a burst of `401` responses fetches one access token, not one per request.
    pub fn invalidate(&self, token: &SensitiveString) {
        let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        if cache
            .as_ref()
            .is_some_and(|cached| cached.access_token == *token)
        {
            *cache = None;
        }
    }

    /// Returns a valid access token. Vector uses the cache when possible.
    async fn token(&self) -> Result<SensitiveString, TokenError> {
        if let Some(token) = self.cached() {
            return Ok(token);
        }

        let _guard = self.refresh.lock().await;

        // Another request may have refreshed the token while this one waited for the lock.
        if let Some(token) = self.cached() {
            return Ok(token);
        }

        let token = self.request_token().await?;
        let access_token = token.access_token.clone();
        *self.cache.lock().unwrap_or_else(PoisonError::into_inner) = Some(token);
        Ok(access_token)
    }

    fn cached(&self) -> Option<SensitiveString> {
        let cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        cache
            .as_ref()
            .filter(|token| token.valid_until > Instant::now())
            .map(|token| token.access_token.clone())
    }

    async fn request_token(&self) -> Result<ExpirableToken, TokenError> {
        let endpoint = &self.config.token_endpoint;
        let timeout = Duration::from_secs(self.config.timeout_secs);
        let request = self.token_request()?;

        let (status, body) = tokio::time::timeout(timeout, async {
            let response = self.client.send(request).await.map_err(|error| {
                TokenError::transient(format!("the request to `{endpoint}` failed: {error}"))
            })?;
            let status = response.status();
            let body = response
                .into_body()
                .collect()
                .await
                .map_err(|error| {
                    TokenError::transient(format!(
                        "the response body from `{endpoint}` cannot be read: {error}"
                    ))
                })?
                .to_bytes();
            Ok::<_, TokenError>((status, body))
        })
        .await
        .map_err(|_| {
            TokenError::transient(format!(
                "the request to `{endpoint}` did not complete within {} seconds",
                self.config.timeout_secs
            ))
        })??;

        if !status.is_success() {
            return Err(TokenError {
                message: format!("`{endpoint}` returned {status}: {}", body_snippet(&body)),
                retriable: status_is_retriable(status),
            });
        }

        let token: TokenResponse = serde_json::from_slice(&body).map_err(|error| {
            // A successful response holds the access token. Its body must not enter the error
            // message, which is logged.
            TokenError::transient(format!(
                "the response from `{endpoint}` is not a token response: {error}"
            ))
        })?;

        // Vector can send only bearer credentials. Another type would fail each request.
        if let Some(token_type) = &token.token_type
            && !token_type.eq_ignore_ascii_case("bearer")
        {
            return Err(TokenError::permanent(format!(
                "`{endpoint}` returned the token type `{token_type}`. Vector supports only \
                 `Bearer`."
            )));
        }

        // Vector must not cache an access token that cannot become a header value.
        Authorization::bearer(&token.access_token).map_err(|error| {
            TokenError::transient(format!(
                "`{endpoint}` returned a token that is not a valid bearer credential: {error}"
            ))
        })?;

        Ok(ExpirableToken {
            access_token: SensitiveString::from(token.access_token),
            valid_until: self.expiry(token.expires_in),
        })
    }

    /// Builds the request to the token endpoint.
    ///
    /// The request carries the proxy credentials. [`vector_http::HttpClient::send`] does not add
    /// them.
    ///
    /// # Errors
    ///
    /// Returns an error when the token endpoint is not a valid URI. Returns an error when the
    /// configured forward proxy cannot route the request.
    fn token_request(&self) -> Result<Request<RequestBody>, TokenError> {
        let endpoint = &self.config.token_endpoint;
        let mut request = Request::post(endpoint)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(header::ACCEPT, "application/json")
            .body(vector_http::full_body(Bytes::from(
                self.token_request_body(),
            )))
            .map_err(|error| {
                TokenError::permanent(format!("invalid token endpoint request: {error}"))
            })?;

        // RFC 6749 section 2.3.1: a client uses one authentication method per request. With a
        // client secret, the credentials travel in this header, not in the body.
        if let Some(client_secret) = &self.config.client_secret {
            request.headers_mut().typed_insert(Authorization::basic(
                &percent_encode(&self.config.client_id),
                &percent_encode(client_secret.inner()),
            ));
        }

        self.client
            .add_proxy_authorization(&mut request)
            .map_err(|error| {
                TokenError::permanent(format!(
                    "the request to `{endpoint}` cannot be routed to the configured proxy: {error}"
                ))
            })?;

        Ok(request)
    }

    /// Returns the body of the token request, encoded as `application/x-www-form-urlencoded`.
    ///
    /// The body holds the client identifier only when `client_secret` is unset. A client secret
    /// selects HTTP Basic authentication, which carries the identifier too. A body without a client
    /// secret and without a client identifier also selects mutual TLS authentication
    /// ([RFC 8705][rfc8705]).
    ///
    /// [rfc8705]: https://datatracker.ietf.org/doc/html/rfc8705
    fn token_request_body(&self) -> String {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        serializer.append_pair("grant_type", "client_credentials");
        if self.config.client_secret.is_none() {
            serializer.append_pair("client_id", &self.config.client_id);
        }
        // ponytail: one `resource` value per request. RFC 8707 allows several. A list field can
        // replace this when an authorization server needs more than one.
        for (name, value) in [
            ("scope", &self.config.scope),
            ("audience", &self.config.audience),
            ("resource", &self.config.resource),
        ] {
            if let Some(value) = value {
                serializer.append_pair(name, value);
            }
        }
        serializer.finish()
    }

    /// Returns the time when Vector stops using an access token with this `expires_in`.
    ///
    /// When `expiry_buffer_secs` is equal to or larger than the lifetime, the access token expires
    /// immediately. Vector then fetches a new access token for each request. This behavior is safe
    /// when the clocks of the two systems disagree.
    fn expiry(&self, expires_in: Option<u64>) -> Instant {
        let lifetime = expires_in.unwrap_or(DEFAULT_TOKEN_LIFETIME_SECS);
        let ttl = lifetime.saturating_sub(u64::from(self.config.expiry_buffer_secs));
        let now = Instant::now();
        // A very large `expires_in` from the authorization server must not overflow the clock.
        now.checked_add(Duration::from_secs(ttl))
            .unwrap_or_else(|| now + Duration::from_secs(DEFAULT_TOKEN_LIFETIME_SECS))
    }
}

/// Whether a token endpoint response status should be retried.
///
/// Delegates to the canonical Vector HTTP retry policy
/// [`crate::sinks::util::http::RetryStrategy::Default`] so this behavior stays in lock-step with
/// HTTP-based sinks: 5xx (except 501 Not Implemented), 408 (Request Timeout) and 429 (Too Many
/// Requests) are transient. Other 4xx statuses are permanent.
fn status_is_retriable(status: StatusCode) -> bool {
    matches!(
        RetryStrategy::Default.retry_action::<()>(status),
        RetryAction::Retry(_) | RetryAction::RetryPartial(_)
    )
}

/// Percent-encodes `value` with the `application/x-www-form-urlencoded` rules that RFC 6749
/// section 2.3.1 applies to the `client_id` and `client_secret` of HTTP Basic authentication.
///
/// A space becomes `+`, as Appendix B of RFC 6749 defines it.
fn percent_encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

fn body_snippet(body: &[u8]) -> String {
    let truncated = &body[..body.len().min(MAX_ERROR_BODY)];
    let mut snippet = String::from_utf8_lossy(truncated).into_owned();
    if body.len() > MAX_ERROR_BODY {
        snippet.push('…');
    }
    snippet
}

#[cfg(test)]
mod tests {
    use headers::authorization::Basic;

    use super::*;

    fn extension(expiry_buffer_secs: u32) -> OAuth2Extension {
        OAuth2Extension {
            config: OAuth2Config {
                client_id: "id".to_owned(),
                client_secret: None,
                token_endpoint: "https://auth.example.com/oauth/token".to_owned(),
                scope: None,
                audience: None,
                resource: None,
                expiry_buffer_secs,
                timeout_secs: 10,
                tls: None,
            },
            client: Arc::new(
                vector_http::HttpClient::new(
                    TlsSettings::from_options(None).unwrap().into(),
                    &ProxyConfig::default(),
                )
                .unwrap(),
            ),
            cache: Arc::new(Mutex::new(None)),
            refresh: Arc::new(AsyncMutex::new(())),
        }
    }

    #[test]
    fn expiry_clamps_the_buffer_below_the_lifetime() {
        let extension = extension(30);
        assert!(extension.expiry(Some(20)) <= Instant::now());
    }

    #[test]
    fn expiry_uses_default_lifetime_when_absent() {
        let extension = extension(0);
        let before = Instant::now();
        let expiry = extension.expiry(None);
        assert!(expiry >= before + Duration::from_secs(DEFAULT_TOKEN_LIFETIME_SECS));
    }

    #[test]
    fn expiry_default_lifetime_outlives_the_default_expiry_buffer_secs() {
        let extension = extension(60);
        let before = Instant::now();
        let expiry = extension.expiry(None);
        assert!(
            expiry >= before + Duration::from_secs(DEFAULT_TOKEN_LIFETIME_SECS - 60),
            "a server that omits `expires_in` must still produce a cached token"
        );
    }

    #[test]
    fn expiry_does_not_panic_on_an_absurd_lifetime() {
        let extension = extension(0);
        let before = Instant::now();
        let expiry = extension.expiry(Some(u64::MAX));
        assert!(expiry >= before + Duration::from_secs(DEFAULT_TOKEN_LIFETIME_SECS));
    }

    #[test]
    fn expiry_subtracts_the_buffer_from_the_lifetime() {
        let extension = extension(5);
        let before = Instant::now();
        let expiry = extension.expiry(Some(20));
        assert!(expiry >= before + Duration::from_secs(15));
    }

    fn pairs_of(body: &str) -> std::collections::HashMap<String, String> {
        url::form_urlencoded::parse(body.as_bytes())
            .into_owned()
            .collect()
    }

    #[test]
    fn token_request_body_carries_the_client_id_only_without_a_client_secret() {
        let mut extension = extension(0);
        extension.config.client_id = "a b&c=d".to_owned();
        extension.config.scope = Some("api://default".to_owned());

        let pairs = pairs_of(&extension.token_request_body());
        assert_eq!(pairs["grant_type"], "client_credentials");
        assert_eq!(pairs["client_id"], "a b&c=d");
        assert_eq!(pairs["scope"], "api://default");
        assert!(!pairs.contains_key("client_secret"));
    }

    #[test]
    fn token_request_body_carries_the_optional_audience_and_resource() {
        let mut configured = extension(0);
        configured.config.audience = Some("api://default".to_owned());
        configured.config.resource = Some("https://api.example.com".to_owned());

        let pairs = pairs_of(&configured.token_request_body());
        assert_eq!(pairs["audience"], "api://default");
        assert_eq!(pairs["resource"], "https://api.example.com");

        let pairs = pairs_of(&extension(0).token_request_body());
        assert!(!pairs.contains_key("audience") && !pairs.contains_key("resource"));
    }

    #[test]
    fn token_request_authenticates_with_basic_percent_encoded_credentials() {
        let mut extension = extension(0);
        extension.config.client_id = "a b&c=d".to_owned();
        extension.config.client_secret = Some(SensitiveString::from("p&s=s".to_owned()));

        let request = extension
            .token_request()
            .expect("the token request must be valid");
        let authorization = request
            .headers()
            .typed_get::<Authorization<Basic>>()
            .expect("the client secret must travel in the Authorization header");
        assert_eq!(authorization.username(), "a+b%26c%3Dd");
        assert_eq!(authorization.password(), "p%26s%3Ds");

        // RFC 6749 section 2.3.1: one authentication method per request.
        let pairs = pairs_of(&extension.token_request_body());
        assert!(!pairs.contains_key("client_id"));
        assert!(!pairs.contains_key("client_secret"));
    }

    #[test]
    fn token_request_without_a_client_secret_has_no_basic_credentials() {
        let request = extension(0)
            .token_request()
            .expect("the token request must be valid");
        assert!(
            !request.headers().contains_key(header::AUTHORIZATION),
            "mutual TLS must not send basic credentials"
        );
    }

    #[test]
    fn token_response_allows_missing_expires_in() {
        let token: TokenResponse = serde_json::from_str(r#"{"access_token":"token"}"#).unwrap();
        assert_eq!(token.access_token, "token");
        assert_eq!(token.expires_in, None);
    }

    fn expires_in_of(body: &str) -> Option<u64> {
        serde_json::from_str::<TokenResponse>(body)
            .unwrap()
            .expires_in
    }

    #[test]
    fn token_response_reads_expires_in_as_a_number() {
        assert_eq!(
            expires_in_of(r#"{"access_token":"t","expires_in":3600}"#),
            Some(3600)
        );
    }

    #[test]
    fn token_response_reads_expires_in_as_a_float() {
        assert_eq!(
            expires_in_of(r#"{"access_token":"t","expires_in":3600.5}"#),
            Some(3600)
        );
    }

    #[test]
    fn token_response_reads_expires_in_as_a_string() {
        assert_eq!(
            expires_in_of(r#"{"access_token":"t","expires_in":" 3600 "}"#),
            Some(3600)
        );
    }

    #[test]
    fn token_response_ignores_an_unreadable_expires_in() {
        assert_eq!(
            expires_in_of(r#"{"access_token":"t","expires_in":"soon"}"#),
            None
        );
    }

    #[test]
    fn token_response_reads_the_token_type() {
        let token: TokenResponse =
            serde_json::from_str(r#"{"access_token":"t","token_type":"Bearer"}"#).unwrap();
        assert_eq!(token.token_type.as_deref(), Some("Bearer"));
    }

    #[test]
    fn body_snippet_truncates_long_bodies() {
        let body = "x".repeat(MAX_ERROR_BODY + 1);
        let head = "x".repeat(MAX_ERROR_BODY);
        assert_eq!(body_snippet(body.as_bytes()), format!("{head}…"));
    }
}
