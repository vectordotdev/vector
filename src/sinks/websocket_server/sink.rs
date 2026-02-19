use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use super::{
    WebSocketListenerSinkConfig,
    buffering::MessageBufferingConfig,
    config::{ExtraMetricTagsConfig, SubProtocolConfig},
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
use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use futures::{
    StreamExt,
    channel::mpsc::{UnboundedSender, unbounded},
    future, pin_mut,
    stream::BoxStream,
};
use http_body_util::Empty;
use hyper1::body::Incoming;
use tokio::net::TcpStream;
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
use yawc::{Frame, WebSocket as YawcWebSocket};

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
    pub fn new(config: WebSocketListenerSinkConfig, cx: SinkContext) -> crate::Result<Self> {
        let tls = MaybeTlsSettings::from_config(config.tls.as_ref(), true)?;
        let transformer = config.encoding.transformer();
        let serializer = config.encoding.build()?;
        let encoder = Encoder::<()>::new(serializer);
        let auth = config
            .auth
            .map(|config| config.build(&cx.enrichment_tables, &cx.metrics_storage))
            .transpose()?;

        Ok(Self {
            tls,
            address: config.address,
            transformer,
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
        req: &hyper1::Request<Incoming>,
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
        peers: Arc<Mutex<HashMap<SocketAddr, UnboundedSender<Frame>>>>,
        extra_tags_config: HashMap<String, ExtraMetricTagsConfig>,
        client_checkpoints: Arc<Mutex<HashMap<String, Uuid>>>,
        buffer: Arc<Mutex<VecDeque<(Uuid, Frame)>>>,
        mut listener: MaybeTlsListener,
    ) {
        let open_gauge = OpenGauge::new();

        while let Ok(stream) = listener.accept().await {
            tokio::spawn(
                Self::handle_connection(
                    auth.clone(),
                    message_buffering.clone(),
                    subprotocol.clone(),
                    Arc::clone(&peers),
                    Arc::clone(&client_checkpoints),
                    Arc::clone(&buffer),
                    stream,
                    extra_tags_config.clone(),
                    open_gauge.clone(),
                )
                .in_current_span(),
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_connection(
        auth: Option<HttpServerAuthMatcher>,
        message_buffering: Option<MessageBufferingConfig>,
        subprotocol: SubProtocolConfig,
        peers: Arc<Mutex<HashMap<SocketAddr, UnboundedSender<Frame>>>>,
        client_checkpoints: Arc<Mutex<HashMap<String, Uuid>>>,
        buffer: Arc<Mutex<VecDeque<(Uuid, Frame)>>>,
        stream: MaybeTlsIncomingStream<TcpStream>,
        extra_tags_config: HashMap<String, ExtraMetricTagsConfig>,
        open_gauge: OpenGauge,
    ) -> Result<(), ()> {
        // Base url for parsing request URLs that may be relative
        let base_url = Url::parse("ws://localhost").ok();
        let addr = stream.peer_addr();
        debug!("Incoming TCP connection from: {}", addr);

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

        // Channel to communicate upgrade context from the HTTP service to the outer scope
        let (ctx_tx, ctx_rx) = tokio::sync::oneshot::channel::<(
            yawc::UpgradeFut,
            Vec<(String, String)>, // extra_tags from request
            Option<String>,        // client_checkpoint_key
            BufferReplayRequest,   // buffer_replay
        )>();
        let ctx_tx = std::sync::Mutex::new(Some(ctx_tx));

        let io = hyper_util::rt::TokioIo::new(stream);

        let message_buffering_for_handler = message_buffering.clone();
        let client_checkpoints_for_handler = Arc::clone(&client_checkpoints);
        let service = hyper1::service::service_fn(move |mut req: hyper1::Request<Incoming>| {
            let auth = auth.clone();
            let message_buffering = message_buffering.clone();
            let subprotocol = subprotocol.clone();
            let extra_tags_config = extra_tags_config.clone();
            let client_checkpoints = Arc::clone(&client_checkpoints);
            let base_url = base_url.clone();
            let ctx_tx = ctx_tx.lock().unwrap().take();

            async move {
                let client_checkpoint_key = message_buffering.client_key(&req, &addr);
                let buffer_replay = message_buffering.extract_message_replay_request(
                    &req,
                    client_checkpoint_key.clone().and_then(|key| {
                        client_checkpoints
                            .lock()
                            .expect("mutex poisoned")
                            .get(&key)
                            .cloned()
                    }),
                );

                // Validate auth
                let mut req_extra_tags = Vec::new();
                if let Some(ref auth) = auth {
                    match auth.handle_auth(
                        Some(&addr),
                        &convert_headers(req.headers()),
                        req.uri().path(),
                    ) {
                        Ok(_) => {
                            req_extra_tags.append(&mut Self::extract_extra_tags(
                                &extra_tags_config,
                                base_url.as_ref(),
                                &req,
                            ));

                            // Handle subprotocol negotiation (will be added to upgrade response below)
                        }
                        Err(message) => {
                            debug!("Websocket handshake auth validation failed: {}", message);
                            return Ok::<_, hyper1::Error>(
                                hyper1::Response::builder()
                                    .status(hyper1::StatusCode::UNAUTHORIZED)
                                    .body(Empty::<Bytes>::new())
                                    .unwrap(),
                            );
                        }
                    }
                } else {
                    req_extra_tags.append(&mut Self::extract_extra_tags(
                        &extra_tags_config,
                        base_url.as_ref(),
                        &req,
                    ));
                }

                // Perform WebSocket upgrade
                let (mut response, upgrade_fut) = match YawcWebSocket::upgrade(&mut req) {
                    Ok(result) => result,
                    Err(err) => {
                        debug!("WebSocket upgrade failed: {}", err);
                        return Ok(hyper1::Response::builder()
                            .status(hyper1::StatusCode::BAD_REQUEST)
                            .body(Empty::<Bytes>::new())
                            .unwrap());
                    }
                };

                // Handle subprotocol negotiation on the response
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
                        let requested_protocols = req.headers().get_all("Sec-WebSocket-Protocol");
                        if let Some(matched_protocol) =
                            requested_protocols.iter().find(|requested_protocol| {
                                supported_subprotocols.iter().any(|supported_protocol| {
                                    requested_protocol.as_bytes() == supported_protocol.as_bytes()
                                })
                            })
                        {
                            response
                                .headers_mut()
                                .insert("Sec-WebSocket-Protocol", matched_protocol.clone());
                        }
                    }
                }

                // Send upgrade context
                if let Some(tx) = ctx_tx {
                    let _ = tx.send((
                        upgrade_fut,
                        req_extra_tags,
                        client_checkpoint_key,
                        buffer_replay,
                    ));
                }

                Ok(response)
            }
        });

        // Spawn the HTTP1 connection handler
        tokio::spawn(async move {
            let _ = hyper1::server::conn::http1::Builder::new()
                .serve_connection(io, service)
                .with_upgrades()
                .await;
        });

        // Wait for the upgrade context from the HTTP handler
        let (upgrade_fut, req_extra_tags, client_checkpoint_key, buffer_replay) =
            ctx_rx.await.map_err(|_| {
                debug!("HTTP handler did not send upgrade context (auth failure or bad request)");
                emit!(WebSocketListenerConnectionFailedError {
                    error: Box::<dyn std::error::Error + Send + Sync>::from(
                        "WebSocket upgrade failed"
                    ),
                    extra_tags: extra_tags.clone()
                })
            })?;

        extra_tags.extend(req_extra_tags);

        // Wait for the WebSocket upgrade to complete
        let ws = upgrade_fut.await.map_err(|err| {
            debug!("WebSocket upgrade error: {}", err);
            emit!(WebSocketListenerConnectionFailedError {
                error: Box::new(err),
                extra_tags: extra_tags.clone()
            })
        })?;

        let _open_token = open_gauge.open(|count| emit!(ConnectionOpen { count }));

        // Insert the write part of this peer to the peer map.
        let (tx, rx) = unbounded();

        {
            let mut peers = peers.lock().expect("mutex poisoned");
            buffer_replay.replay_messages(&buffer.lock().expect("mutex poisoned"), |(_, frame)| {
                if let Err(error) = tx.unbounded_send(frame.clone()) {
                    emit!(WebSocketListenerSendError {
                        error: Box::new(error)
                    });
                }
            });

            debug!("WebSocket connection established: {}", addr);

            peers.insert(addr, tx);
            emit!(WebSocketListenerConnectionEstablished {
                client_count: peers.len(),
                extra_tags: extra_tags.clone()
            });
        }

        let (outgoing, incoming) = ws.split();

        let incoming_data_handler = incoming.for_each(|frame| {
            let ip = addr.ip();
            let text = std::str::from_utf8(frame.payload()).unwrap_or("<non-utf8>");
            debug!("Received a message from {ip}: {text}",);
            if let (Some(client_key), Some(checkpoint)) = (
                &client_checkpoint_key,
                message_buffering_for_handler.handle_ack_request(frame),
            ) {
                debug!("Inserting checkpoint for {client_key}({ip}): {checkpoint}");
                client_checkpoints_for_handler
                    .lock()
                    .expect("mutex poisoned")
                    .insert(client_key.clone(), checkpoint);
            }

            future::ready(())
        });
        let forward_data_to_client = rx
            .map(|frame| {
                emit!(WebSocketListenerMessageSent {
                    message_size: frame.payload().len(),
                    extra_tags: extra_tags.clone()
                });
                Ok(frame)
            })
            .forward(outgoing);

        pin_mut!(forward_data_to_client, incoming_data_handler);
        match future::select(forward_data_to_client, incoming_data_handler).await {
            future::Either::Left((Err(error), _)) => {
                emit!(WebSocketListenerSendError {
                    error: Box::new(error)
                })
            }
            _ => {}
        }

        {
            let mut peers = peers.lock().expect("mutex poisoned");
            debug!("{} disconnected.", &addr);
            peers.remove(&addr);
            emit!(WebSocketListenerConnectionShutdown {
                client_count: peers.len(),
                extra_tags: extra_tags.clone()
            });
        }

        Ok(())
    }
}

/// Convert http 1.x HeaderMap to http 0.2 HeaderMap for compatibility with auth handler.
fn convert_headers(headers: &hyper1::HeaderMap) -> http::header::HeaderMap {
    let mut map = http::header::HeaderMap::new();
    for (name, value) in headers.iter() {
        if let (Ok(name), Ok(value)) = (
            http::header::HeaderName::from_bytes(name.as_str().as_bytes()),
            http::header::HeaderValue::from_bytes(value.as_bytes()),
        ) {
            map.insert(name, value);
        }
    }
    map
}

#[async_trait]
impl StreamSink<Event> for WebSocketListenerSink {
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let input = input.fuse().peekable();
        pin_mut!(input);

        let bytes_sent = register!(BytesSent::from(Protocol("websocket".into())));
        let events_sent = register!(EventsSent::from(Output(None)));
        let encode_as_binary = self.encoder.serializer().is_binary();

        let listener = self.tls.bind(&self.address).await.map_err(|_| ())?;

        let peers = Arc::new(Mutex::new(HashMap::default()));
        let message_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(
            self.message_buffering.buffer_capacity(),
        )));
        let client_checkpoints = Arc::new(Mutex::new(HashMap::default()));

        tokio::spawn(
            Self::handle_connections(
                self.auth,
                self.message_buffering.clone(),
                self.subprotocol.clone(),
                Arc::clone(&peers),
                self.extra_tags_config,
                Arc::clone(&client_checkpoints),
                Arc::clone(&message_buffer),
                listener,
            )
            .in_current_span(),
        );

        while input.as_mut().peek().await.is_some() {
            let mut event = input.next().await.unwrap();
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

                    let frame = if encode_as_binary {
                        Frame::binary(bytes.freeze())
                    } else {
                        Frame::text(String::from_utf8_lossy(&bytes).into_owned())
                    };
                    let frame_len = frame.payload().len();

                    if self.message_buffering.should_buffer() {
                        let mut buffer = message_buffer.lock().expect("mutex poisoned");
                        if buffer.len() + 1 >= buffer.capacity() {
                            buffer.pop_front();
                        }
                        buffer.push_back((message_id, frame.clone()));
                    }

                    let peers = peers.lock().expect("mutex poisoned");
                    let broadcast_recipients = peers.values();
                    for recp in broadcast_recipients {
                        if let Err(error) = recp.unbounded_send(frame.clone()) {
                            emit!(WebSocketListenerSendError {
                                error: Box::new(error)
                            });
                        } else {
                            events_sent.emit(CountByteSize(1, event_byte_size));
                            bytes_sent.emit(ByteSize(frame_len));
                        }
                    }
                }
                Err(_) => {
                    // Error is handled by `Encoder`.
                    finalizers.update_status(EventStatus::Errored);
                }
            };
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{future::ready, num::NonZeroUsize};

    use futures::{SinkExt, Stream, StreamExt, channel::mpsc::UnboundedReceiver};
    use futures_util::stream;
    use tokio::{task::JoinHandle, time};
    use vector_lib::{
        codecs::{
            JsonDeserializerConfig,
            decoding::{DeserializerConfig, JsonDeserializerOptions},
        },
        lookup::lookup_v2::ConfigValuePath,
        metrics::Controller,
        sink::VectorSink,
    };
    use yawc::{Frame, WebSocket as YawcWebSocket};

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

    async fn attach_websocket_client(
        url: String,
        expected_events: Vec<Event>,
        ack: bool,
    ) -> JoinHandle<()> {
        let ws = YawcWebSocket::connect(Url::parse(&url).expect("Invalid URL"))
            .await
            .expect("Client failed to connect.");
        let (mut tx, rx) = ws.split();
        tokio::spawn(async move {
            let events = expected_events.clone();

            let pairs: Vec<(Frame, Event)> = rx
                .take(events.len())
                .zip(stream::iter(events))
                .collect()
                .await;

            pairs.iter().for_each(|(frame, expected)| {
                let text = std::str::from_utf8(frame.payload()).unwrap();
                let mut base_msg = serde_json::from_str::<Value>(text).unwrap();
                // Removing message_id from message, since it is not part of the event
                base_msg.remove("message_id", true);
                let msg_text = serde_json::to_string(&base_msg).unwrap();
                let expected = serde_json::to_string(expected.clone().into_log().value()).unwrap();
                assert_eq!(expected, msg_text);
            });

            if ack {
                for (frame, _) in pairs {
                    tx.send(frame).await.unwrap();
                }
            }

            let _ = tx.close().await;
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
