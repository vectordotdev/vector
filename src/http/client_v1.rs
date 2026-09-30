#![allow(dead_code)] // The remaining unused pieces will be exercised as more sinks migrate.

use std::{
    convert::Infallible,
    error::Error,
    fmt,
    future::Future,
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use bytes::Bytes;
use http_1::{
    HeaderValue, Method, Request, Response, Uri,
    header::{ACCEPT_ENCODING, HOST, PROXY_AUTHORIZATION, USER_AGENT},
};
use http_body_util::{BodyExt, Empty, Full, combinators::UnsyncBoxBody};
use hyper_1::{body::Incoming, rt};
use hyper_openssl_1::{SslStream, client::legacy::HttpsConnector};
use hyper_util::{
    client::legacy::{
        Client,
        connect::{Connected, Connection, HttpConnector},
    },
    rt::TokioExecutor,
};
use openssl::ssl::SslConnector;
use percent_encoding::percent_decode_str;
use tower::Service;
use tracing::Instrument;
use url::{Host, Url};

use crate::{
    config::ProxyConfig,
    internal_events::http_client::{
        AboutToSendHttpRequest, GotHttpResponse, GotHttpWarning, HttpRequestV1Telemetry,
        HttpResponseV1Telemetry,
    },
    tls::{MaybeTlsSettings, TlsSettings, tls_connector_builder},
};

type BoxError = Box<dyn Error + Send + Sync>;
pub(crate) type RequestBody = UnsyncBoxBody<Bytes, BoxError>;
const HTTP1_ALPN: &[u8] = b"\x08http/1.1";

#[derive(Debug)]
pub(crate) struct HttpError {
    source: BoxError,
    retriable: bool,
}

impl HttpError {
    pub(crate) fn new(source: impl Error + Send + Sync + 'static) -> Self {
        Self {
            source: Box::new(source),
            retriable: true,
        }
    }

    pub(crate) const fn is_retriable(&self) -> bool {
        self.retriable
    }
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "HTTP request failed: {}", self.source)
    }
}

impl Error for HttpError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref())
    }
}

impl From<BoxError> for HttpError {
    fn from(source: BoxError) -> Self {
        Self {
            source,
            retriable: false,
        }
    }
}

impl crate::sinks::util::http::HttpErrorClassify for HttpError {
    fn is_retriable(&self) -> bool {
        HttpError::is_retriable(self)
    }
}

pub(crate) fn empty_body() -> RequestBody {
    Empty::<Bytes>::new()
        .map_err(|never: Infallible| match never {})
        .boxed_unsync()
}

pub(crate) fn full_body(bytes: Bytes) -> RequestBody {
    Full::new(bytes)
        .map_err(|never: Infallible| match never {})
        .boxed_unsync()
}

#[derive(Clone)]
pub(crate) struct HttpClient {
    client: Client<HttpProxyConnectorV1, RequestBody>,
    routes: Arc<RoutePlanner>,
    user_agent: HeaderValue,
}

impl fmt::Debug for HttpClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("HttpClient").finish_non_exhaustive()
    }
}

impl HttpClient {
    pub(crate) fn new(tls: MaybeTlsSettings, proxy: &ProxyConfig) -> Result<Self, BoxError> {
        let routes = Arc::new(RoutePlanner::new(proxy)?);
        let connector = HttpProxyConnectorV1::new(tls, Arc::clone(&routes))?;
        let client = Client::builder(TokioExecutor::new()).build(connector);

        Ok(Self {
            client,
            routes,
            user_agent: default_user_agent(),
        })
    }

