use std::{
    fmt::Debug,
    net::SocketAddr,
    num::NonZeroU64,
    task::{Context, Poll},
    time::Duration,
};

use snafu::{ResultExt, Snafu};
use tokio::{net::TcpStream, time};
use url::Url;
use vector_config_macros::configurable_component;
use yawc::{HttpRequest, Options as WsOptions, WebSocket};

use crate::{
    common::backoff::ExponentialBackoff,
    dns,
    http::Auth,
    internal_events::{WebSocketConnectionEstablished, WebSocketConnectionFailedError},
    tls::{MaybeTlsSettings, MaybeTlsStream, TlsEnableableConfig, TlsError},
};

#[allow(unreachable_pub)]
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum WebSocketError {
    #[snafu(display("Creating WebSocket client failed: {}", source))]
    CreateFailed { source: yawc::WebSocketError },
    #[snafu(display("Connect error: {}", source))]
    ConnectError { source: TlsError },
    #[snafu(display("Unable to resolve DNS: {}", source))]
    DnsError { source: dns::DnsError },
    #[snafu(display("No addresses returned."))]
    NoAddresses,
    #[snafu(display("Connection attempt timed out"))]
    ConnectionTimedOut,
    #[snafu(display("Invalid URI: {}", source))]
    InvalidUri { source: url::ParseError },
}

#[derive(Clone)]
pub(crate) struct WebSocketConnector {
    uri: String,
    url: Url,
    host: String,
    port: u16,
    tls: MaybeTlsSettings,
    auth: Option<Auth>,
    compression: Option<WebSocketCompression>,
}

impl WebSocketConnector {
    pub(crate) fn new(
        uri: String,
        tls: MaybeTlsSettings,
        auth: Option<Auth>,
        compression: Option<WebSocketCompression>,
    ) -> Result<Self, WebSocketError> {
        let url = Url::parse(&uri).context(InvalidUriSnafu)?;
        let host = url
            .host_str()
            .ok_or(WebSocketError::NoAddresses)?
            .to_string();
        let port = url.port_or_known_default().unwrap_or(match url.scheme() {
            "wss" => 443,
            _ => 80,
        });

        Ok(Self {
            uri,
            url,
            host,
            port,
            tls,
            auth,
            compression,
        })
    }

    async fn tls_connect(&self) -> Result<MaybeTlsStream<TcpStream>, WebSocketError> {
        let ip = dns::Resolver
            .lookup_ip(self.host.clone())
            .await
            .context(DnsSnafu)?
            .next()
            .ok_or(WebSocketError::NoAddresses)?;

        let addr = SocketAddr::new(ip, self.port);
        self.tls
            .connect(&self.host, &addr)
            .await
            .context(ConnectSnafu)
    }

    fn build_options(&self) -> WsOptions {
        let mut options = WsOptions::default();
        if let Some(ref compression) = self.compression {
            options = options.with_compression_level(flate2::Compression::new(compression.level));
        }
        options
    }

    async fn connect(
        &self,
    ) -> Result<WebSocket<MaybeTlsStream<TcpStream>>, WebSocketError> {
        let maybe_tls = self.tls_connect().await?;

        let mut builder = HttpRequest::builder();

        // Apply auth headers by building a temporary http 0.2 request, applying auth, then
        // copying headers to the yawc HttpRequestBuilder (http 1.x)
        if let Some(auth) = &self.auth {
            let mut tmp_request = http::Request::builder()
                .uri(&self.uri)
                .body(())
                .expect("failed to build temp request");
            auth.apply(&mut tmp_request);
            for (key, value) in tmp_request.headers() {
                builder = builder.header(key.as_str(), value.to_str().unwrap_or(""));
            }
        }

        let options = self.build_options();

        WebSocket::handshake_with_request(self.url.clone(), maybe_tls, options, builder)
            .await
            .context(CreateFailedSnafu)
    }

    #[cfg(feature = "sinks-websocket")]
    pub(crate) async fn connect_backoff(&self) -> WebSocket<MaybeTlsStream<TcpStream>> {
        let mut backoff = ExponentialBackoff::default();

        loop {
            match self.connect().await {
                Ok(ws_stream) => {
                    emit!(WebSocketConnectionEstablished {});
                    return ws_stream;
                }
                Err(error) => {
                    emit!(WebSocketConnectionFailedError {
                        error: Box::new(error)
                    });
                    time::sleep(backoff.next().unwrap()).await;
                }
            }
        }
    }

