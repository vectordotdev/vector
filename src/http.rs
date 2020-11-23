use crate::{
    dns::Resolver,
    internal_events::http_client,
    tls::{tls_connector_builder, MaybeTlsSettings, TlsError},
};
use futures::future::BoxFuture;
use headers::{Authorization, HeaderMapExt};
use http::header::HeaderValue;
use http::request::Builder;
use http::HeaderMap;
use http::Request;
use hyper::{
    body::{Body, HttpBody},
    client::{Client, HttpConnector},
};
use hyper_openssl::HttpsConnector;
use percent_encoding::percent_decode;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::{
    fmt,
    task::{Context, Poll},
};
use tower::Service;
use tracing::Span;
use tracing_futures::Instrument;

#[derive(Debug, Snafu)]
pub enum HttpError {
    #[snafu(display("Failed to build TLS connector"))]
    BuildTlsConnector { source: TlsError },
    #[snafu(display("Failed to build HTTPS connector"))]
    MakeHttpsConnector { source: openssl::error::ErrorStack },
    #[snafu(display("Failed to make HTTP(S) request"))]
    CallRequest { source: hyper::Error },
}

pub type HttpClientFuture = <HttpClient as Service<http::Request<Body>>>::Future;

pub struct HttpClient<B = Body> {
    client: Client<HttpsConnector<HttpConnector<Resolver>>, B>,
    span: Span,
    user_agent: HeaderValue,
}

impl<B> HttpClient<B>
where
    B: fmt::Debug + HttpBody + Send + 'static,
    B::Data: Send,
    B::Error: Into<crate::Error>,
{
    pub fn new(tls_settings: impl Into<MaybeTlsSettings>) -> Result<HttpClient<B>, HttpError> {
        let mut http = HttpConnector::new_with_resolver(Resolver);
        http.enforce_http(false);

        let settings = tls_settings.into();
        let tls = tls_connector_builder(&settings).context(BuildTlsConnector)?;
        let mut https = HttpsConnector::with_connector(http, tls).context(MakeHttpsConnector)?;

        let settings = settings.tls().cloned();
        https.set_callback(move |c, _uri| {
            if let Some(settings) = &settings {
                settings.apply_connect_configuration(c);
            }

            Ok(())
        });

        let client = Client::builder().build(https);

        let version = crate::get_version();
        let user_agent = HeaderValue::from_str(&format!("Vector/{}", version))
            .expect("Invalid header value for version!");

        let span = tracing::info_span!("http");

        Ok(HttpClient {
            client,
            span,
            user_agent,
        })
    }

    pub fn send(
        &self,
        mut request: Request<B>,
    ) -> BoxFuture<'static, Result<http::Response<Body>, HttpError>> {
        let _enter = self.span.enter();

        if !request.headers().contains_key("User-Agent") {
            request
                .headers_mut()
                .insert("User-Agent", self.user_agent.clone());
        }

        emit!(http_client::AboutToSendHTTPRequest { request: &request });

        let response = self.client.request(request);

        let fut = async move {
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
                .map_err(|error| {
                    // Emit the error into the internal events system.
                    emit!(http_client::GotHTTPError {
                        error: &error,
                        roundtrip
                    });
                    error
                })
                .context(CallRequest)?;

            // Emit the response into the internal events system.
            emit!(http_client::GotHTTPResponse {
                response: &response,
                roundtrip
            });
            Ok(response)
        }
        .instrument(self.span.clone());

        Box::pin(fut)
    }
}

impl<B> Service<Request<B>> for HttpClient<B>
where
    B: fmt::Debug + HttpBody + Send + 'static,
    B::Data: Send,
    B::Error: Into<crate::Error>,
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
            span: self.span.clone(),
            user_agent: self.user_agent.clone(),
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

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "strategy")]
pub enum Auth {
    Basic { user: String, password: String },
    Bearer { token: String },
}

impl Auth {
    /// Get basic-auth credentials encoded in the url,
    /// example: http://user:password@example.com/ .
    /// Remove the credentials from the url if exist.
    pub fn get_and_strip_basic_auth(url: &str) -> (String, Option<Self>) {
        match Self::get_and_strip(url) {
            Some((url, auth)) => (url, Some(auth)),
            None => (url.to_owned(), None),
        }
    }

    // We can use `?` with this return type.
    fn get_and_strip(url: &str) -> Option<(String, Self)> {
        let mut url = url::Url::parse(url).ok()?;

        let user = url.username();
        let scheme = url.scheme();
        if !user.is_empty() && (scheme == "http" || scheme == "https") {
            let user = percent_decode(user.as_bytes())
                .decode_utf8_lossy()
                .into_owned();

            let password = url.password().unwrap_or("");
            let password = percent_decode(password.as_bytes())
                .decode_utf8_lossy()
                .into_owned();

            url.set_username("").ok()?;
            url.set_password(None).ok()?;

            Some((url.to_string(), Auth::Basic { user, password }))
        } else {
            None
        }
    }

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
                let auth = Authorization::basic(&user, &password);
                map.typed_insert(auth);
            }
            Auth::Bearer { token } => match Authorization::bearer(&token) {
                Ok(auth) => map.typed_insert(auth),
                Err(error) => error!(message = "Invalid bearer token.", token = %token, %error),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Auth;
    use http::HeaderMap;

    fn test_basic_auth(url: &str) -> (String, Option<String>) {
        let (url, auth) = Auth::get_and_strip_basic_auth(url);
        (
            url,
            auth.map(|auth| {
                let mut map = HeaderMap::new();
                auth.apply_headers_map(&mut map);
                map["authorization"].to_str().unwrap().to_owned()
            }),
        )
    }

    #[test]
    fn basic_auth_url() {
        assert_eq!(
            test_basic_auth("http://user:pass@example.com"),
            (
                "http://example.com/".to_owned(),
                Some(format!("Basic {}", base64::encode("user:pass")))
            )
        );

        // special character
        assert_eq!(
            test_basic_auth("http://user:pass;@example.com"),
            (
                "http://example.com/".to_owned(),
                Some(format!("Basic {}", base64::encode("user:pass;")))
            )
        );

        // no password
        assert_eq!(
            test_basic_auth("http://user@example.com"),
            (
                "http://example.com/".to_owned(),
                Some(format!("Basic {}", base64::encode("user:")))
            )
        );

        assert_eq!(
            test_basic_auth("http://example.com:8080/test"),
            ("http://example.com:8080/test".to_owned(), None)
        );

        assert_eq!(
            test_basic_auth("mailto:admin@example.com"),
            ("mailto:admin@example.com".to_owned(), None)
        );

        assert_eq!(test_basic_auth("/test"), ("/test".to_owned(), None));

        // url without protocol is not supported
        assert_eq!(
            test_basic_auth("user:pass@example.com/test"),
            ("user:pass@example.com/test".to_owned(), None)
        );

        assert_eq!(
            test_basic_auth("ftp://user:pass@example.com"),
            ("ftp://user:pass@example.com".to_owned(), None)
        );
    }
}