    pub(crate) async fn send(
        &self,
        mut request: Request<RequestBody>,
    ) -> Result<Response<Incoming>, HttpError> {
        let span = tracing::info_span!("http");
        async move {
            default_request_headers(&mut request, &self.user_agent);

            if let Route::ForwardProxy {
                authorization: Some(authorization),
                ..
            } = self.routes.route(request.uri()).map_err(HttpError::from)?
                && !request.headers().contains_key(PROXY_AUTHORIZATION)
            {
                request
                    .headers_mut()
                    .insert(PROXY_AUTHORIZATION, authorization);
            }

            {
                let telemetry = HttpRequestV1Telemetry::new(&request);
                emit!(AboutToSendHttpRequest {
                    request: &telemetry
                });
            }

            let before = std::time::Instant::now();
            let response = self.client.request(request).await;
            let roundtrip = before.elapsed();

            match response {
                Ok(response) => {
                    let telemetry = HttpResponseV1Telemetry::new(&response);
                    emit!(GotHttpResponse {
                        response: &telemetry,
                        roundtrip
                    });
                    Ok(response)
                }
                Err(error) => {
                    let error = HttpError::new(error);
                    emit!(GotHttpWarning {
                        error: &error,
                        roundtrip
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

#[derive(Clone)]
struct ProxyEndpoint {
    uri: Uri,
    authorization: Option<HeaderValue>,
}

impl ProxyEndpoint {
    fn parse(value: &str) -> Result<Self, BoxError> {
        let url = Url::parse(value)?;
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(invalid_input("proxy URI scheme must be http or https"));
        }

        let host = match url.host() {
            Some(Host::Domain(host)) => host.to_owned(),
            Some(Host::Ipv4(host)) => host.to_string(),
            Some(Host::Ipv6(host)) => format!("[{host}]"),
            None => return Err(invalid_input("proxy URI must contain a host")),
        };
        let authority = match url.port() {
            Some(port) => format!("{host}:{port}"),
            None => host,
        };
        let uri = Uri::builder()
            .scheme(url.scheme())
            .authority(authority)
            .path_and_query("/")
            .build()?;

        let authorization = url
            .password()
            .map(|password| -> Result<_, BoxError> {
                let username = percent_decode_str(url.username()).decode_utf8()?;
                let password = percent_decode_str(password).decode_utf8()?;
                let encoded =
                    openssl::base64::encode_block(format!("{username}:{password}").as_bytes());
                let mut value = HeaderValue::from_str(&format!("Basic {encoded}"))?;
                value.set_sensitive(true);
                Ok(value)
            })
            .transpose()?;

        Ok(Self { uri, authorization })
    }
}

#[derive(Clone)]
struct RoutePlanner {
    config: ProxyConfig,
    http: Option<ProxyEndpoint>,
    https: Option<ProxyEndpoint>,
}

impl RoutePlanner {
    fn new(config: &ProxyConfig) -> Result<Self, BoxError> {
        let (http, https) = if config.enabled {
            (
                config
                    .http
                    .as_deref()
                    .map(ProxyEndpoint::parse)
                    .transpose()?,
                config
                    .https
                    .as_deref()
                    .map(ProxyEndpoint::parse)
                    .transpose()?,
            )
        } else {
            (None, None)
        };

        Ok(Self {
            config: config.clone(),
            http,
            https,
        })
    }

    fn route(&self, destination: &Uri) -> Result<Route, BoxError> {
        let scheme = destination
            .scheme_str()
            .ok_or_else(|| invalid_input("destination URI must contain a scheme"))?;
        if scheme != "http" && scheme != "https" {
            return Err(invalid_input(
                "destination URI scheme must be http or https",
            ));
        }

        if !self.config.enabled || self.bypasses_proxy(destination) {
            return Ok(Route::Direct {
                destination: destination.clone(),
            });
        }

        let proxy = if scheme == "https" {
            self.https.clone()
        } else {
            self.http.clone()
        };
        let Some(proxy) = proxy else {
            return Ok(Route::Direct {
                destination: destination.clone(),
            });
        };

        if scheme == "https" {
            Ok(Route::ConnectProxy {
                proxy: proxy.uri,
                destination: destination.clone(),
                authorization: proxy.authorization,
            })
        } else {
            Ok(Route::ForwardProxy {
                proxy: proxy.uri,
                authorization: proxy.authorization,
            })
        }
    }

    fn bypasses_proxy(&self, destination: &Uri) -> bool {
        destination.host().is_some_and(|host| {
            self.config.no_proxy.matches(host)
                || destination
                    .port_u16()
                    .is_some_and(|port| self.config.no_proxy.matches(&format!("{host}:{port}")))
        })
    }
}

enum Route {
    Direct {
        destination: Uri,
    },
    ForwardProxy {
        proxy: Uri,
        authorization: Option<HeaderValue>,
    },
    ConnectProxy {
        proxy: Uri,
        destination: Uri,
        authorization: Option<HeaderValue>,
    },
}

type BaseConnector = HttpsConnector<HttpConnector>;

#[derive(Clone)]
struct HttpProxyConnectorV1 {
    direct: BaseConnector,
    proxy: BaseConnector,
    destination_tls: SslConnector,
    tls_settings: Option<TlsSettings>,
    routes: Arc<RoutePlanner>,
}

impl HttpProxyConnectorV1 {
    fn new(tls: MaybeTlsSettings, routes: Arc<RoutePlanner>) -> Result<Self, BoxError> {
        let tls_settings = tls.tls().cloned();
        let direct = https_connector(&tls, false)?;
        let proxy = https_connector(&tls, true)?;
        let destination_tls = tls_connector_builder(&tls)?.build();

        Ok(Self {
            direct,
            proxy,
            destination_tls,
            tls_settings,
            routes,
        })
    }

    async fn connect_tunnel(
        mut proxy: BaseConnector,
        destination_tls: SslConnector,
        tls_settings: Option<TlsSettings>,
        proxy_uri: Uri,
        destination: Uri,
        authorization: Option<HeaderValue>,
    ) -> Result<BoxedIo, BoxError> {
        let proxy_stream = proxy.call(proxy_uri).await?;
        let (mut sender, connection) =
            hyper_1::client::conn::http1::handshake(proxy_stream).await?;
        tokio::spawn(async move {
            if let Err(error) = connection.with_upgrades().await {
                tracing::debug!(message = "Proxy connection closed.", %error);
            }
        });

        let authority = connect_authority(&destination)?;
        let mut request = Request::builder()
            .method(Method::CONNECT)
            .uri(&authority)
            .header(HOST, &authority)
            .body(Empty::<Bytes>::new())?;
        if let Some(authorization) = authorization {
            request
                .headers_mut()
                .insert(PROXY_AUTHORIZATION, authorization);
        }

        let mut response = sender.send_request(request).await?;
        if !response.status().is_success() {
            return Err(invalid_input(format!(
                "proxy CONNECT failed with status {}",
                response.status()
            )));
        }
        let upgraded = hyper_1::upgrade::on(&mut response).await?;

        let host = tls_host(&destination)?;
        let mut configuration = destination_tls.configure()?;
        if let Some(settings) = &tls_settings {
            settings.apply_connect_configuration(&mut configuration, false)?;
        }
        let ssl = configuration.into_ssl(host)?;
        let mut stream = SslStream::new(ssl, upgraded)?;
        Pin::new(&mut stream).connect().await?;
        let negotiated_h2 = stream.ssl().selected_alpn_protocol() == Some(b"h2".as_slice());

        Ok(BoxedIo::new(TunnelIo {
            inner: stream,
            negotiated_h2,
        }))
    }
}

impl Service<Uri> for HttpProxyConnectorV1 {
    type Response = BoxedIo;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, destination: Uri) -> Self::Future {
        let route = self.routes.route(&destination);
        let mut direct = self.direct.clone();
        let mut proxy = self.proxy.clone();
        let destination_tls = self.destination_tls.clone();
        let tls_settings = self.tls_settings.clone();

        Box::pin(async move {
            match route? {
                Route::Direct { destination } => direct.call(destination).await.map(BoxedIo::new),
                Route::ForwardProxy {
                    proxy: proxy_uri, ..
                } => proxy
                    .call(proxy_uri)
                    .await
                    .map(|stream| BoxedIo::new(ForwardProxyIo(stream))),
                Route::ConnectProxy {
                    proxy: proxy_uri,
                    destination,
                    authorization,
                } => {
                    Self::connect_tunnel(
                        proxy,
                        destination_tls,
                        tls_settings,
                        proxy_uri,
                        destination,
                        authorization,
                    )
                    .await
                }
            }
        })
    }
}

fn https_connector(
    tls: &MaybeTlsSettings,
    skip_server_name: bool,
) -> Result<BaseConnector, BoxError> {
    let mut http = HttpConnector::new();
    http.enforce_http(false);
    let mut tls_builder = tls_connector_builder(tls)?;
    if skip_server_name {
        // Proxy connections are driven through HTTP/1.1, including CONNECT. Do not allow a TLS
        // proxy to negotiate h2 from the destination's configured ALPN list.
        tls_builder.set_alpn_protos(HTTP1_ALPN)?;
    }
    let mut https = HttpsConnector::with_connector(http, tls_builder)?;
    let settings = tls.tls().cloned();
    https.set_callback(move |configuration, _uri| {
        if let Some(settings) = &settings {
            settings.apply_connect_configuration(configuration, skip_server_name)?;
        }
        Ok(())
    });
    Ok(https)
}

fn connect_authority(destination: &Uri) -> Result<String, BoxError> {
    let authority = destination
        .authority()
        .ok_or_else(|| invalid_input("HTTPS destination must contain an authority"))?;
    Ok(if authority.port_u16().is_some() {
        authority.as_str().to_owned()
    } else {
        format!("{authority}:443")
    })
}

fn tls_host(destination: &Uri) -> Result<&str, BoxError> {
    let host = destination
        .host()
        .ok_or_else(|| invalid_input("HTTPS destination must contain a host"))?;
    Ok(host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host))
}

trait ConnectionIo: rt::Read + rt::Write + Connection + Unpin + Send {}

impl<T> ConnectionIo for T where T: rt::Read + rt::Write + Connection + Unpin + Send {}

struct BoxedIo(Pin<Box<dyn ConnectionIo>>);

impl BoxedIo {
    fn new<T>(stream: T) -> Self
    where
        T: ConnectionIo + 'static,
    {
        Self(Box::pin(stream))
    }
}

impl rt::Read for BoxedIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: rt::ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        self.0.as_mut().poll_read(cx, buffer)
    }
}

impl rt::Write for BoxedIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.0.as_mut().poll_write(cx, buffer)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.0.as_mut().poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.0.as_mut().poll_shutdown(cx)
    }

