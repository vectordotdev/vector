use crate::dns::Resolver;
use crate::tls::{tls_connector_builder, MaybeTlsSettings};
use futures::future::BoxFuture;
use http::header::HeaderValue;
use http::Request;
use hyper::{
    body::{Body, HttpBody},
    client::{Client, HttpConnector},
};
use hyper_openssl::HttpsConnector;
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    task::{Context, Poll},
};
use tower::Service;
use tracing::Span;
use tracing_futures::Instrument;

pub type HttpClientFuture = <HttpClient as Service<http::Request<Body>>>::Future;

pub struct HttpClient<B = Body> {
    client: Client<HttpsConnector<HttpConnector<Resolver>>, B>,
    span: Span,
    user_agent: HeaderValue,
}

impl<B> HttpClient<B>
where
    B: HttpBody + Send + 'static,
    B::Data: Send,
    B::Error: Into<crate::Error>,
{
    pub fn new(
        resolver: Resolver,
        tls_settings: impl Into<MaybeTlsSettings>,
    ) -> crate::Result<HttpClient<B>> {
        let mut http = HttpConnector::new_with_resolver(resolver);
        http.enforce_http(false);

        let settings = tls_settings.into();
        let tls = tls_connector_builder(&settings)?;
        let mut https = HttpsConnector::with_connector(http, tls)?;

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

    pub async fn send(&mut self, request: Request<B>) -> crate::Result<http::Response<Body>> {
        self.call(request).await.map_err(Into::into)
    }
}

impl<B> Service<Request<B>> for HttpClient<B>
where
    B: HttpBody + Send + 'static,
    B::Data: Send,
    B::Error: Into<crate::Error>,
{
    type Response = http::Response<Body>;
    type Error = hyper::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut request: Request<B>) -> Self::Future {
        let _enter = self.span.enter();

        if !request.headers().contains_key("User-Agent") {
            request
                .headers_mut()
                .insert("User-Agent", self.user_agent.clone());
        }

        debug!(message = "Sending request.", uri = %request.uri(), method = %request.method());

        let response = self.client.request(request);

        let fut = async move {
            let res = response.await?;
            debug!(
                    message = "Response.",
                    status = ?res.status(),
                    version = ?res.version(),
            );
            Ok(res)
        }
        .instrument(self.span.clone());

        Box::pin(fut)
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
    pub fn apply<B>(&self, req: &mut Request<B>) {
        use headers::{Authorization, HeaderMapExt};

        match &self {
            Auth::Basic { user, password } => {
                let auth = Authorization::basic(&user, &password);
                req.headers_mut().typed_insert(auth);
            }
            Auth::Bearer { token } => match Authorization::bearer(&token) {
                Ok(auth) => req.headers_mut().typed_insert(auth),
                Err(error) => error!(message = "invalid bearer token", %token, %error),
            },
        }
    }
}
