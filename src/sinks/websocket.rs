use crate::{
    buffers::Acker,
    config::{DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    dns, emit,
    event::Event,
    internal_events::{
        ConnectionOpen, OpenGauge, WsConnectionError, WsConnectionEstablished, WsConnectionFailed,
        WsConnectionShutdown, WsEventSent,
    },
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration},
        retries::ExponentialBackoff,
        StreamSink,
    },
    tls::{MaybeTlsSettings, MaybeTlsStream, TlsConfig, TlsError},
};
use async_trait::async_trait;
use futures::{
    future::{self},
    pin_mut,
    sink::SinkExt,
    stream::BoxStream,
    Sink, Stream, StreamExt,
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::{
    fmt::Debug,
    io,
    net::SocketAddr,
    task::{Context, Poll},
    time::{Duration, Instant},
};
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

#[derive(Debug, Snafu)]
enum WebSocketError {
    #[snafu(display("Creating WebSocket client failed: {}", source))]
    CreateFailed { source: WsError },
    #[snafu(display("Connect error: {}", source))]
    ConnectError { source: TlsError },
    #[snafu(display("Unable to resolve DNS: {}", source))]
    DnsError { source: dns::DnsError },
    #[snafu(display("No addresses returned."))]
    NoAddresses,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WebSocketSinkConfig {
    uri: String,
    tls: Option<TlsConfig>,
    encoding: EncodingConfig<Encoding>,
    ping_interval: Option<u64>,
    ping_timeout: Option<u64>,
}

#[derive(Clone, Copy, Debug, Derivative, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

inventory::submit! {
    SinkDescription::new::<WebSocketSinkConfig>("websocket")
}

impl GenerateConfig for WebSocketSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"uri = "ws://127.0.0.1:9000/endpoint"
            encoding.codec = "json""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "websocket")]
impl SinkConfig for WebSocketSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let connector = self.build_connector()?;
        let ws_sink = WebSocketSink::new(&self, connector.clone(), cx.acker());

        Ok((
            super::VectorSink::Stream(Box::new(ws_sink)),
            Box::pin(async move { connector.healthcheck().await }),
        ))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "websocket"
    }
}

impl WebSocketSinkConfig {
    fn build_connector(&self) -> Result<WebSocketConnector, WebSocketError> {
        let tls = MaybeTlsSettings::from_config(&self.tls, false).context(ConnectError)?;
        Ok(WebSocketConnector::new(self.uri.clone(), tls)?)
    }
}

#[derive(Clone)]
struct WebSocketConnector {
    uri: String,
    host: String,
    port: u16,
    tls: MaybeTlsSettings,
}

impl WebSocketConnector {
    fn new(uri: String, tls: MaybeTlsSettings) -> Result<Self, WebSocketError> {
        let request = (&uri).into_client_request().context(CreateFailed)?;
        let (host, port) = Self::extract_host_and_port(&request).context(CreateFailed)?;

        Ok(Self {
            uri,
            host,
            port,
            tls,
        })
    }

    fn extract_host_and_port(request: &WsRequest) -> Result<(String, u16), WsError> {
        let host = request
            .uri()
            .host()
            .ok_or(WsError::Url(UrlError::NoHostName))?
            .to_string();
        let mode = uri_mode(&request.uri())?;
        let port = request.uri().port_u16().unwrap_or_else(|| match mode {
            UriMode::Tls => 443,
            UriMode::Plain => 80,
        });

        Ok((host, port))
    }

    fn fresh_backoff() -> ExponentialBackoff {
        ExponentialBackoff::from_millis(2)
            .factor(250)
            .max_delay(Duration::from_secs(60))
    }

    async fn tls_connect(&self) -> Result<MaybeTlsStream<TcpStream>, WebSocketError> {
        let ip = dns::Resolver
            .lookup_ip(self.host.clone())
            .await
            .context(DnsError)?
            .next()
            .ok_or(WebSocketError::NoAddresses)?;

        let addr = SocketAddr::new(ip, self.port);
        self.tls
            .connect(&self.host, &addr)
            .await
            .context(ConnectError)
    }

    async fn connect(&self) -> Result<WsStream<MaybeTlsStream<TcpStream>>, WebSocketError> {
        let request = (&self.uri).into_client_request().context(CreateFailed)?;
        let maybe_tls = self.tls_connect().await?;

        let ws_config = WebSocketConfig {
            max_send_queue: None, // don't buffer messages
            ..Default::default()
        };

        let (ws_stream, _response) = client_async_with_config(request, maybe_tls, Some(ws_config))
            .await
            .context(CreateFailed)?;

        Ok(ws_stream)
    }

