use std::{fmt::Debug, net::SocketAddr, time::Duration};

use snafu::{ResultExt, Snafu};
use tokio::{net::TcpStream, time};
use tokio_tungstenite::{
    client_async_with_config,
    tungstenite::{
        client::{uri_mode, IntoClientRequest},
        error::{Error as WsError, ProtocolError, UrlError},
        handshake::client::Request as WsRequest,
        protocol::WebSocketConfig,
        stream::Mode as UriMode,
    },
    WebSocketStream as WsStream,
};

use crate::{
    common::backoff::ExponentialBackoff,
    dns,
    http::Auth,
    internal_events::{WsConnectionEstablished, WsConnectionFailedError},
    tls::{MaybeTlsSettings, MaybeTlsStream, TlsError},
};

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum WebSocketError {
    #[snafu(display("Creating WebSocket client failed: {}", source))]
    CreateFailed { source: WsError },
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

    fn extract_host_and_port(request: &WsRequest) -> Result<(String, u16), WsError> {
        let host = request
            .uri()
            .host()
            .ok_or(WsError::Url(UrlError::NoHostName))?
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

    async fn connect(&self) -> Result<WsStream<MaybeTlsStream<TcpStream>>, WebSocketError> {
        let mut request = (&self.uri)
            .into_client_request()
            .context(CreateFailedSnafu)?;

        if let Some(auth) = &self.auth {
            auth.apply(&mut request);
        }

        let maybe_tls = self.tls_connect().await?;

        let ws_config = WebSocketConfig {
            max_send_queue: None, // don't buffer messages
            ..Default::default()
        };

        let (ws_stream, _response) = client_async_with_config(request, maybe_tls, Some(ws_config))
            .await
            .context(CreateFailedSnafu)?;

        Ok(ws_stream)
    }

    pub(crate) async fn connect_backoff(&self) -> WsStream<MaybeTlsStream<TcpStream>> {
        let mut backoff = Self::fresh_backoff();
        loop {
            match self.connect().await {
                Ok(ws_stream) => {
                    emit!(WsConnectionEstablished {});
                    return ws_stream;
                }
                Err(error) => {
                    emit!(WsConnectionFailedError {
                        error: Box::new(error)
                    });
                    time::sleep(backoff.next().unwrap()).await;
                }
            }
        }
    }

    pub(crate) async fn healthcheck(&self) -> crate::Result<()> {
        self.connect().await.map(|_| ()).map_err(Into::into)
    }
}

pub(crate) const fn is_closed(error: &WsError) -> bool {
    matches!(
        error,
        WsError::ConnectionClosed
            | WsError::AlreadyClosed
            | WsError::Protocol(ProtocolError::ResetWithoutClosingHandshake)
    )
}
