use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use super::{
    WebSocketListenerSinkConfig,
    buffering::MessageBufferingConfig,
    config::{ExtraMetricTagsConfig, SubProtocolConfig, ValidatedWebSocketListenerSink},
};
use crate::{
    codecs::{Encoder, Transformer},
    common::http::server_auth::HttpServerAuthMatcher,
    internal_events::{
        ConnectionOpen, OpenGauge, WebSocketListenerConnectionEstablished,
        WebSocketListenerConnectionFailedError, WebSocketListenerConnectionShutdown,
        WebSocketListenerMessageSent, WebSocketListenerSendError,
    },
    sinks::{
        prelude::*,
        websocket_server::buffering::{BufferReplayRequest, WsMessageBufferConfig},
    },
};

#[cfg(test)]
use crate::config::ValidatedSink;
use async_trait::async_trait;
use bytes::BytesMut;
use futures::{
    StreamExt, TryStreamExt,
    channel::mpsc::{UnboundedSender, unbounded},
    future, pin_mut,
    stream::{self, BoxStream},
};
use http::StatusCode;
use stream_cancel::Tripwire;
use tokio::{
    net::TcpStream,
    task::{AbortHandle, JoinSet},
    time,
};
use tokio_tungstenite::tungstenite::{
    Message,
    handshake::server::{ErrorResponse, Request, Response},
    protocol::frame::{CloseFrame, coding::CloseCode},
};
use tokio_util::codec::Encoder as _;
use tracing::Instrument;
use url::Url;
use uuid::Uuid;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    event::{Event, EventStatus},
    finalization::Finalizable,
    internal_event::{
        ByteSize, BytesSent, CountByteSize, EventsSent, InternalEventHandle, Output, Protocol,
    },
    sink::StreamSink,
    tls::{MaybeTlsIncomingStream, MaybeTlsListener, MaybeTlsSettings},
};

const CONNECTION_SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(5);

struct AbortOnDrop(AbortHandle);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        // Aborting an already completed task is harmless, so no separate completion state is needed.
        self.0.abort();
    }
}

struct PeerRegistration {
    addr: SocketAddr,
    peers: Arc<Mutex<HashMap<SocketAddr, UnboundedSender<Message>>>>,
    extra_tags: Vec<(String, String)>,
}

impl PeerRegistration {
    fn stop_sending(&self) {
        if let Some(sender) = self.peers.lock().expect("mutex poisoned").get(&self.addr) {
            // Keep this connection counted until its task has actually finished.
            sender.close_channel();
        }
    }
}

impl Drop for PeerRegistration {
    fn drop(&mut self) {
        let mut peers = self.peers.lock().expect("mutex poisoned");
        peers.remove(&self.addr);
        debug!(message = "WebSocket client disconnected.", address = %self.addr);
        emit!(WebSocketListenerConnectionShutdown {
            client_count: peers.len(),
            extra_tags: self.extra_tags.clone()
        });
    }
}

pub struct WebSocketListenerSink {
    tls: MaybeTlsSettings,
    transformer: Transformer,
    encoder: Encoder<()>,
    address: SocketAddr,
    auth: Option<HttpServerAuthMatcher>,
    extra_tags_config: HashMap<String, ExtraMetricTagsConfig>,
    message_buffering: Option<MessageBufferingConfig>,
    subprotocol: SubProtocolConfig,
}

impl WebSocketListenerSink {
    #[cfg(test)]
    pub fn new(config: WebSocketListenerSinkConfig, cx: SinkContext) -> crate::Result<Self> {
        let validated = config.validate()?;
        let tls = MaybeTlsSettings::from_config(config.tls.as_ref(), true)?;
        Self::from_validated(config, &validated, tls, cx)
    }

    /// Constructs the sink from the validated state, performing only the
    /// context-dependent work: building the auth matcher from the enrichment
    /// tables / metrics storage.
    pub(crate) fn from_validated(
        config: WebSocketListenerSinkConfig,
        validated: &ValidatedWebSocketListenerSink,
        tls: MaybeTlsSettings,
        cx: SinkContext,
    ) -> crate::Result<Self> {
        let auth = config
            .auth
            .map(|config| config.build(&cx.enrichment_tables, &cx.metrics_storage))
            .transpose()?;
        let serializer = config.encoding.build()?;
        let encoder = Encoder::<()>::new(serializer);

        Ok(Self {
            tls,
            address: config.address,
            transformer: validated.transformer.clone(),
            encoder,
            auth,
            extra_tags_config: config.internal_metrics.extra_tags,
            message_buffering: config.message_buffering,
            subprotocol: config.subprotocol,
        })
    }