    fn is_write_vectored(&self) -> bool {
        self.0.is_write_vectored()
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffers: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        self.0.as_mut().poll_write_vectored(cx, buffers)
    }
}

impl Connection for BoxedIo {
    fn connected(&self) -> Connected {
        self.0.as_ref().get_ref().connected()
    }
}

struct ForwardProxyIo<T>(T);

impl<T: rt::Read + Unpin> rt::Read for ForwardProxyIo<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: rt::ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().0).poll_read(cx, buffer)
    }
}

impl<T: rt::Write + Unpin> rt::Write for ForwardProxyIo<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().0).poll_write(cx, buffer)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().0).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().0).poll_shutdown(cx)
    }

    fn is_write_vectored(&self) -> bool {
        self.0.is_write_vectored()
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffers: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().0).poll_write_vectored(cx, buffers)
    }
}

impl<T: Connection> Connection for ForwardProxyIo<T> {
    fn connected(&self) -> Connected {
        self.0.connected().proxy(true)
    }
}

struct TunnelIo<T> {
    inner: T,
    negotiated_h2: bool,
}

impl<T: rt::Read + Unpin> rt::Read for TunnelIo<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: rt::ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_read(cx, buffer)
    }
}

impl<T: rt::Write + Unpin> rt::Write for TunnelIo<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, buffer)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffers: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write_vectored(cx, buffers)
    }
}