    /// Connects with exponential backoff, applying a timeout to each individual connection attempt.
    /// This will retry forever until a connection is established.
    #[cfg(feature = "sources-websocket")]
    pub(crate) async fn connect_backoff_with_timeout(
        &self,
        timeout_duration: Duration,
    ) -> WebSocket<MaybeTlsStream<TcpStream>> {
        let mut backoff = ExponentialBackoff::default();

        loop {
            match time::timeout(timeout_duration, self.connect()).await {
                Ok(Ok(ws_stream)) => {
                    emit!(WebSocketConnectionEstablished {});
                    return ws_stream;
                }
                Ok(Err(error)) => {
                    emit!(WebSocketConnectionFailedError {
                        error: Box::new(error)
                    });
                }
                Err(_) => {
                    emit!(WebSocketConnectionFailedError {
                        error: Box::new(WebSocketError::ConnectionTimedOut),
                    });
                }
            }

            time::sleep(
                backoff
                    .next()
                    .expect("backoff iterator always returns some value"),
            )
            .await;
        }
    }

    #[cfg(feature = "sinks-websocket")]
    pub(crate) async fn healthcheck(&self) -> crate::Result<()> {
        self.connect().await.map(|_| ()).map_err(Into::into)
    }
}

pub(crate) struct PingInterval {
    interval: Option<time::Interval>,
}

impl PingInterval {
    pub(crate) fn new(period: Option<u64>) -> Self {
        Self {
            interval: period.map(|period| time::interval(Duration::from_secs(period))),
        }
    }

    pub(crate) fn poll_tick(&mut self, cx: &mut Context<'_>) -> Poll<time::Instant> {
        match self.interval.as_mut() {
            Some(interval) => interval.poll_tick(cx),
            None => Poll::Pending,
        }
    }

    pub(crate) async fn tick(&mut self) -> time::Instant {
        std::future::poll_fn(|cx| self.poll_tick(cx)).await
    }
}

/// WebSocket compression configuration.
///
/// When enabled, negotiates RFC 7692 permessage-deflate compression
/// with the remote peer to reduce bandwidth usage.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct WebSocketCompression {
    /// Compression level (0-9). Higher values produce better compression at the cost of more CPU.
    ///
    /// Defaults to 6 (balanced).
    #[serde(default = "default_compression_level")]
    pub level: u32,
}

const fn default_compression_level() -> u32 {
    6
}

/// Shared websocket configuration for sources and sinks.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct WebSocketCommonConfig {
    /// The WebSocket URI to connect to.
    ///
    /// This should include the protocol and host, but can also include the port, path, and any other valid part of a URI.
    ///  **Note**: Using the `wss://` protocol requires enabling `tls`.
    #[configurable(metadata(docs::examples = "ws://localhost:8080"))]
    #[configurable(metadata(docs::examples = "wss://example.com/socket"))]
    pub uri: String,

    /// The interval, in seconds, between sending [Ping][ping]s to the remote peer.
    ///
    /// If this option is not configured, pings are not sent on an interval.
    ///
    /// If the `ping_timeout` is not set, pings are still sent but there is no expectation of pong
    /// response times.
    ///
    /// [ping]: https://www.rfc-editor.org/rfc/rfc6455#section-5.5.2
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[configurable(metadata(docs::advanced))]
    #[configurable(metadata(docs::examples = 30))]
    pub ping_interval: Option<NonZeroU64>,

    /// The number of seconds to wait for a [Pong][pong] response from the remote peer.
    ///
    /// If a response is not received within this time, the connection is re-established.
    ///
    /// [pong]: https://www.rfc-editor.org/rfc/rfc6455#section-5.5.3
    // NOTE: this option is not relevant if the `ping_interval` is not configured.
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[configurable(metadata(docs::advanced))]
    #[configurable(metadata(docs::examples = 5))]
    pub ping_timeout: Option<NonZeroU64>,

    /// TLS configuration.
    #[configurable(derived)]
    pub tls: Option<TlsEnableableConfig>,

    /// HTTP Authentication.
    #[configurable(derived)]
    pub auth: Option<Auth>,

    /// Compression configuration for WebSocket connections.
    ///
    /// When enabled, negotiates RFC 7692 permessage-deflate compression
    /// with the remote peer to reduce bandwidth usage.
    #[configurable(derived)]
    #[serde(default)]
    pub compression: Option<WebSocketCompression>,
}

impl Default for WebSocketCommonConfig {
    fn default() -> Self {
        Self {
            uri: "ws://127.0.0.1:8080".to_owned(),
            ping_interval: None,
            ping_timeout: None,
            tls: None,
            auth: None,
            compression: None,
        }
    }
}
