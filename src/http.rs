use crate::{
    dns::Resolver,
    tls::{tls_connector_builder, MaybeTlsSettings, TlsError},
};
use futures::future::BoxFuture;
use http::header::HeaderValue;
use http::request::Builder;
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

        debug!(
            message = "Sending HTTP request.",
            uri = %request.uri(),
            method = %request.method(),
            version = ?request.version(),
            headers = ?request.headers(),
            body = %FormatBody(request.body()),
        );

        let response = self.client.request(request);

        let fut = async move {
            let res = response.await.context(CallRequest)?;
            debug!(
                    message = "HTTP response.",
                    status = %res.status(),
                    version = ?res.version(),
                    headers = ?res.headers(),
                    body = %FormatBody(res.body()),
            );
            Ok(res)
        }
        .instrument(self.span.clone());

        Box::pin(fut)
    }
}

/// Newtype placeholder to provide a formatter for the request and response body.
struct FormatBody<'a, B>(&'a B);

impl<'a, B: HttpBody> fmt::Display for FormatBody<'a, B> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        let size = self.0.size_hint();
        match (size.lower(), size.upper()) {
            (0, None) => write!(fmt, "[unknown]"),
            (lower, None) => write!(fmt, "[>={} bytes]", lower),

            (0, Some(0)) => write!(fmt, "[empty]"),
            (0, Some(upper)) => write!(fmt, "[<={} bytes]", upper),

            (lower, Some(upper)) if lower == upper => write!(fmt, "[{} bytes]", lower),
            (lower, Some(upper)) => write!(fmt, "[{}..={} bytes]", lower, upper),
        }
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
    pub fn get_and_strip_basic_auth(url: &str) -> Option<(String, Self)> {
        let mut url = url::Url::parse(url).ok()?;

        let user = url.username();
        let password = url.password().unwrap_or("");
        if !user.is_empty() {
            let user = percent_decode(user.as_bytes())
                .decode_utf8_lossy()
                .into_owned();
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
        use headers::{Authorization, HeaderMapExt};

        match &self {
            Auth::Basic { user, password } => {
                let auth = Authorization::basic(&user, &password);
                req.headers_mut().typed_insert(auth);
            }
            Auth::Bearer { token } => match Authorization::bearer(&token) {
                Ok(auth) => req.headers_mut().typed_insert(auth),
                Err(error) => error!(message = "Invalid bearer token.", token = %token, %error),
            },
        }
    }

    pub fn apply_builder(&self, builder: Builder) -> Builder {
        builder
    }
}

#[cfg(test)]
mod tests {
    use super::Auth;

    fn test_basic_auth(url: &str) -> Option<(String, String)> {
        Auth::get_and_strip_basic_auth(url).map(|(url, auth)| {
            let mut request = http::Request::new(());
            auth.apply(&mut request);
            (
                url,
                (request.headers())["authorization"]
                    .to_str()
                    .unwrap()
                    .to_owned(),
            )
        })
    }

    #[test]
    fn basic_auth_url() {
        assert_eq!(
            test_basic_auth("http://user:pass@example.com"),
            Some((
                "http://example.com/".to_owned(),
                format!("Basic {}", base64::encode("user:pass"))
            ))
        );

        // special character
        assert_eq!(
            test_basic_auth("http://user:pass;@example.com"),
            Some((
                "http://example.com/".to_owned(),
                format!("Basic {}", base64::encode("user:pass;"))
            ))
        );

        // no password
        assert_eq!(
            test_basic_auth("http://user@example.com"),
            Some((
                "http://example.com/".to_owned(),
                format!("Basic {}", base64::encode("user:"))
            ))
        );

        assert_eq!(test_basic_auth("http://example.com:8080/test"), None);
    }
}
