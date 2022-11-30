use std::{
    fmt::Debug,
    io,
    net::SocketAddr,
    task::{Context, Poll},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use bytes::BytesMut;
use futures::{pin_mut, sink::SinkExt, stream::BoxStream, Sink, Stream, StreamExt};
use snafu::{ResultExt, Snafu};
use tokio::{net::TcpStream, time};
use tokio_stream::StreamExt;
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
use tokio_util::codec::Encoder as _;
use vector_common::shutdown::ShutdownSignal;
use vector_core::{
    internal_event::{ByteSize, BytesSent, EventsSent, InternalEventHandle as _, Protocol},
    ByteSizeOf,
};

use crate::{codecs::{Encoder, Transformer}, dns, emit, event::{Event, EventStatus, Finalizable}, http::Auth, internal_events::{
    ConnectionOpen, OpenGauge, WsConnectionError, WsConnectionEstablished,
    WsConnectionFailedError, WsConnectionShutdown,
}, SourceSender, tls::{MaybeTlsSettings, MaybeTlsStream, TlsError}};
use crate::http::HttpClient;
use crate::sources::loki::config::LokiSourceConfig;

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

    pub async fn healthcheck(&self) -> crate::Result<()> {
        self.connect().await.map(|_| ()).map_err(Into::into)
    }
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

pub struct LokiSource {
    connector: WebSocketConnector,
    ping_interval: Option<u64>,
    ping_timeout: Option<u64>,
}

impl LokiSource {
    pub fn new(config: &LokiSourceConfig, connector: WebSocketConnector) -> crate::Result<Self> {
        Ok(Self {
            connector,
            ping_interval: config.ping_interval.filter(|v| *v > 0),
            ping_timeout: config.ping_timeout.filter(|v| *v > 0),
        })
    }

    async fn create_sink_and_stream(
        &self,
    ) -> (
        impl Sink<Message, Error = WsError>,
        impl Stream<Item = Result<Message, WsError>>,
    ) {
        let ws_stream = self.connector.connect_backoff().await;
        ws_stream.split()
    }

    fn check_received_pong_time(&self, last_pong: Instant) -> Result<(), WsError> {
        if let Some(ping_timeout) = self.ping_timeout {
            if last_pong.elapsed() > Duration::from_secs(ping_timeout) {
                return Err(WsError::Io(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Pong not received in time",
                )));
            }
        }

        Ok(())
    }

    async fn handle_events<I, WS, O>(
        &mut self,
        input: &mut I,
        ws_stream: &mut WS,
        ws_sink: &mut O,
    ) -> Result<(), ()>
        where
            I: Stream<Item = Event> + Unpin,
            WS: Stream<Item = Result<Message, WsError>> + Unpin,
            O: Sink<Message, Error = WsError> + Unpin,
    {
        const PING: &[u8] = b"PING";

        let mut ping_interval = PingInterval::new(self.ping_interval);

        if let Err(error) = ws_sink.send(Message::Ping(PING.to_vec())).await {
            emit!(WsConnectionError { error });
            return Err(());
        }
        let mut last_pong = Instant::now();

        let bytes_sent = register!(BytesSent::from(Protocol("websocket".into())));

        loop {
            let result = tokio::select! {
                _ = ping_interval.tick() => {
                    match self.check_received_pong_time(last_pong) {
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
                        Ok(_) => Ok(()),
                        Err(e) => Err(e)
                    }
                },

                event = input.next() => {
                    let mut event = if let Some(event) = event {
                        event
                    } else {
                        break;
                    };

                    let finalizers = event.take_finalizers();

                    self.transformer.transform(&mut event);

                    let event_byte_size = event.size_of();

                    let mut bytes = BytesMut::new();
                    let res = match self.encoder.encode(event, &mut bytes) {
                        Ok(()) => {
                            finalizers.update_status(EventStatus::Delivered);

                            let message = Message::text(String::from_utf8_lossy(&bytes));
                            let message_len = message.len();

                            ws_sink.send(message).await.map(|_| {
                                emit!(EventsSent {
                                    count: 1,
                                    byte_size: event_byte_size,
                                    output: None
                                });
                                bytes_sent.emit(ByteSize(message_len));
                            })
                        },
                        Err(_) => {
                            // Error is handled by `Encoder`.
                            finalizers.update_status(EventStatus::Errored);
                            Ok(())
                        }
                    };

                    res
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
}

fn build_connector(config: &LokiSourceConfig) -> Result<WebSocketConnector, WebSocketError> {
    let tls = MaybeTlsSettings::from_config(&config.tls, false).context(ConnectSnafu)?;
    WebSocketConnector::new(config.uri.clone(), tls, config.auth.clone())
}

pub async fn loki_source(
    config: &LokiSourceConfig,
    http_client: HttpClient,
    shutdown: ShutdownSignal,
    mut out: SourceSender,
) -> Result<(), ()> {
    let connector = build_connector(config)?;

    let ws_stream = connector.connect_backoff().await;
    //ws_stream.split()
    ws_stream
    Ok(())
}