    fn extract_extra_tags(
        extra_tags_config: &HashMap<String, ExtraMetricTagsConfig>,
        base_url: Option<&Url>,
        req: &Request,
    ) -> Vec<(String, String)> {
        extra_tags_config
            .iter()
            .filter_map(|(key, value)| match value {
                ExtraMetricTagsConfig::Header { name } => req
                    .headers()
                    .get(name)
                    .and_then(|h| h.to_str().ok())
                    .map(ToString::to_string)
                    .map(|header| (key.clone(), header)),
                ExtraMetricTagsConfig::Url => Some((key.clone(), req.uri().to_string())),
                ExtraMetricTagsConfig::Query { name } => Url::options()
                    .base_url(base_url)
                    .parse(req.uri().to_string().as_str())
                    .ok()
                    .and_then(|url| {
                        url.query_pairs()
                            .find(|(k, _)| k == name)
                            .map(|(_, value)| value.to_string())
                    })
                    .map(|value| (key.clone(), value)),
                _ => None,
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_connections(
        auth: Option<HttpServerAuthMatcher>,
        message_buffering: Option<MessageBufferingConfig>,
        subprotocol: SubProtocolConfig,
        peers: Arc<Mutex<HashMap<SocketAddr, UnboundedSender<Message>>>>,
        extra_tags_config: HashMap<String, ExtraMetricTagsConfig>,
        client_checkpoints: Arc<Mutex<HashMap<String, Uuid>>>,
        buffer: Arc<Mutex<VecDeque<(Uuid, Message)>>>,
        listener: MaybeTlsListener,
        shutdown: Tripwire,
    ) {
        let open_gauge = OpenGauge::new();
        let (client_shutdown_trigger, client_shutdown) = Tripwire::new();
        let mut connections = JoinSet::new();
        let mut listener = Some(listener);
        pin_mut!(shutdown);

        loop {
            let stream = tokio::select! {
                biased;

                _ = shutdown.as_mut() => break,
                result = connections.join_next(), if !connections.is_empty() => {
                    if let Some(result) = result {
                        Self::handle_connection_task_result(result);
                    }
                    continue;
                }
                accepted = async {
                    match listener.as_mut() {
                        Some(listener) => listener.accept().await,
                        None => future::pending().await,
                    }
                } => match accepted {
                    Ok(stream) => stream,
                    Err(error) => {
                        error!(
                            message = "WebSocket listener failed to accept a connection.",
                            %error
                        );
                        // Preserve existing behavior: stop accepting, but keep serving existing
                        // clients until the sink shuts down rather than failing the whole topology.
                        listener = None;
                        continue;
                    }
                },
            };
            let connection = Self::handle_connection(
                auth.clone(),
                message_buffering.clone(),
                subprotocol.clone(),
                Arc::clone(&peers),
                Arc::clone(&client_checkpoints),
                Arc::clone(&buffer),
                stream,
                extra_tags_config.clone(),
                open_gauge.clone(),
                client_shutdown.clone(),
            )
            .in_current_span();
            connections.spawn(connection);
        }

        // Stop accepting before asking existing clients to close so a new connection cannot race
        // with shutdown and escape the connection task set.
        drop(listener);
        client_shutdown_trigger.cancel();
        Self::shutdown_connections(&mut connections).await;
    }

    fn handle_connection_task_result(result: Result<Result<(), ()>, tokio::task::JoinError>) {
        if let Err(error) = result
            && !error.is_cancelled()
        {
            error!(message = "WebSocket connection task failed.", %error);
        }
    }

    async fn shutdown_connections(connections: &mut JoinSet<Result<(), ()>>) {
        let graceful_shutdown = async {
            while let Some(result) = connections.join_next().await {
                Self::handle_connection_task_result(result);
            }
        };

        if time::timeout(CONNECTION_SHUTDOWN_GRACE_PERIOD, graceful_shutdown)
            .await
            .is_err()
        {
            warn!(
                message = "Timed out waiting for WebSocket connections to close.",
                timeout_secs = CONNECTION_SHUTDOWN_GRACE_PERIOD.as_secs()
            );
            connections.abort_all();
            while let Some(result) = connections.join_next().await {
                Self::handle_connection_task_result(result);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_connection(
        auth: Option<HttpServerAuthMatcher>,
        message_buffering: Option<MessageBufferingConfig>,
        subprotocol: SubProtocolConfig,
        peers: Arc<Mutex<HashMap<SocketAddr, UnboundedSender<Message>>>>,
        client_checkpoints: Arc<Mutex<HashMap<String, Uuid>>>,
        buffer: Arc<Mutex<VecDeque<(Uuid, Message)>>>,
        stream: MaybeTlsIncomingStream<TcpStream>,
        extra_tags_config: HashMap<String, ExtraMetricTagsConfig>,
        open_gauge: OpenGauge,
        shutdown: Tripwire,
    ) -> Result<(), ()> {
        // Base url for parsing request URLs that may be relative
        let base_url = Url::parse("ws://localhost").ok();
        let addr = stream.peer_addr();
        debug!("Incoming TCP connection from: {addr}");

        let mut extra_tags: Vec<(String, String)> = extra_tags_config
            .iter()
            .filter_map(|(key, value)| match value {
                ExtraMetricTagsConfig::Fixed { value } => Some((key.clone(), value.clone())),
                ExtraMetricTagsConfig::IpAddress { with_port } => {
                    let tag_value = if *with_port {
                        addr.to_string()
                    } else {
                        addr.ip().to_string()
                    };
                    Some((key.clone(), tag_value))
                }
                _ => None,
            })
            .collect();
        let mut buffer_replay = BufferReplayRequest::NO_REPLAY;
        let mut client_checkpoint_key = None;

        let header_callback = |req: &Request, mut response: Response| {
            client_checkpoint_key = message_buffering.client_key(req, &addr);
            buffer_replay = message_buffering.extract_message_replay_request(
                req,
                client_checkpoint_key.clone().and_then(|key| {
                    client_checkpoints
                        .lock()
                        .expect("mutex poisoned")
                        .get(&key)
                        .cloned()
                }),
            );
            let Some(auth) = auth else {
                extra_tags.append(&mut Self::extract_extra_tags(
                    &extra_tags_config,
                    base_url.as_ref(),
                    req,
                ));
                return Ok(response);
            };
            match auth.handle_auth(Some(&addr), req.headers(), req.uri().path()) {
                Ok(_) => {
                    extra_tags.append(&mut Self::extract_extra_tags(
                        &extra_tags_config,
                        base_url.as_ref(),
                        req,
                    ));
                    match subprotocol {
                        SubProtocolConfig::Any => {
                            if let Some(websocket_protocol) =
                                req.headers().get("Sec-WebSocket-Protocol")
                            {
                                response
                                    .headers_mut()
                                    .insert("Sec-WebSocket-Protocol", websocket_protocol.clone());
                            }
                        }
                        SubProtocolConfig::Specific {
                            supported_subprotocols,
                        } => {
                            let requested_protocols =
                                req.headers().get_all("Sec-WebSocket-Protocol");
                            if let Some(matched_protocol) =
                                requested_protocols.iter().find(|requested_protocol| {
                                    supported_subprotocols.iter().any(|supported_protocol| {
                                        requested_protocol.as_bytes()
                                            == supported_protocol.as_bytes()
                                    })
                                })
                            {
                                response
                                    .headers_mut()
                                    .insert("Sec-WebSocket-Protocol", matched_protocol.clone());
                            }
                        }
                    }
                    Ok(response)
                }
                Err(message) => {
                    let mut response = ErrorResponse::default();
                    *response.status_mut() = StatusCode::UNAUTHORIZED;
                    *response.body_mut() = Some(message.message().to_string());
                    debug!("Websocket handshake auth validation failed: {message}");
                    Err(response)
                }
            }
        };

        pin_mut!(shutdown);
        let ws_stream = tokio::select! {
            biased;

            _ = shutdown.as_mut() => return Ok(()),
            result = tokio_tungstenite::accept_hdr_async(stream, header_callback) => {
                result.map_err(|error| {
                    debug!(message = "Error during WebSocket handshake.", %error);
                    emit!(WebSocketListenerConnectionFailedError {
                        error: Box::new(error),
                        extra_tags: extra_tags.clone()
                    })
                })?
            }
        };

        let _open_token = open_gauge.open(|count| emit!(ConnectionOpen { count }));

        // Insert the write part of this peer to the peer map.
        let (tx, rx) = unbounded();

        {
            let mut peers = peers.lock().expect("mutex poisoned");
            buffer_replay.replay_messages(
                &buffer.lock().expect("mutex poisoned"),
                |(_, message)| {
                    if let Err(error) = tx.unbounded_send(message.clone()) {
                        emit!(WebSocketListenerSendError {
                            error: Box::new(error)
                        });
                    }
                },
            );

            debug!("WebSocket connection established: {addr}");

            peers.insert(addr, tx);
            emit!(WebSocketListenerConnectionEstablished {
                client_count: peers.len(),
                extra_tags: extra_tags.clone()
            });
        }

        let peer_registration = PeerRegistration {
            addr,
            peers,
            extra_tags: extra_tags.clone(),
        };
        let (outgoing, incoming) = ws_stream.split();

        let incoming_data_handler = incoming.try_for_each(|msg| {
            let ip = addr.ip();
            debug!(
                "Received a message from {ip}: {}",
                msg.to_text().unwrap_or(&format!("Couldn't convert: {msg}"))
            );
            if let (Some(client_key), Some(checkpoint)) = (
                &client_checkpoint_key,
                message_buffering.handle_ack_request(msg),
            ) {
                debug!("Inserting checkpoint for {client_key}({ip}): {checkpoint}");
                client_checkpoints
                    .lock()
                    .expect("mutex poisoned")
                    .insert(client_key.clone(), checkpoint);
            }

            future::ok(())
        });
        let forward_data_to_client = rx
            .map(|message| {
                emit!(WebSocketListenerMessageSent {
                    message_size: message.len(),
                    extra_tags: extra_tags.clone()
                });
                Ok(message)
            })
            .chain(stream::once(future::ready(Ok(Message::Close(Some(
                CloseFrame {
                    code: CloseCode::Away,
                    reason: Cow::Borrowed("Server shutting down."),
                },
            ))))))
            .forward(outgoing);

        pin_mut!(forward_data_to_client, incoming_data_handler);
        let transfer = future::select(forward_data_to_client, incoming_data_handler);
        pin_mut!(transfer);
        let result = tokio::select! {
            biased;

            _ = shutdown.as_mut() => {
                // Closing the channel lets `forward` drain queued messages before sending Close.
                // Keep polling both directions so backpressure never blocks incoming ACKs or Close.
                peer_registration.stop_sending();
                transfer.await
            }
            result = transfer.as_mut() => result,
        };
        let result = match result {
            future::Either::Left((Ok(()), incoming_data_handler)) => {
                // Wait for the peer's close response within the supervisor's shared deadline.
                incoming_data_handler.await
            }
            future::Either::Left((Err(error), _)) => Err(error),
            future::Either::Right((result, _)) => result,
        };
        if let Err(error) = result {
            emit!(WebSocketListenerSendError {
                error: Box::new(error)
            });
        }

        Ok(())
    }
}

#[async_trait]
impl StreamSink<Event> for WebSocketListenerSink {
    async fn run(mut self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        let bytes_sent = register!(BytesSent::from(Protocol("websocket".into())));
        let events_sent = register!(EventsSent::from(Output(None)));
        let encode_as_binary = self.encoder.serializer().is_binary();

        let listener = self.tls.bind(&self.address).await.map_err(|_| ())?;

        let peers = Arc::new(Mutex::new(HashMap::default()));
        let message_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(
            self.message_buffering.buffer_capacity(),
        )));
        let client_checkpoints = Arc::new(Mutex::new(HashMap::default()));
        let (shutdown_trigger, shutdown) = Tripwire::new();

        let listener_task = crate::spawn_in_current_span(Self::handle_connections(
            self.auth,
            self.message_buffering.clone(),
            self.subprotocol.clone(),
            Arc::clone(&peers),
            self.extra_tags_config,
            Arc::clone(&client_checkpoints),
            Arc::clone(&message_buffer),
            listener,
            shutdown,
        ));
        let _abort_on_drop = AbortOnDrop(listener_task.abort_handle());

        while let Some(mut event) = input.next().await {
            let finalizers = event.take_finalizers();

            self.transformer.transform(&mut event);

            let message_id = self
                .message_buffering
                .add_replay_message_id_to_event(&mut event);

            let event_byte_size = event.estimated_json_encoded_size_of();

            let mut bytes = BytesMut::new();
            match self.encoder.encode(event, &mut bytes) {
                Ok(()) => {
                    finalizers.update_status(EventStatus::Delivered);

                    let message = if encode_as_binary {
                        Message::binary(bytes)
                    } else {
                        Message::text(String::from_utf8_lossy(&bytes))
                    };
                    let message_len = message.len();

                    if self.message_buffering.should_buffer() {
                        let mut buffer = message_buffer.lock().expect("mutex poisoned");
                        if buffer.len() + 1 >= buffer.capacity() {
                            buffer.pop_front();
                        }
                        buffer.push_back((message_id, message.clone()));
                    }

                    let peers = peers.lock().expect("mutex poisoned");
                    let broadcast_recipients = peers.values();
                    for recp in broadcast_recipients {
                        if let Err(error) = recp.unbounded_send(message.clone()) {
                            emit!(WebSocketListenerSendError {
                                error: Box::new(error)
                            });
                        } else {
                            events_sent.emit(CountByteSize(1, event_byte_size));
                            bytes_sent.emit(ByteSize(message_len));
                        }
                    }
                }
                Err(_) => {
                    // Error is handled by `Encoder`.
                    finalizers.update_status(EventStatus::Errored);
                }
            };
        }

        shutdown_trigger.cancel();
        listener_task.await.map_err(|error| {
            error!(message = "WebSocket listener task failed during shutdown.", %error);
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::{pending, ready},
        num::NonZeroUsize,
    };

    use futures::{SinkExt, Stream, StreamExt, channel::mpsc::UnboundedReceiver};
    use futures_util::stream;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        task::{JoinHandle, JoinSet},
        time,
    };
    use tokio_tungstenite::tungstenite::{
        client::IntoClientRequest, protocol::frame::coding::CloseCode,
    };
    use vector_lib::{
        codecs::{
            JsonDeserializerConfig,
            decoding::{DeserializerConfig, JsonDeserializerOptions},
        },
        lookup::lookup_v2::ConfigValuePath,
        metrics::Controller,
        sink::VectorSink,
    };

    use super::*;
    use crate::{
        event::{Event, LogEvent},
        sinks::websocket_server::{
            buffering::{BufferingAckConfig, ClientKeyConfig},
            config::InternalMetricsConfig,
        },
        test_util::{
            addr::next_addr,
            components::{SINK_TAGS, run_and_assert_sink_compliance},
        },
    };

    const METRICS_WITH_EXTRA_TAGS: [&str; 6] = [
        "connection_established_total",
        "active_clients",
        "component_errors_total",
        "connection_shutdown_total",
        "websocket_messages_sent_total",
        "websocket_bytes_sent_total",
    ];
    const SHUTDOWN_TEST_TIMEOUT: time::Duration = time::Duration::from_secs(2);
    const SHARED_DEADLINE_TEST_TIMEOUT: time::Duration = time::Duration::from_secs(8);

    type TestWebSocket =
        tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>;

    #[tokio::test]
    async fn test_single_client() {
        let event = Event::Log(LogEvent::from("foo"));

        let (mut sender, input_events) = build_test_event_channel();
        let (_guard, address) = next_addr();
        let port = address.port();

        let websocket_sink = start_websocket_server_sink(
            WebSocketListenerSinkConfig {
                address,
                ..Default::default()
            },
            input_events,
        )
        .await;

        let client_handle =
            attach_websocket_client(localhost_with_port(port), vec![event.clone()], false).await;
        sender.send(event).await.expect("Failed to send.");

        client_handle.await.unwrap();
        drop(sender);
        websocket_sink.await.unwrap();
    }

    #[tokio::test]
    async fn test_single_client_late_connect() {
        let event1 = Event::Log(LogEvent::from("foo1"));
        let event2 = Event::Log(LogEvent::from("foo2"));

        let (mut sender, input_events) = build_test_event_channel();
        let (_guard, address) = next_addr();
        let port = address.port();

        let websocket_sink = start_websocket_server_sink(
            WebSocketListenerSinkConfig {
                address,
                ..Default::default()
            },
            input_events,
        )
        .await;

        // Sending event 1 before client joined, the client should not received it
        sender.send(event1).await.expect("Failed to send.");

        // Now connect the client
        let client_handle =
            attach_websocket_client(localhost_with_port(port), vec![event2.clone()], false).await;

        // Sending event 2, this one should be received by the client
        sender.send(event2).await.expect("Failed to send.");

        client_handle.await.unwrap();
        drop(sender);
        websocket_sink.await.unwrap();
    }

    #[tokio::test]
    async fn test_multiple_clients() {
        let event = Event::Log(LogEvent::from("foo"));

        let (mut sender, input_events) = build_test_event_channel();
        let (_guard, address) = next_addr();
        let port = address.port();

        let websocket_sink = start_websocket_server_sink(
            WebSocketListenerSinkConfig {
                address,
                ..Default::default()
            },
            input_events,
        )
        .await;

        let client_handle_1 =
            attach_websocket_client(localhost_with_port(port), vec![event.clone()], false).await;
        let client_handle_2 =
            attach_websocket_client(localhost_with_port(port), vec![event.clone()], false).await;
        sender.send(event).await.expect("Failed to send.");

        client_handle_1.await.unwrap();
        client_handle_2.await.unwrap();
        drop(sender);
        websocket_sink.await.unwrap();
    }

    #[tokio::test]
    async fn extra_fixed_metrics_tags() {
        let event = Event::Log(LogEvent::from("foo"));

        let (mut sender, input_events) = build_test_event_channel();
        let (_guard, address) = next_addr();
        let port = address.port();

        let websocket_sink = start_websocket_server_sink(
            WebSocketListenerSinkConfig {
                address,
                internal_metrics: InternalMetricsConfig {
                    extra_tags: HashMap::from([(
                        "test_fixed_tag".to_string(),
                        ExtraMetricTagsConfig::Fixed {
                            value: "test_fixed_value".to_string(),
                        },
                    )]),
                },
                ..Default::default()
            },
            input_events,
        )
        .await;

        let client_handle =
            attach_websocket_client(localhost_with_port(port), vec![event.clone()], false).await;
        sender.send(event).await.expect("Failed to send.");

        client_handle.await.unwrap();
        drop(sender);
        websocket_sink.await.unwrap();

        let expected_tags =
            HashMap::from([("test_fixed_tag".to_string(), "test_fixed_value".to_string())]);
        assert_extra_metrics_tags(&expected_tags);
    }

    #[tokio::test]
    async fn extra_multiple_metrics_tags() {
        let event = Event::Log(LogEvent::from("foo"));

        let (mut sender, input_events) = build_test_event_channel();
        let (_guard, address) = next_addr();
        let port = address.port();

        let websocket_sink = start_websocket_server_sink(
            WebSocketListenerSinkConfig {
                address,
                internal_metrics: InternalMetricsConfig {
                    extra_tags: HashMap::from([
                        (
                            "test_fixed_tag".to_string(),
                            ExtraMetricTagsConfig::Fixed {
                                value: "test_fixed_value".to_string(),
                            },
                        ),
                        ("client_request_url".to_string(), ExtraMetricTagsConfig::Url),
                        (
                            "last_received_query_value".to_string(),
                            ExtraMetricTagsConfig::Query {
                                name: "last_received".to_string(),
                            },
                        ),
                    ]),
                },
                ..Default::default()
            },
            input_events,
        )
        .await;

        let full_url = format!(
            "{}/?last_received=x&some_other_param=ignored",
            localhost_with_port(port)
        );
        let client_handle =
            attach_websocket_client(full_url.clone(), vec![event.clone()], false).await;
        sender.send(event).await.expect("Failed to send.");

        client_handle.await.unwrap();
        drop(sender);
        websocket_sink.await.unwrap();

        let expected_tags = HashMap::from([
            ("test_fixed_tag".to_string(), "test_fixed_value".to_string()),
            (
                "client_request_url".to_string(),
                full_url
                    .strip_prefix(format!("ws://localhost:{port}").as_str())
                    .unwrap()
                    .to_string(),
            ),
            ("last_received_query_value".to_string(), "x".to_string()),
        ]);
        assert_extra_metrics_tags(&expected_tags);
    }

    #[tokio::test]
    async fn sink_spec_compliance() {
        let event = Event::Log(LogEvent::from("foo"));

        let (_guard, address) = next_addr();
        let sink = WebSocketListenerSink::new(
            WebSocketListenerSinkConfig {
                address,
                ..Default::default()
            },
            SinkContext::default(),
        )
        .unwrap();

        run_and_assert_sink_compliance(
            VectorSink::from_event_streamsink(sink),
            stream::once(ready(event)),
            &SINK_TAGS,
        )
        .await;
    }

    #[tokio::test]
    async fn test_client_late_connect_with_buffering() {
        let event1 = Event::Log(LogEvent::from("foo1"));
        let event2 = Event::Log(LogEvent::from("foo2"));

        let (mut sender, input_events) = build_test_event_channel();
        let (_guard, address) = next_addr();
        let port = address.port();

        let websocket_sink = start_websocket_server_sink(
            WebSocketListenerSinkConfig {
                address,
                message_buffering: Some(MessageBufferingConfig {
                    max_events: NonZeroUsize::new(1).unwrap(),
                    message_id_path: None,
                    client_ack_config: None,
                }),
                ..Default::default()
            },
            input_events,
        )
        .await;

        // Sending event 1 before client joined, the client without buffering should not receive it
        sender.send(event1.clone()).await.expect("Failed to send.");

        // Now connect the clients
        let client_handle =
            attach_websocket_client(localhost_with_port(port), vec![event2.clone()], false).await;
        let client_with_buffer_handle = attach_websocket_client_with_query(
            port,
            "last_received=0",
            vec![event1.clone(), event2.clone()],
        )
        .await;

        // Sending event 2, this one should be received by both clients
        sender.send(event2).await.expect("Failed to send.");

        client_handle.await.unwrap();
        client_with_buffer_handle.await.unwrap();
        drop(sender);
        websocket_sink.await.unwrap();
    }

    #[tokio::test]
    async fn test_client_late_connect_with_buffering_over_max_events_limit() {
        let event1 = Event::Log(LogEvent::from("foo1"));
        let event2 = Event::Log(LogEvent::from("foo2"));

        let (mut sender, input_events) = build_test_event_channel();
        let (_guard, address) = next_addr();
        let port = address.port();

        let websocket_sink = start_websocket_server_sink(
            WebSocketListenerSinkConfig {
                address,
                message_buffering: Some(MessageBufferingConfig {
                    max_events: NonZeroUsize::new(1).unwrap(),
                    message_id_path: None,
                    client_ack_config: None,
                }),
                ..Default::default()
            },
            input_events,
        )
        .await;

        let client_handle = attach_websocket_client(
            localhost_with_port(port),
            vec![event1.clone(), event2.clone()],
            false,
        )
        .await;

        // Sending 2 events before client joined, the client without buffering should receive just one
        sender.send(event1.clone()).await.expect("Failed to send.");
        sender.send(event2.clone()).await.expect("Failed to send.");

        let client_with_buffer_handle =
            attach_websocket_client_with_query(port, "last_received=0", vec![event2.clone()]).await;

        client_handle.await.unwrap();
        client_with_buffer_handle.await.unwrap();
        drop(sender);
        websocket_sink.await.unwrap();
    }

    #[tokio::test]
    async fn test_client_late_connect_with_acks() {
        let event1 = Event::Log(LogEvent::from("foo1"));
        let event2 = Event::Log(LogEvent::from("foo2"));
        let event3 = Event::Log(LogEvent::from("foo3"));

        let (mut sender, input_events) = build_test_event_channel();
        let (_guard, address) = next_addr();
        let port = address.port();

        let websocket_sink = start_websocket_server_sink(
            WebSocketListenerSinkConfig {
                address,
                message_buffering: Some(MessageBufferingConfig {
                    max_events: NonZeroUsize::new(1).unwrap(),
                    message_id_path: Some(ConfigValuePath::from("message_id")),
                    client_ack_config: Some(BufferingAckConfig {
                        ack_decoding: DeserializerConfig::Json(JsonDeserializerConfig::new(
                            JsonDeserializerOptions::default(),
                        )),
                        message_id_path: ConfigValuePath::from("message_id"),
                        client_key: ClientKeyConfig::IpAddress { with_port: false },
                    }),
                }),
                ..Default::default()
            },
            input_events,
        )
        .await;

        // First connection, to ACK and save last event
        let first_connection = attach_websocket_client_with_ack(port, vec![event1.clone()]).await;
        sender.send(event1.clone()).await.expect("Failed to send.");
        first_connection.await.unwrap();

        // Second event sent while not connected
        sender.send(event2.clone()).await.expect("Failed to send.");

        // Second connection, should receive missed event
        let second_connection =
            attach_websocket_client_with_ack(port, vec![event2.clone(), event3.clone()]).await;

        sender.send(event3.clone()).await.expect("Failed to send.");

        second_connection.await.unwrap();
        drop(sender);
        websocket_sink.await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_keeps_peer_registered_while_draining_messages() {
        let (_guard, address) = next_addr();
        let (sender, receiver) = unbounded();
        let message = Message::text("queued message");
        sender.unbounded_send(message.clone()).unwrap();
        let peers = Arc::new(Mutex::new(HashMap::from([(address, sender)])));
        let registration = PeerRegistration {
            addr: address,
            peers: Arc::clone(&peers),
            extra_tags: Vec::new(),
        };

        registration.stop_sending();
        assert_eq!(peers.lock().unwrap().len(), 1);
        let messages = time::timeout(SHUTDOWN_TEST_TIMEOUT, receiver.collect::<Vec<_>>())
            .await
            .expect("the closed channel should drain without removing the peer");
        assert_eq!(messages, vec![message]);
        drop(registration);
        assert!(peers.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn accept_error_keeps_supervisor_alive_until_shutdown() {
        let (_guard, address) = next_addr();
        // An empty allowlist deterministically rejects the accepted TCP stream.
        let listener = MaybeTlsSettings::Raw(())
            .bind_with_allowlist(&address, Vec::new())
            .await
            .unwrap();
        let (trigger, shutdown) = Tripwire::new();
        let supervisor = tokio::spawn(WebSocketListenerSink::handle_connections(
            None,
            None,
            SubProtocolConfig::default(),
            Arc::default(),
            HashMap::new(),
            Arc::default(),
            Arc::default(),
            listener,
            shutdown,
        ));
        let mut client = TcpStream::connect(address).await.unwrap();
        assert_stream_closed(&mut client).await;
        let _listener = wait_for_listener_release(address).await;
        assert!(!supervisor.is_finished());

        trigger.cancel();
        time::timeout(SHUTDOWN_TEST_TIMEOUT, supervisor)
            .await
            .expect("accept failure must not prevent shutdown")
            .unwrap();
    }

    #[tokio::test]
    async fn shutdown_without_clients_releases_listener_before_returning() {
        let (_guard, address) = next_addr();
        let (sender, websocket_sink) = start_shutdown_test_sink(address).await;

        drop(sender);
        await_sink_shutdown(websocket_sink, SHUTDOWN_TEST_TIMEOUT).await;

        let _listener = TcpListener::bind(address)
            .await
            .expect("the listener must be released before the sink returns");
    }

    #[tokio::test]
    async fn shutdown_drains_messages_and_sends_going_away_close() {
        let event = Event::Log(LogEvent::from("last message"));
        let (_guard, address) = next_addr();
        let (mut sender, websocket_sink) = start_shutdown_test_sink(address).await;
        let mut client = connect_websocket(address).await;

        sender.send(event).await.expect("Failed to send.");
        drop(sender);

        let message = time::timeout(SHUTDOWN_TEST_TIMEOUT, async {
            let message = client
                .next()
                .await
                .expect("server should send the queued message")
                .expect("queued message should be valid");
            assert_going_away_close(&mut client).await;
            message
        })
        .await
        .expect("server should drain and close promptly");

        let message: serde_json::Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
        assert_eq!(message["message"], "last message");
        drop(client);
        await_sink_shutdown(websocket_sink, SHUTDOWN_TEST_TIMEOUT).await;
    }

    #[tokio::test]
    async fn shutdown_cancels_incomplete_handshake() {
        let (_guard, address) = next_addr();
        let (sender, websocket_sink) = start_shutdown_test_sink(address).await;
        let mut partial_client = connect_partial_handshake(address).await;
        // This second connection can only finish its handshake after the accept loop has already
        // accepted and delegated the earlier partial handshake.
        let mut websocket_client = connect_websocket(address).await;

        drop(sender);
        time::timeout(
            SHUTDOWN_TEST_TIMEOUT,
            assert_going_away_close(&mut websocket_client),
        )
        .await
        .expect("established client should be closed promptly");
        drop(websocket_client);
        await_sink_shutdown(websocket_sink, SHUTDOWN_TEST_TIMEOUT).await;
        assert_stream_closed(&mut partial_client).await;
    }

    #[tokio::test]
    async fn cancelling_sink_aborts_listener_and_connection_tasks() {
        let (_guard, address) = next_addr();
        let (_sender, websocket_sink) = start_shutdown_test_sink(address).await;
        let mut partial_client = connect_partial_handshake(address).await;
        let mut websocket_client = connect_websocket(address).await;

        websocket_sink.abort();
        assert!(websocket_sink.await.unwrap_err().is_cancelled());
        assert_stream_closed(&mut partial_client).await;
        assert_websocket_closed(&mut websocket_client).await;
        drop(partial_client);
        drop(websocket_client);

        let _listener = wait_for_listener_release(address).await;
    }

    #[tokio::test]
    async fn shutdown_aborts_unresponsive_clients_after_shared_deadline() {
        let (_guard, address) = next_addr();
        let (sender, websocket_sink) = start_shutdown_test_sink(address).await;
        let client_one = connect_websocket(address).await;
        let client_two = connect_websocket(address).await;

        drop(sender);
        await_sink_shutdown(websocket_sink, SHARED_DEADLINE_TEST_TIMEOUT).await;

        for mut client in [client_one, client_two] {
            // Read the transport directly so tungstenite cannot acknowledge the Close frame.
            let mut bytes = Vec::new();
            if let Err(error) = time::timeout(
                SHUTDOWN_TEST_TIMEOUT,
                client.get_mut().read_to_end(&mut bytes),
            )
            .await
            .expect("the sink must close both client sockets before returning")
            {
                assert_eq!(error.kind(), std::io::ErrorKind::ConnectionReset);
            }
        }
        let _listener = TcpListener::bind(address)
            .await
            .expect("the listener must be released before the sink returns");
    }

    #[tokio::test(start_paused = true)]
    async fn connection_shutdown_timeout_is_shared_by_all_clients() {
        let mut connections = JoinSet::new();
        connections.spawn(pending::<Result<(), ()>>());
        connections.spawn(pending::<Result<(), ()>>());
        let started = time::Instant::now();

        WebSocketListenerSink::shutdown_connections(&mut connections).await;

        assert_eq!(started.elapsed(), CONNECTION_SHUTDOWN_GRACE_PERIOD);
        assert!(connections.is_empty());
    }

    async fn start_shutdown_test_sink(
        address: SocketAddr,
    ) -> (UnboundedSender<Event>, JoinHandle<Result<(), ()>>) {
        crate::test_util::trace_init();
        let (sender, events) = build_test_event_channel();
        let config = WebSocketListenerSinkConfig {
            address,
            ..Default::default()
        };
        let sink = WebSocketListenerSink::new(config, SinkContext::default()).unwrap();
        let sink = VectorSink::from_event_streamsink(sink);
        let sink_task = tokio::spawn(async move { sink.run(events.map(Into::into)).await });

        time::sleep(time::Duration::from_millis(100)).await;

        (sender, sink_task)
    }

    async fn await_sink_shutdown(sink: JoinHandle<Result<(), ()>>, timeout: time::Duration) {
        time::timeout(timeout, sink)
            .await
            .expect("sink should stop within the test timeout")
            .unwrap()
            .unwrap();
    }

    async fn connect_websocket(address: SocketAddr) -> TestWebSocket {
        tokio_tungstenite::connect_async(localhost_with_port(address.port()))
            .await
            .expect("WebSocket client should connect")
            .0
    }

    async fn connect_partial_handshake(address: SocketAddr) -> TcpStream {
        let mut client = TcpStream::connect(address)
            .await
            .expect("TCP client should connect");
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nUpgrade: web")
            .await
            .expect("partial handshake should be written");
        client
    }

    async fn assert_going_away_close(client: &mut TestWebSocket) {
        let Some(Ok(Message::Close(Some(frame)))) = client.next().await else {
            panic!("server should send a valid close frame");
        };
        assert_eq!(frame.code, CloseCode::Away);
        client
            .flush()
            .await
            .expect("client should acknowledge close");
    }

    async fn assert_websocket_closed(client: &mut TestWebSocket) {
        time::timeout(SHUTDOWN_TEST_TIMEOUT, async {
            while let Some(Ok(message)) = client.next().await {
                if matches!(message, Message::Close(_)) {
                    client.flush().await.ok();
                }
            }
        })
        .await
        .expect("cancelling the sink should close established clients");
    }

    async fn wait_for_listener_release(address: SocketAddr) -> TcpListener {
        time::timeout(SHUTDOWN_TEST_TIMEOUT, async {
            loop {
                match TcpListener::bind(address).await {
                    Ok(listener) => break listener,
                    Err(_) => time::sleep(time::Duration::from_millis(10)).await,
                }
            }
        })
        .await
        .expect("cancelling the sink should eventually release the listener")
    }

    async fn start_websocket_server_sink<S>(
        config: WebSocketListenerSinkConfig,
        events: S,
    ) -> JoinHandle<()>
    where
        S: Stream<Item = Event> + Send + 'static,
    {
        let sink = WebSocketListenerSink::new(config, SinkContext::default()).unwrap();

        let compliance_assertion = tokio::spawn(run_and_assert_sink_compliance(
            VectorSink::from_event_streamsink(sink),
            events,
            &SINK_TAGS,
        ));

        time::sleep(time::Duration::from_millis(100)).await;

        compliance_assertion
    }

    async fn assert_stream_closed(stream: &mut TcpStream) {
        let mut byte = [0];
        let result = time::timeout(time::Duration::from_secs(2), stream.read(&mut byte))
            .await
            .expect("server side of TCP stream should close promptly");
        match result {
            Ok(0) | Err(_) => {}
            Ok(count) => panic!("expected a closed TCP stream, read {count} bytes"),
        }
    }

    fn localhost_with_port(port: u16) -> String {
        format!("ws://localhost:{port}")
    }

    async fn attach_websocket_client_with_query(
        port: u16,
        query: &str,
        expected_events: Vec<Event>,
    ) -> JoinHandle<()> {
        attach_websocket_client(
            format!("{}/?{query}", localhost_with_port(port)),
            expected_events,
            false,
        )
        .await
    }

    async fn attach_websocket_client_with_ack(
        port: u16,
        expected_events: Vec<Event>,
    ) -> JoinHandle<()> {
        attach_websocket_client(localhost_with_port(port), expected_events, true).await
    }

    async fn attach_websocket_client<R: IntoClientRequest + Unpin>(
        client_request: R,
        expected_events: Vec<Event>,
        ack: bool,
    ) -> JoinHandle<()> {
        let (ws_stream, _) = tokio_tungstenite::connect_async(client_request)
            .await
            .expect("Client failed to connect.");
        let (mut tx, rx) = ws_stream.split();
        tokio::spawn(async move {
            let events = expected_events.clone();

            let pairs: Vec<(Result<Message, _>, Event)> = rx
                .take(events.len())
                .zip(stream::iter(events))
                .collect()
                .await;

            pairs.iter().for_each(|(msg, expected)| {
                let mut base_msg = serde_json::from_str::<Value>(
                    &msg.as_ref().unwrap().clone().into_text().unwrap(),
                )
                .unwrap();
                // Removing message_id from message, since it is not part of the event
                base_msg.remove(vrl::path!("message_id"), true);
                let msg_text = serde_json::to_string(&base_msg).unwrap();
                let expected = serde_json::to_string(expected.clone().into_log().value()).unwrap();
                assert_eq!(expected, msg_text);
            });

            if ack {
                for (msg, _) in pairs {
                    tx.send(msg.unwrap()).await.unwrap();
                }
            }

            // Error is ignored since it only fails if the channel is already closed.
            tx.close().await.ok();
        })
    }

    fn assert_extra_metrics_tags(expected: &HashMap<String, String>) {
        let captured_metrics = Controller::get().unwrap().capture_metrics();
        let mut found_metrics = false;
        for metric in captured_metrics {
            let metric_name = metric.name();
            if METRICS_WITH_EXTRA_TAGS.contains(&metric_name) {
                let Some(tags) = metric.tags() else {
                    panic!("Expected metric {metric_name} to have tags!");
                };
                for (key, value) in expected {
                    let Some(tag_value) = tags.get(key.as_str()) else {
                        panic!("Expected metric {metric_name} to have {key} tag!");
                    };
                    assert_eq!(tag_value.to_string(), *value);
                }
                found_metrics = true;
            }
        }
        if !found_metrics {
            panic!("Websocket server didn't emit any of the metrics that use extra tags!");
        }
    }

    fn build_test_event_channel() -> (UnboundedSender<Event>, UnboundedReceiver<Event>) {
        let (tx, rx) = futures::channel::mpsc::unbounded();
        (tx, rx)
    }
}
