use std::{
    fmt::Debug,
    net::SocketAddr,
    num::NonZeroU64,
    task::{Context, Poll},
    time::Duration,
};
use vector_config_macros::configurable_component;

use snafu::{ResultExt, Snafu};
use tokio::{net::TcpStream, time};
use tokio_tungstenite::{
    client_async_with_config,
    tungstenite::{
        client::{uri_mode, IntoClientRequest},
        error::{Error as TungsteniteError, ProtocolError, UrlError},
        handshake::client::Request,
        protocol::WebSocketConfig,
        stream::Mode as UriMode,
    },
    WebSocketStream,
};

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
    CreateFailed { source: TungsteniteError },
    #[snafu(display("Connect error: {}", source))]
    ConnectError { source: TlsError },
    #[snafu(display("Unable to resolve DNS: {}", source))]
    DnsError { source: dns::DnsError },
    #[snafu(display("No addresses returned."))]
    NoAddresses,
}

#[derive(Clone)]
pub(crate) struct WebSocketConnector {
    uri: String,
    host: String,
    port: u16,
    tls: MaybeTlsSettings,
    auth: Option<Auth>,
}

impl WebSocketConnector {
    pub(crate) fn new(
        uri: String,
        tls: MaybeTlsSettings,
        auth: Option<Auth>,
    ) -> Result<Self, WebSocketError> {
        let request = (&uri).into_client_request().context(CreateFailedSnafu)?;
        let (host, port) = Self::extract_host_and_port(&request).context(CreateFailedSnafu)?;

        Ok(Self {
            uri,
            host,
            port,
            tls,
            auth,
        })
    }

    fn extract_host_and_port(request: &Request) -> Result<(String, u16), TungsteniteError> {
        let host = request
            .uri()
            .host()
            .ok_or(TungsteniteError::Url(UrlError::NoHostName))?
            .to_string();
        let mode = uri_mode(request.uri())?;
        let port = request.uri().port_u16().unwrap_or(match mode {
            UriMode::Tls => 443,
            UriMode::Plain => 80,
        });

        Ok((host, port))
    }

    const fn fresh_backoff() -> ExponentialBackoff {
        ExponentialBackoff::from_millis(2)
            .factor(250)
            .max_delay(Duration::from_secs(60))
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

    async fn connect(&self) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, WebSocketError> {
        let mut request = (&self.uri)
            .into_client_request()
            .context(CreateFailedSnafu)?;

        if let Some(auth) = &self.auth {
            auth.apply(&mut request);
        }

        let maybe_tls = self.tls_connect().await?;

        let ws_config = WebSocketConfig::default();

        let (ws_stream, _response) = client_async_with_config(request, maybe_tls, Some(ws_config))
            .await
            .context(CreateFailedSnafu)?;

        Ok(ws_stream)
    }

    pub(crate) async fn connect_backoff(&self) -> WebSocketStream<MaybeTlsStream<TcpStream>> {
        let mut backoff = Self::fresh_backoff();
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

    #[cfg(feature = "sinks-websocket")]
    pub(crate) async fn healthcheck(&self) -> crate::Result<()> {
        self.connect().await.map(|_| ()).map_err(Into::into)
    }
}

pub(crate) const fn is_closed(error: &TungsteniteError) -> bool {
    matches!(
        error,
        TungsteniteError::ConnectionClosed
            | TungsteniteError::AlreadyClosed
            | TungsteniteError::Protocol(ProtocolError::ResetWithoutClosingHandshake)
    )
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
}

impl Default for WebSocketCommonConfig {
    fn default() -> Self {
        Self {
            uri: "ws://127.0.0.1:8080".to_owned(),
            ping_interval: None,
            ping_timeout: None,
            tls: None,
            auth: None,
        }
    }
}