impl<T> Connection for TunnelIo<T> {
    fn connected(&self) -> Connected {
        let connected = Connected::new();
        if self.negotiated_h2 {
            connected.negotiated_h2()
        } else {
            connected
        }
    }
}

fn invalid_input(message: impl Into<String>) -> BoxError {
    Box::new(io::Error::new(io::ErrorKind::InvalidInput, message.into()))
}

#[cfg(test)]
mod tests {
    use super::{connect_authority, tls_host};

    #[test]
    fn connect_authority_includes_a_port() {
        assert_eq!(
            connect_authority(&"https://example.com/path".parse().unwrap()).unwrap(),
            "example.com:443"
        );
        assert_eq!(
            connect_authority(&"https://example.com:8443/path".parse().unwrap()).unwrap(),
            "example.com:8443"
        );
        assert_eq!(
            connect_authority(&"https://[::1]/path".parse().unwrap()).unwrap(),
            "[::1]:443"
        );
    }

    #[test]
    fn tls_host_normalizes_ipv6_literals() {
        assert_eq!(
            tls_host(&"https://[::1]/path".parse().unwrap()).unwrap(),
            "::1"
        );
        assert_eq!(
            tls_host(&"https://127.0.0.1/path".parse().unwrap()).unwrap(),
            "127.0.0.1"
        );
        assert_eq!(
            tls_host(&"https://example.com/path".parse().unwrap()).unwrap(),
            "example.com"
        );
    }
}
