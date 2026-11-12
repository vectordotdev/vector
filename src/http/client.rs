use std::fmt;
use vector_lib::internal_event::emit;

use http::{
    HeaderValue, Request, Response,
    header::{ACCEPT_ENCODING, USER_AGENT},
};
use hyper::body::Incoming;
use tracing::Instrument;
use vector_http::{ConnectionOptions, HttpError, RequestBody, TlsReloadHandle};

use crate::{
    config::ProxyConfig,
    internal_events::http_client::{AboutToSendHttpRequest, GotHttpResponse, GotHttpWarning},
    tls::MaybeTlsSettings,
};

/// A [`vector_http::HttpClient`] that adds Vector's default request headers and emits the HTTP
/// client events.
#[derive(Clone)]
pub struct HttpClient {
    client: vector_http::HttpClient,
    user_agent: HeaderValue,
}

impl fmt::Debug for HttpClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("HttpClient").finish_non_exhaustive()
    }
}

impl HttpClient {
    /// Builds a client that routes requests through `proxy` and uses `tls` for HTTPS
    /// destinations and TLS proxies.
    ///
    /// # Errors
    ///
    /// Returns an error if a configured proxy URL is invalid or the TLS settings cannot
    /// build a connector.
    pub fn new(tls: MaybeTlsSettings, proxy: &ProxyConfig) -> crate::Result<Self> {
        Self::new_with_options(tls, proxy, ConnectionOptions::default())
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
        })
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

            emit(AboutToSendHttpRequest { request: &request });

            let before = std::time::Instant::now();
            let response = self.client.send(request).await;
            let roundtrip = before.elapsed();

            match response {
                Ok(response) => {
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
