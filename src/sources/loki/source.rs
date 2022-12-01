use std::{
    fmt::Debug,
    io,
    net::SocketAddr,
    task::{Context, Poll},
    time::{Duration, Instant},
};

use futures::{sink::SinkExt, Sink, Stream, StreamExt};
use snafu::{ResultExt, Snafu};
use tokio::{net::TcpStream, time};
use tokio_tungstenite::{
    client_async_with_config,
    tungstenite::{
        client::{uri_mode, IntoClientRequest},
        error::{Error as WsError, ProtocolError, UrlError},
        handshake::client::Request as WsRequest,
        protocol::{Message, WebSocketConfig},
        stream::Mode as UriMode,
    },
    WebSocketStream as WsStream,
};
use vector_common::shutdown::ShutdownSignal;
use vector_core::event::{Event, LogEvent};
use vector_core::internal_event::{BytesSent, Protocol};

use crate::internal_events::StreamClosedError;
use crate::sinks::util::retries::ExponentialBackoff;
use crate::sources::loki::config::LokiSourceConfig;
use crate::{
    dns, emit,
    http::Auth,
    internal_events::{
        WsConnectionError, WsConnectionEstablished, WsConnectionFailedError, WsConnectionShutdown,
    },
    tls::{MaybeTlsSettings, MaybeTlsStream, TlsError},
    SourceSender,
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
pub struct WebSocketConnector {
    uri: String,
    host: String,
    port: u16,
    tls: MaybeTlsSettings,
    auth: Option<Auth>,
}

impl WebSocketConnector {
    pub fn new(
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

    async fn connect_backoff(&self) -> WsStream<MaybeTlsStream<TcpStream>> {
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

    // pub async fn healthcheck(&self) -> crate::Result<()> {
    //     self.connect().await.map(|_| ()).map_err(Into::into)
    // }
}

struct PingInterval {
    interval: Option<time::Interval>,
}

impl PingInterval {
    fn new(period: Option<u64>) -> Self {
        Self {
            interval: period.map(|period| time::interval(Duration::from_secs(period))),
        }
    }

    fn poll_tick(&mut self, cx: &mut Context<'_>) -> Poll<time::Instant> {
        match self.interval.as_mut() {
            Some(interval) => interval.poll_tick(cx),
            None => Poll::Pending,
        }
    }

    async fn tick(&mut self) -> time::Instant {
        std::future::poll_fn(|cx| self.poll_tick(cx)).await
    }
}

const fn is_closed(error: &WsError) -> bool {
    matches!(
        error,
        WsError::ConnectionClosed
            | WsError::AlreadyClosed
            | WsError::Protocol(ProtocolError::ResetWithoutClosingHandshake)
    )
}

fn build_connector(config: &LokiSourceConfig) -> Result<WebSocketConnector, WebSocketError> {
    let tls = MaybeTlsSettings::from_config(&config.tls, false).context(ConnectSnafu)?;
    WebSocketConnector::new(config.endpoint.clone(), tls, config.auth.clone())
}

fn check_received_pong_time(last_pong: Instant) -> Result<(), WsError> {
    // TODO: make configurable
    //if let Some(ping_timeout) = self.ping_timeout {
    if last_pong.elapsed() > Duration::from_secs(60) {
        return Err(WsError::Io(io::Error::new(
            io::ErrorKind::TimedOut,
            "Pong not received in time",
        )));
    }
    //}

    Ok(())
}

async fn handle_events<WS, O>(
    ws_stream: &mut WS,
    ws_sink: &mut O,
    mut shutdown: ShutdownSignal,
    mut out: SourceSender,
) -> Result<(), ()>
where
    WS: Stream<Item = Result<Message, WsError>> + Unpin,
    O: Sink<Message, Error = WsError> + Unpin,
{
    const PING: &[u8] = b"PING";

    //TODO: make ping interval configurable
    let mut ping_interval = PingInterval::new(Some(10u64));

    if let Err(error) = ws_sink.send(Message::Ping(PING.to_vec())).await {
        emit!(WsConnectionError { error });
        return Err(());
    }
    let mut last_pong = Instant::now();

    let _bytes_sent = register!(BytesSent::from(Protocol("websocket".into())));

    loop {
        let result = tokio::select! {
            _ = &mut shutdown => break,
            _ = ping_interval.tick() => {
                match check_received_pong_time(last_pong) {
                    Ok(()) => ws_sink.send(Message::Ping(PING.to_vec())).await.map(|_| ()),
                    Err(e) => Err(e)
                }
            },

            Some(msg) = ws_stream.next() => {
                // Pongs are sent automatically by tungstenite during reading from the stream.
                match msg {
                    Ok(Message::Pong(_)) => {
                        last_pong = Instant::now();
                        Ok(())
                    },
                    Ok(Message::Text(text)) => {
                        //TODO: rewrite it
                        //println!("start message");
                        //println!("{}", text);
                        //println!("end message");
                        let count = 1;
                        out.send_event(Event::Log(LogEvent::from_str_legacy(text))).await.map_err(|error| {
                            emit!(StreamClosedError { error, count });
                        })?;
                        Ok(())
                    },
                    Ok(_) => Ok(()),
                    Err(e) => Err(e)
                }
            },
        else => break,
        };

        if let Err(error) = result {
            if is_closed(&error) {
                emit!(WsConnectionShutdown);
            } else {
                emit!(WsConnectionError { error });
            }
            return Err(());
        }
    }

    Ok(())
}

pub async fn loki_source(
    config: LokiSourceConfig,
    shutdown: ShutdownSignal,
    out: SourceSender,
) -> Result<(), ()> {
    //TODO: rewrite to proper error handling
    let connector = build_connector(&config).expect("Cannot build WS connector");

    let ws_stream = connector.connect_backoff().await;
    let (mut ws_sink, mut ws_stream) = ws_stream.split();

    if handle_events(&mut ws_stream, &mut ws_sink, shutdown, out)
        .await
        .is_ok()
    {
        let _ = ws_sink.close().await;
    }

    Ok(())
}
