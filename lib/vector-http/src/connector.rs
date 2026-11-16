//! Connection routing: direct, through a forward proxy, or through a `CONNECT` tunnel.

use std::{
    error::Error,
    future::Future,
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use bytes::Bytes;
use http::{
    HeaderValue, Method, Request, Uri,
    header::{HOST, PROXY_AUTHORIZATION},
};
use http_body_util::Empty;
use hyper::rt;
use hyper_openssl::{SslStream, client::legacy::HttpsConnector};
use hyper_util::client::legacy::connect::{Connected, Connection, HttpConnector};
use openssl::ssl::SslConnector;
use percent_encoding::percent_decode_str;
use tokio::time::{Instant, Sleep, timeout};
use tower::Service;
use url::{Host, Url};
use vector_core::{
    config::proxy::ProxyConfig,
    tls::{MaybeTlsSettings, TlsSettings, tls_connector_builder},
};

use crate::{ConnectionOptions, jittered_duration};

type BoxError = Box<dyn Error + Send + Sync>;

const HTTP1_ALPN: &[u8] = b"\x08http/1.1";

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
pub(crate) struct RoutePlanner {
    config: ProxyConfig,
    http: Option<ProxyEndpoint>,
    https: Option<ProxyEndpoint>,
}

impl RoutePlanner {
    pub(crate) fn new(config: &ProxyConfig) -> Result<Self, BoxError> {
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

    pub(crate) fn route(&self, destination: &Uri) -> Result<Route, BoxError> {
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

pub(crate) enum Route {
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

/// ALPN protocols offered to a TLS forward (non-`CONNECT`) proxy.
#[derive(Clone, Copy)]
pub(crate) enum ForwardProxyAlpn {
    /// Only `http/1.1`, for [`HttpClient`](crate::HttpClient).
    Http1,
    /// The configured `tls.alpn_protocols`, for [`Http2Client`](crate::Http2Client).
    Configured,
}

#[derive(Clone)]
pub(crate) struct HttpProxyConnector {
    direct: BaseConnector,
    forward_proxy: BaseConnector,
    connect_proxy: BaseConnector,
    destination_tls: SslConnector,
    tls_settings: Option<TlsSettings>,
    routes: Arc<RoutePlanner>,
    max_connection_age: Option<(Duration, f64)>,
    connect_timeout: Option<Duration>,
}

impl HttpProxyConnector {
    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::needless_pass_by_value,
        reason = "Preserve ownership and drop timing"
    )]
    pub(crate) fn new(
        tls: MaybeTlsSettings,
        routes: Arc<RoutePlanner>,
        forward_proxy_alpn: ForwardProxyAlpn,
    ) -> Result<Self, BoxError> {
        let tls_settings = tls.tls().cloned();
        let direct = https_connector(&tls, false, false)?;
        // `CONNECT` is always driven through HTTP/1.1. Do not allow a TLS proxy to negotiate h2
        // from the destination's configured ALPN list.
        let connect_proxy = https_connector(&tls, true, true)?;
        let forward_proxy = match forward_proxy_alpn {
            ForwardProxyAlpn::Http1 => connect_proxy.clone(),
            ForwardProxyAlpn::Configured => https_connector(&tls, true, false)?,
        };
        let destination_tls = tls_connector_builder(&tls)?.build();

        Ok(Self {
            direct,
            forward_proxy,
            connect_proxy,
            destination_tls,
            tls_settings,
            routes,
            max_connection_age: None,
            connect_timeout: None,
        })
    }

    pub(crate) const fn with_options(mut self, options: ConnectionOptions) -> Self {
        self.max_connection_age = match options.max_connection_age {
            Some(age) => Some((age, options.max_connection_age_jitter_factor)),
            None => None,
        };
        self.connect_timeout = options.connect_timeout;
        self
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
        let (mut sender, connection) = hyper::client::conn::http1::handshake(proxy_stream).await?;
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
        let upgraded = hyper::upgrade::on(&mut response).await?;

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

impl Service<Uri> for HttpProxyConnector {
    type Response = BoxedIo;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, destination: Uri) -> Self::Future {
        let route = self.routes.route(&destination);
        let mut direct = self.direct.clone();
        let mut forward_proxy = self.forward_proxy.clone();
        let connect_proxy = self.connect_proxy.clone();
        let destination_tls = self.destination_tls.clone();
        let tls_settings = self.tls_settings.clone();
        let max_connection_age = self.max_connection_age;
        let connect_timeout = self.connect_timeout;

        Box::pin(async move {
            let connect = async move {
                match route? {
                    Route::Direct { destination } => {
                        direct.call(destination).await.map(BoxedIo::new)
                    }
                    Route::ForwardProxy {
                        proxy: proxy_uri, ..
                    } => forward_proxy
                        .call(proxy_uri)
                        .await
                        .map(|stream| BoxedIo::new(ForwardProxyIo(stream))),
                    Route::ConnectProxy {
                        proxy: proxy_uri,
                        destination,
                        authorization,
                    } => {
                        Self::connect_tunnel(
                            connect_proxy,
                            destination_tls,
                            tls_settings,
                            proxy_uri,
                            destination,
                            authorization,
                        )
                        .await
                    }
                }
            };
            let io = match connect_timeout {
                Some(duration) => timeout(duration, connect)
                    .await
                    .map_err(|_elapsed| connect_timed_out(duration))??,
                None => connect.await?,
            };
            Ok(match max_connection_age {
                Some((age, jitter_factor)) => BoxedIo::new(MaxAgeIo::new(
                    io,
                    Instant::now() + jittered_duration(age, jitter_factor),
                )),
                None => io,
            })
        })
    }
}

fn https_connector(
    tls: &MaybeTlsSettings,
    skip_server_name: bool,
    http1_alpn: bool,
) -> Result<BaseConnector, BoxError> {
    let mut http = HttpConnector::new();
    http.enforce_http(false);
    let mut tls_builder = tls_connector_builder(tls)?;
    if http1_alpn {
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

pub(crate) struct BoxedIo(Pin<Box<dyn ConnectionIo>>);

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

/// Closes the connection once `deadline` passes: reads and writes fail, so hyper drops the
/// connection instead of returning it to the pool. Reads fail rather than report EOF, which would
/// complete a close-delimited response body with truncated data.
struct MaxAgeIo<T> {
    inner: T,
    // `Sleep` is `!Unpin`, and hyper requires connections to be `Unpin`.
    deadline: Pin<Box<Sleep>>,
    expired: bool,
}

impl<T> MaxAgeIo<T> {
    fn new(inner: T, deadline: Instant) -> Self {
        Self {
            inner,
            deadline: Box::pin(tokio::time::sleep_until(deadline)),
            expired: false,
        }
    }

    fn poll_expired(&mut self, cx: &mut Context<'_>) -> bool {
        if !self.expired && self.deadline.as_mut().poll(cx).is_ready() {
            self.expired = true;
        }
        self.expired
    }
}

fn connection_expired() -> io::Error {
    io::Error::new(
        io::ErrorKind::BrokenPipe,
        "connection exceeded its maximum age",
    )
}

impl<T: rt::Read + Unpin> rt::Read for MaxAgeIo<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: rt::ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.poll_expired(cx) {
            return Poll::Ready(Err(connection_expired()));
        }
        Pin::new(&mut this.inner).poll_read(cx, buffer)
    }
}

impl<T: rt::Write + Unpin> rt::Write for MaxAgeIo<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if this.poll_expired(cx) {
            return Poll::Ready(Err(connection_expired()));
        }
        Pin::new(&mut this.inner).poll_write(cx, buffer)
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
        let this = self.get_mut();
        if this.poll_expired(cx) {
            return Poll::Ready(Err(connection_expired()));
        }
        Pin::new(&mut this.inner).poll_write_vectored(cx, buffers)
    }
}

impl<T: Connection> Connection for MaxAgeIo<T> {
    fn connected(&self) -> Connected {
        self.inner.connected()
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

fn connect_timed_out(duration: Duration) -> BoxError {
    Box::new(io::Error::new(
        io::ErrorKind::TimedOut,
        format!("HTTP connect timeout occurred after {duration:?}"),
    ))
}

#[cfg(test)]
mod tests {
    use std::{
        error::Error,
        io::{self, ErrorKind},
        iter,
        time::Duration,
    };

    use http::{HeaderValue, Request, Uri};
    use http_body_util::BodyExt as _;
    use hyper_util::rt::TokioIo;
    use tokio::{
        io::{AsyncReadExt as _, AsyncWriteExt as _, duplex},
        net::TcpListener,
        time::Instant,
    };
    use vector_core::{
        config::proxy::ProxyConfig,
        tls::{MaybeTls, MaybeTlsSettings},
    };

    use super::{MaxAgeIo, Route, RoutePlanner, connect_authority, tls_host};
    use crate::{ConnectionOptions, HttpClient, empty_body};

    /// Returns the proxy URI and `Proxy-Authorization` value of the route to `destination`.
    fn proxy_route(config: &ProxyConfig, destination: &str) -> (Uri, Option<HeaderValue>) {
        let route = RoutePlanner::new(config)
            .unwrap()
            .route(&destination.parse().unwrap())
            .unwrap();
        match route {
            Route::ForwardProxy {
                proxy,
                authorization,
            }
            | Route::ConnectProxy {
                proxy,
                authorization,
                ..
            } => (proxy, authorization),
            Route::Direct { .. } => panic!("{destination} must be proxied"),
        }
    }

    #[test]
    fn routes_each_scheme_to_its_proxy() {
        let config = ProxyConfig {
            http: Some("http://1.2.3.4:5678".into()),
            https: Some("https://2.3.4.5:9876".into()),
            ..Default::default()
        };

        let (proxy, authorization) = proxy_route(&config, "http://example.com/");
        assert_eq!(proxy, "http://1.2.3.4:5678/");
        assert!(authorization.is_none());

        let (proxy, authorization) = proxy_route(&config, "https://example.com/");
        assert_eq!(proxy, "https://2.3.4.5:9876/");
        assert!(authorization.is_none());
    }

    #[test]
    fn proxy_credentials_become_proxy_authorization() {
        let config = ProxyConfig {
            http: Some("http://user:pass@1.2.3.4:5678".into()),
            https: Some("https://user:pass@2.3.4.5:9876".into()),
            ..Default::default()
        };

        for (destination, expected_proxy) in [
            ("http://example.com/", "http://1.2.3.4:5678/"),
            ("https://example.com/", "https://2.3.4.5:9876/"),
        ] {
            let (proxy, authorization) = proxy_route(&config, destination);
            // The credentials leave the proxy URI and go only into `Proxy-Authorization`.
            assert_eq!(proxy, expected_proxy);
            let authorization = authorization.expect("proxy credentials are configured");
            assert_eq!(authorization, "Basic dXNlcjpwYXNz");
            assert!(authorization.is_sensitive());
        }
    }

    #[test]
    fn proxy_credentials_are_percent_decoded() {
        let config = ProxyConfig {
            http: Some("http://user:P%40ssw0rd@1.2.3.4:5678".into()),
            ..Default::default()
        };

        let (_, authorization) = proxy_route(&config, "http://example.com/");
        // base64("user:P@ssw0rd")
        assert_eq!(authorization.unwrap(), "Basic dXNlcjpQQHNzdzByZA==");
    }

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

    #[tokio::test(start_paused = true)]
    async fn max_age_io_closes_after_deadline() {
        let (client, mut server) = duplex(64);
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut io = TokioIo::new(MaxAgeIo::new(TokioIo::new(client), deadline));

        server.write_all(b"hello").await.unwrap();
        let mut buffer = [0u8; 5];
        io.read_exact(&mut buffer).await.unwrap();
        assert_eq!(&buffer, b"hello");

        // Past the deadline, reads fail even with data available, and so do writes.
        server.write_all(b"world").await.unwrap();
        tokio::time::advance(Duration::from_secs(11)).await;
        let error = io.read(&mut buffer).await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::BrokenPipe);
        let error = io.write(b"data").await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::BrokenPipe);
    }

    #[tokio::test]
    async fn connect_timeout_bounds_a_stalled_tls_handshake() {
        // Accept the TCP connection and hold it open without ever answering the TLS handshake.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let accepted = tokio::spawn(async move { listener.accept().await.unwrap().0 });

        let client = HttpClient::new_with_options(
            MaybeTlsSettings::enable_client().unwrap(),
            &ProxyConfig::default(),
            ConnectionOptions::default().connect_timeout(Some(Duration::from_millis(200))),
        )
        .unwrap();
        let request = Request::get(format!("https://127.0.0.1:{port}/"))
            .body(empty_body())
            .unwrap();

        let error = tokio::time::timeout(Duration::from_secs(10), client.send(request))
            .await
            .expect("the connect timeout should fail the request")
            .unwrap_err();
        let timed_out = iter::successors(Some(&error as &(dyn Error + 'static)), |error| {
            (*error).source()
        })
        .filter_map(|error| error.downcast_ref::<io::Error>())
        .any(|error| error.kind() == ErrorKind::TimedOut);
        assert!(timed_out, "expected a timed out I/O error, got {error:?}");

        // The TCP connection was established, so the timeout hit during the TLS handshake.
        let _stream = accepted.await.unwrap();
    }

    #[tokio::test]
    async fn idle_pooled_connections_are_closed_in_the_background() {
        // Answer one keep-alive request, then report what the next read of the idle connection
        // returns: `0` once the client closes it.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 4096];
            let mut request = Vec::new();
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = stream.read(&mut buffer).await.unwrap();
                assert_ne!(
                    read, 0,
                    "the client closed the connection before the request"
                );
                request.extend_from_slice(&buffer[..read]);
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n")
                .await
                .unwrap();
            stream.read(&mut buffer).await.unwrap()
        });

        let client = HttpClient::new(MaybeTls::Raw(()), &ProxyConfig::default()).unwrap();
        let request = Request::get(format!("http://127.0.0.1:{port}/"))
            .body(empty_body())
            .unwrap();
        let response = client.send(request).await.unwrap();
        response.into_body().collect().await.unwrap();
        // Let the connection return to the pool before the clock is paused.
        tokio::time::sleep(Duration::from_millis(50)).await;

        // The pool's default idle timeout is 90 seconds. The background check runs once per
        // timeout, and evicts connections idle for longer than it, so the second check does.
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(181)).await;
        tokio::time::resume();

        let read = tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .expect("the idle connection should be closed without another request")
            .unwrap();
        assert_eq!(read, 0);
        drop(client);
    }
}