    async fn connect_backoff(&self) -> WsStream<MaybeTlsStream<TcpStream>> {
        let mut backoff = Self::fresh_backoff();
        loop {
            match self.connect().await {
                Ok(ws_stream) => {
                    emit!(&WsConnectionEstablished {});
                    return ws_stream;
                }
                Err(error) => {
                    emit!(&WsConnectionFailed { error });
                    time::sleep(backoff.next().unwrap()).await;
                }
            }
        }
    }

    async fn healthcheck(&self) -> crate::Result<()> {
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
        future::poll_fn(|cx| self.poll_tick(cx)).await
    }
}

pub struct WebSocketSink {
    encoding: EncodingConfig<Encoding>,
    connector: WebSocketConnector,
    acker: Acker,
    ping_interval: Option<u64>,
    ping_timeout: Option<u64>,
}

impl WebSocketSink {
    fn new(config: &WebSocketSinkConfig, connector: WebSocketConnector, acker: Acker) -> Self {
        Self {
            encoding: config.encoding.clone(),
            connector,
            acker,
            ping_interval: config.ping_interval.filter(|v| *v > 0),
            ping_timeout: config.ping_timeout.filter(|v| *v > 0),
        }
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

    fn check_received_pong_time(&self, last_pong: &Instant) -> Result<(), WsError> {
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
        &self,
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
            emit!(&WsConnectionError { error });
            return Err(());
        }
        let mut last_pong = Instant::now();

        loop {
            let result = tokio::select! {
                _ = ping_interval.tick() => {
                    match self.check_received_pong_time(&last_pong) {
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
                    if event.is_none() {
                        break;
                    }
                    let log = encode_event(event.unwrap(), &self.encoding);
                    let res = match log {
                        Some(msg) => {
                            let msg_len = msg.len();
                            ws_sink.send(msg).await.map(|_| {
                                emit!(&WsEventSent { byte_size: msg_len });
                            })
                        },
                        None => {
                            Ok(())
                        }
                    };
                    self.acker.ack(1);
                    res
                },
                else => break,
            };

            if let Err(error) = result {
                if is_closed(&error) {
                    emit!(&WsConnectionShutdown);
                } else {
                    emit!(&WsConnectionError { error });
                }
                return Err(());
            }
        }

        Ok(())
    }
}

#[async_trait]
impl StreamSink for WebSocketSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let input = input.fuse().peekable();
        pin_mut!(input);

        while input.as_mut().peek().await.is_some() {
            let (ws_sink, ws_stream) = self.create_sink_and_stream().await;
            pin_mut!(ws_sink);
            pin_mut!(ws_stream);

            let _open_token = OpenGauge::new().open(|count| emit!(&ConnectionOpen { count }));

            if self
                .handle_events(&mut input, &mut ws_stream, &mut ws_sink)
                .await
                .is_ok()
            {
                let _ = ws_sink.close().await;
            }
        }

        Ok(())
    }
}

fn is_closed(error: &WsError) -> bool {
    match error {
        WsError::ConnectionClosed | WsError::AlreadyClosed => true,
        WsError::Protocol(ProtocolError::ResetWithoutClosingHandshake) => true,
        _ => false,
    }
}

