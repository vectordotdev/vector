use std::{fmt, time::Duration};
use vector_lib::internal_event::emit;

use http::{
    HeaderValue, Request, Response, StatusCode,
    header::{ACCEPT_ENCODING, USER_AGENT},
};
use hyper::body::Incoming;
use tracing::Instrument;
use vector_http::{ConnectionOptions, HttpError, RequestBody, TlsReloadHandle};

use crate::{
    config::ProxyConfig,
    http::OAuth2Extension,
    internal_events::http_client::{
        AboutToSendHttpRequest, AuthExtensionError, GotHttpResponse, GotHttpWarning,
    },
    tls::MaybeTlsSettings,
};

/// The connection setup limit that [`HttpClient::new`] applies; see
/// [`ConnectionOptions::connect_timeout`].
///
/// A request timeout does not bound every connect. When an idle connection becomes available
/// while a new one is still being set up, the pool moves the unfinished connect into a
/// background task so that its socket can be reused later, and nothing else bounds that task.
/// If its TLS handshake stalls, for example because the peer's FIN or RST was lost on a lossy
/// path, the socket stays open until the process exits, and enough of them exhaust the file
/// descriptors. See <https://github.com/vectordotdev/vector/issues/27245>.
///
/// This is a little above the default sink request timeout of 60 seconds, so that with default
/// settings a connect made for a request is still ended by the request timeout first, rather
/// than both timers expiring on the same tick.
pub(crate) const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(65);

/// A [`vector_http::HttpClient`] that adds Vector's default request headers and emits the HTTP
/// client events.
#[derive(Clone)]
pub struct HttpClient {
    client: vector_http::HttpClient,
    user_agent: HeaderValue,
    /// The OAuth 2.0 credentials that Vector applies to each request. `None` selects a plain
    /// client.
    oauth2: Option<OAuth2Extension>,
}

impl fmt::Debug for HttpClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("HttpClient").finish_non_exhaustive()
    }
}

impl HttpClient {
    /// Builds a client that routes requests through `proxy` and uses `tls` for HTTPS
    /// destinations and TLS proxies. Connection setup is limited to
    /// [`DEFAULT_CONNECT_TIMEOUT`].
    ///
    /// # Errors
    ///
    /// Returns an error if a configured proxy URL is invalid or the TLS settings cannot
    /// build a connector.
    pub fn new(tls: MaybeTlsSettings, proxy: &ProxyConfig) -> crate::Result<Self> {
        Self::new_with_options(
            tls,
            proxy,
            ConnectionOptions::default().connect_timeout(Some(DEFAULT_CONNECT_TIMEOUT)),
        )
    }

    /// Like [`HttpClient::new`], with the given connection `options`.
    ///
    /// # Errors
    ///
    /// Returns an error if a configured proxy URL is invalid or the TLS settings cannot
    /// build a connector.
    pub fn new_with_options(
        tls: MaybeTlsSettings,
        proxy: &ProxyConfig,
        options: ConnectionOptions,
    ) -> crate::Result<Self> {
        Ok(Self {
            client: vector_http::HttpClient::new_with_options(tls, proxy, options)?,
            user_agent: default_user_agent(),
            oauth2: None,
        })
    }

    /// Returns this client with `oauth2` applied to each request before it is sent.
    ///
    /// Clones share the token cache. Thus one access token serves all their requests.
    #[must_use]
    pub fn with_oauth2(mut self, oauth2: OAuth2Extension) -> Self {
        self.oauth2 = Some(oauth2);
        self
    }

    /// Returns a handle that replaces the TLS settings of this client and of every clone of it,
    /// for example after certificate rotation. The handle does not keep the client alive.
    #[must_use]
    pub fn tls_reload_handle(&self) -> TlsReloadHandle {
        self.client.tls_reload_handle()
    }

    /// Sends `request`, adding the default request headers and any forward proxy credentials.
    ///
    /// # Errors
    ///
    /// Returns an error if the request cannot be routed through the configured proxy or the
    /// request fails in transport.
    pub async fn send(
        &self,
        mut request: Request<RequestBody>,
    ) -> Result<Response<Incoming>, HttpError> {
        let span = tracing::info_span!("http");
        async move {
            default_request_headers(&mut request, &self.user_agent);

            self.client.add_proxy_authorization(&mut request)?;

            let oauth2_token = if let Some(oauth2) = &self.oauth2 {
                match oauth2.apply(&mut request).await {
                    Ok(token) => Some(token),
                    Err(error) => {
                        emit(AuthExtensionError { error: &error });
                        return Err(if error.is_retriable() {
                            HttpError::new(error)
                        } else {
                            let source: Box<dyn std::error::Error + Send + Sync> = Box::new(error);
                            HttpError::from(source)
                        });
                    }
                }
            } else {
                None
            };

            emit(AboutToSendHttpRequest { request: &request });

            let before = std::time::Instant::now();
            let response = self.client.send(request).await;
            let roundtrip = before.elapsed();

            match response {
                Ok(response) => {
                    // The authorization server may revoke the access token, or expire it before
                    // the lifetime it announced. Vector then discards it, so the next request
                    // fetches a new one.
                    if response.status() == StatusCode::UNAUTHORIZED
                        && let (Some(oauth2), Some(token)) = (&self.oauth2, &oauth2_token)
                    {
                        oauth2.invalidate(token);
                    }

                    emit(GotHttpResponse {
                        response: &response,
                        roundtrip,
                    });
                    Ok(response)
                }
                Err(error) => {
                    emit(GotHttpWarning {
                        error: &error,
                        roundtrip,
                    });
                    Err(error)
                }
            }
        }
        .instrument(span)
        .await
    }
}

fn default_user_agent() -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{}/{}",
        crate::get_app_name(),
        crate::get_version()
    ))
    .expect("the application name and version must form a valid user agent")
}

/// Sets `User-Agent` and `Accept-Encoding` unless the caller already set them.
///
/// `Accept-Encoding` defaults to `identity` because callers of [`HttpClient::send`] read the
/// raw body. Sink services that decode compressed responses, such as
/// `sinks::util::http::HttpService`, advertise their supported encodings before they send.
fn default_request_headers<B>(request: &mut Request<B>, user_agent: &HeaderValue) {
    if !request.headers().contains_key(USER_AGENT) {
        request.headers_mut().insert(USER_AGENT, user_agent.clone());
    }
    if !request.headers().contains_key(ACCEPT_ENCODING) {
        request
            .headers_mut()
            .insert(ACCEPT_ENCODING, HeaderValue::from_static("identity"));
    }
}

#[cfg(test)]
mod tests {
    use http::{
        HeaderValue, Request,
        header::{ACCEPT_ENCODING, USER_AGENT},
    };

    use super::default_request_headers;

    #[test]
    fn default_request_headers_are_added() {
        let user_agent = HeaderValue::from_static("vector");
        let mut request = Request::post("http://example.com").body(()).unwrap();
        default_request_headers(&mut request, &user_agent);
        assert_eq!(request.headers()[ACCEPT_ENCODING], "identity");
        assert_eq!(request.headers()[USER_AGENT], user_agent);
    }

    #[test]
    fn default_request_headers_do_not_overwrite() {
        let mut request = Request::post("http://example.com")
            .header(ACCEPT_ENCODING, "gzip")
            .header(USER_AGENT, "foo")
            .body(())
            .unwrap();
        default_request_headers(&mut request, &HeaderValue::from_static("vector"));
        assert_eq!(request.headers()[ACCEPT_ENCODING], "gzip");
        assert_eq!(request.headers()[USER_AGENT], "foo");
    }
}
