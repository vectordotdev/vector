#![allow(missing_docs)]
use std::{
    fmt,
    task::{Context, Poll},
};

use futures::future::BoxFuture;
use http::{Request, header::HeaderValue};
use hyper::{
    body::{Body, HttpBody},
    client,
    client::Client,
};
use hyper_openssl::HttpsConnector;
use hyper_proxy::ProxyConnector;
use hyperlocal::UnixConnector;
use snafu::ResultExt;
use tower::Service;
use tracing::Instrument;
use vector_lib::{
    config::proxy::ProxyConfig,
    tls::{MaybeTlsSettings, tls_connector_builder},
};

use crate::{
    http::{
        BuildTlsConnectorSnafu, CallRequestSnafu, HttpError, MakeHttpsConnectorSnafu,
        MakeProxyConnectorSnafu, default_request_headers,
    },
    internal_events::http_client,
};

type UnixHttpProxyConnector = ProxyConnector<HttpsConnector<UnixConnector>>;

pub struct UnixHttpClient<B = Body> {
    client: Client<UnixHttpProxyConnector, B>,
    user_agent: HeaderValue,
    proxy_connector: UnixHttpProxyConnector,
}

pub fn build_unix_tls_connector(
    tls_settings: MaybeTlsSettings,
) -> Result<HttpsConnector<UnixConnector>, HttpError> {
    let tls = tls_connector_builder(&tls_settings).context(BuildTlsConnectorSnafu)?;
    let mut https =
        HttpsConnector::with_connector(UnixConnector, tls).context(MakeHttpsConnectorSnafu)?;

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

pub fn build_unix_proxy_connector(
    tls_settings: MaybeTlsSettings,
    proxy_config: &ProxyConfig,
) -> Result<ProxyConnector<HttpsConnector<UnixConnector>>, HttpError> {
    // Create dedicated unix TLS connector for the proxied connection with user TLS settings.
    let tls = tls_connector_builder(&tls_settings)
        .context(BuildTlsConnectorSnafu)?
        .build();

    let https = build_unix_tls_connector(tls_settings)?;
    let mut proxy = ProxyConnector::new(https).unwrap();
    // Make proxy connector aware of user TLS settings by setting the TLS connector:
    // https://github.com/vectordotdev/vector/issues/13683
    proxy.set_tls(Some(tls));
    proxy_config
        .configure(&mut proxy)
        .context(MakeProxyConnectorSnafu)?;
    Ok(proxy)
}

impl<B> UnixHttpClient<B>
where
    B: fmt::Debug + HttpBody + Send + 'static,
    B::Data: Send,
    B::Error: Into<crate::Error>,
{
    pub fn new(
        tls_settings: impl Into<MaybeTlsSettings>,
        proxy_config: &ProxyConfig,
    ) -> Result<UnixHttpClient<B>, HttpError> {
        UnixHttpClient::new_with_custom_client(tls_settings, proxy_config, &mut Client::builder())
    }

    pub fn new_with_custom_client(
        tls_settings: impl Into<MaybeTlsSettings>,
        proxy_config: &ProxyConfig,
        client_builder: &mut client::Builder,
    ) -> Result<UnixHttpClient<B>, HttpError> {
        let proxy_connector = build_unix_proxy_connector(tls_settings.into(), proxy_config)?;
        let client = client_builder.build(proxy_connector.clone());

        let app_name = crate::get_app_name();
        let version = crate::get_version();
        let user_agent = HeaderValue::from_str(&format!("{app_name}/{version}"))
            .expect("Invalid header value for user-agent!");

        Ok(UnixHttpClient {
            client,
            user_agent,
            proxy_connector,
        })
    }

    // TODO: check this with uri?
    pub fn send(
        &self,
        mut request: Request<B>,
    ) -> BoxFuture<'static, Result<http::Response<Body>, HttpError>> {
        let span = tracing::info_span!("http");
        let _enter = span.enter();

        default_request_headers(&mut request, &self.user_agent);
        self.maybe_add_proxy_headers(&mut request);

        emit!(http_client::AboutToSendHttpRequest { request: &request });

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

impl<B> Service<Request<B>> for UnixHttpClient<B>
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

impl<B> Clone for UnixHttpClient<B> {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            user_agent: self.user_agent.clone(),
            proxy_connector: self.proxy_connector.clone(),
        }
    }
}

impl<B> fmt::Debug for UnixHttpClient<B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnixHttpClient")
            .field("client", &self.client)
            .field("user_agent", &self.user_agent)
            .finish()
    }
}