fn encode_event(mut event: Event, encoding: &EncodingConfig<Encoding>) -> Option<Message> {
    encoding.apply_rules(&mut event);

    let msg = match encoding.codec() {
        Encoding::Json => serde_json::to_string(event.as_log())
            .map_err(|error| error!(message = "Unable to encode.", %error))
            .ok(),
        Encoding::Text => event
            .as_log()
            .get(crate::config::log_schema().message_key())
            .map(|v| v.to_string_lossy()),
    };

    msg.map(|msg| Message::text(msg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::SinkContext,
        event::{Event, Value as EventValue},
        test_util::{next_addr, random_lines_with_stream, trace_init, CountReceiver},
        tls::{self, TlsOptions},
    };
    use futures::{future, FutureExt, StreamExt};
    use serde_json::Value as JsonValue;
    use std::net::SocketAddr;
    use tokio::time::timeout;
    use tokio_tungstenite::{
        accept_async,
        tungstenite::{
            error::{Error as WsError, ProtocolError},
            Message,
        },
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<WebSocketSinkConfig>();
    }

    #[test]
    fn encodes_raw_logs() {
        let event = Event::from("foo");
        assert_eq!(
            Message::text("foo"),
            encode_event(event, &EncodingConfig::from(Encoding::Text)).unwrap()
        );
    }

    #[test]
    fn encodes_log_events() {
        let mut event = Event::new_empty_log();

        let log = event.as_mut_log();
        log.insert("str", EventValue::from("bar"));
        log.insert("num", EventValue::from(10));

        let encoded = encode_event(event, &EncodingConfig::from(Encoding::Json));
        let expected = Message::text(r#"{"num":10,"str":"bar"}"#);
        assert_eq!(expected, encoded.unwrap());
    }

    #[tokio::test]
    async fn test_websocket() {
        trace_init();

        let addr = next_addr();
        let config = WebSocketSinkConfig {
            uri: format!("ws://{}", addr.to_string()),
            tls: None,
            encoding: Encoding::Json.into(),
            ping_interval: None,
            ping_timeout: None,
        };
        let tls = MaybeTlsSettings::Raw(());

        send_events_and_assert(addr, config, tls).await;
    }

    #[cfg(feature = "sources-utils-tls")]
    #[tokio::test]
    async fn test_tls_websocket() {
        trace_init();

        let addr = next_addr();
        let tls_config = Some(TlsConfig::test_config());
        let tls = MaybeTlsSettings::from_config(&tls_config, true).unwrap();

        let config = WebSocketSinkConfig {
            uri: format!("wss://{}", addr.to_string()),
            tls: Some(TlsConfig {
                enabled: Some(true),
                options: TlsOptions {
                    verify_certificate: Some(false),
                    verify_hostname: Some(true),
                    ca_file: Some(tls::TEST_PEM_CRT_PATH.into()),
                    ..Default::default()
                },
            }),
            encoding: Encoding::Json.into(),
            ping_timeout: None,
            ping_interval: None,
        };

        send_events_and_assert(addr, config, tls).await;
    }

    #[tokio::test]
    async fn test_websocket_reconnect() {
        trace_init();

        let addr = next_addr();
        let config = WebSocketSinkConfig {
            uri: format!("ws://{}", addr.to_string()),
            tls: None,
            encoding: Encoding::Json.into(),
            ping_interval: None,
            ping_timeout: None,
        };
        let tls = MaybeTlsSettings::Raw(());

        let mut receiver = create_count_receiver(addr.clone(), tls.clone(), true);

        let context = SinkContext::new_test();
        let (sink, _healthcheck) = config.build(context).await.unwrap();

        let (_lines, events) = random_lines_with_stream(10, 100, None);
        let events = events.then(|event| async move {
            time::sleep(Duration::from_millis(10)).await;
            event
        });
        let _ = tokio::spawn(sink.run(events));

        receiver.connected().await;
        time::sleep(Duration::from_millis(500)).await;
        assert!(!receiver.await.is_empty());

        let mut receiver = create_count_receiver(addr, tls, false);
        assert!(timeout(Duration::from_secs(10), receiver.connected())
            .await
            .is_ok());
    }

    async fn send_events_and_assert(
        addr: SocketAddr,
        config: WebSocketSinkConfig,
        tls: MaybeTlsSettings,
    ) {
        let mut receiver = create_count_receiver(addr, tls, false);

        let context = SinkContext::new_test();
        let (sink, _healthcheck) = config.build(context).await.unwrap();

        let (lines, events) = random_lines_with_stream(10, 100, None);
        sink.run(events).await.unwrap();

        receiver.connected().await;

        let output = receiver.await;
        assert_eq!(lines.len(), output.len());
        let message_key = crate::config::log_schema().message_key();
        for (source, received) in lines.iter().zip(output) {
            let json = serde_json::from_str::<JsonValue>(&received).expect("Invalid JSON");
            let received = json.get(message_key).unwrap().as_str().unwrap();
            assert_eq!(source, received);
        }
    }

    fn create_count_receiver(
        addr: SocketAddr,
        tls: MaybeTlsSettings,
        interrupt_stream: bool,
    ) -> CountReceiver<String> {
        CountReceiver::receive_items_stream(move |tripwire, connected| async move {
            let listener = tls.bind(&addr).await.unwrap();
            let stream = listener.accept_stream();

            let tripwire = tripwire.map(|_| ()).shared();
            let stream_tripwire = tripwire.clone();
            let mut connected = Some(connected);

            let stream = stream
                .take_until(tripwire)
                .filter_map(|maybe_tls_stream| async move {
                    let maybe_tls_stream = maybe_tls_stream.unwrap();
                    let ws_stream = accept_async(maybe_tls_stream).await.unwrap();

                    Some(
                        ws_stream
                            .filter_map(|msg| {
                                future::ready(match msg {
                                    Ok(msg) if msg.is_text() => Some(Ok(msg.into_text().unwrap())),
                                    Err(WsError::Protocol(
                                        ProtocolError::ResetWithoutClosingHandshake,
                                    )) => None,
                                    Err(e) => Some(Err(e)),
                                    _ => None,
                                })
                            })
                            .take_while(|msg| future::ready(msg.is_ok()))
                            .filter_map(|msg| future::ready(msg.ok())),
                    )
                })
                .map(move |ws_stream| {
                    connected.take().map(|trigger| trigger.send(()));
                    ws_stream
                })
                .flatten();

            match interrupt_stream {
                false => stream.boxed(),
                true => stream.take_until(stream_tripwire).boxed(),
            }
        })
    }
}
