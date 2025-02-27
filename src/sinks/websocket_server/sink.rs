use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use bytes::BytesMut;
use futures::{
    channel::mpsc::{unbounded, UnboundedSender},
    pin_mut,
    stream::BoxStream,
    StreamExt,
};
use http::StatusCode;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::{
    handshake::server::{ErrorResponse, Request, Response},
    Message,
};
use tokio_util::codec::Encoder as _;
use tracing::Instrument;
use uuid::Uuid;
use vector_lib::{
    event::{Event, EventStatus},
    finalization::Finalizable,
    internal_event::{
        ByteSize, BytesSent, CountByteSize, EventsSent, InternalEventHandle, Output, Protocol,
    },
    sink::StreamSink,
    tls::{MaybeTlsIncomingStream, MaybeTlsListener, MaybeTlsSettings},
    EstimatedJsonEncodedSizeOf,
};

use crate::{
    codecs::{Encoder, Transformer},
    common::http::server_auth::HttpServerAuthMatcher,
    internal_events::{
        ConnectionOpen, OpenGauge, WsConnectionFailedError, WsListenerConnectionEstablished,
        WsListenerConnectionShutdown, WsListenerSendError,
    },
    sinks::{
        prelude::*,
        websocket_server::buffering::{BufferReplayRequest, WsMessageBufferConfig},
    },
};

use super::{buffering::MessageBufferingConfig, WebSocketListenerSinkConfig};

pub struct WebSocketListenerSink {
    peers: Arc<Mutex<HashMap<SocketAddr, UnboundedSender<Message>>>>,
    tls: MaybeTlsSettings,
    transformer: Transformer,
    encoder: Encoder<()>,
    address: SocketAddr,
    auth: Option<HttpServerAuthMatcher>,
    message_buffering: Option<MessageBufferingConfig>,
}

impl WebSocketListenerSink {
    pub fn new(config: WebSocketListenerSinkConfig, cx: SinkContext) -> crate::Result<Self> {
        let tls = MaybeTlsSettings::from_config(config.tls.as_ref(), true)?;
        let transformer = config.encoding.transformer();
        let serializer = config.encoding.build()?;
        let encoder = Encoder::<()>::new(serializer);
        let auth = config
            .auth
            .map(|config| config.build(&cx.enrichment_tables))
            .transpose()?;
        Ok(Self {
            peers: Arc::new(Mutex::new(HashMap::new())),
            tls,
            address: config.address,
            transformer,
            encoder,
            auth,
            message_buffering: config.message_buffering,
        })
    }

    const fn should_encode_as_binary(&self) -> bool {
        use vector_lib::codecs::encoding::Serializer::{
            Avro, Cef, Csv, Gelf, Json, Logfmt, Native, NativeJson, Protobuf, RawMessage, Text,
        };

        match self.encoder.serializer() {
            RawMessage(_) | Avro(_) | Native(_) | Protobuf(_) => true,
            Cef(_) | Csv(_) | Logfmt(_) | Gelf(_) | Json(_) | Text(_) | NativeJson(_) => false,
        }
    }

    async fn handle_connections(
        auth: Option<HttpServerAuthMatcher>,
        message_buffering: Option<MessageBufferingConfig>,
        peers: Arc<Mutex<HashMap<SocketAddr, UnboundedSender<Message>>>>,
        buffer: Arc<Mutex<VecDeque<(Uuid, Message)>>>,
        mut listener: MaybeTlsListener,
    ) {
        let open_gauge = OpenGauge::new();

        while let Ok(stream) = listener.accept().await {
            tokio::spawn(
                Self::handle_connection(
                    auth.clone(),
                    message_buffering.clone(),
                    Arc::clone(&peers),
                    Arc::clone(&buffer),
                    stream,
                    open_gauge.clone(),
                )
                .in_current_span(),
            );
        }
    }

    async fn handle_connection(
        auth: Option<HttpServerAuthMatcher>,
        message_buffering: Option<MessageBufferingConfig>,
        peers: Arc<Mutex<HashMap<SocketAddr, UnboundedSender<Message>>>>,
        buffer: Arc<Mutex<VecDeque<(Uuid, Message)>>>,
        stream: MaybeTlsIncomingStream<TcpStream>,
        open_gauge: OpenGauge,
    ) -> Result<(), ()> {
        // Base url for parsing request URLs that may be relative
        let addr = stream.peer_addr();
        debug!("Incoming TCP connection from: {}", addr);

        let mut buffer_replay = BufferReplayRequest::NO_REPLAY;
        let header_callback = |req: &Request, response: Response| {
            buffer_replay = message_buffering.extract_message_replay_request(req);
            let Some(auth) = auth else {
                return Ok(response);
            };
            match auth.handle_auth(req.headers()) {
                Ok(_) => Ok(response),
                Err(message) => {
                    let mut response = ErrorResponse::default();
                    *response.status_mut() = StatusCode::UNAUTHORIZED;
                    *response.body_mut() = Some(message.message().to_string());
                    debug!("Websocket handshake auth validation failed: {}", message);
                    Err(response)
                }
            }
        };

        let ws_stream = tokio_tungstenite::accept_hdr_async(stream, header_callback)
            .await
            .map_err(|err| {
                debug!("Error during websocket handshake: {}", err);
                emit!(WsConnectionFailedError {
                    error: Box::new(err)
                })
            })?;

        let _open_token = open_gauge.open(|count| emit!(ConnectionOpen { count }));

        // Insert the write part of this peer to the peer map.
        let (tx, rx) = unbounded();

        {
            let mut peers = peers.lock().unwrap();
            buffer_replay.replay_messages(&buffer.lock().unwrap(), |(_, message)| {
                if let Err(error) = tx.unbounded_send(message.clone()) {
                    emit!(WsListenerSendError { error });
                }
            });

            debug!("WebSocket connection established: {}", addr);

            peers.insert(addr, tx);
            emit!(WsListenerConnectionEstablished {
                client_count: peers.len()
            });
        }

        let (outgoing, _incoming) = ws_stream.split();

        let forward_data_to_client = rx.map(Ok).forward(outgoing);

        pin_mut!(forward_data_to_client);
        let _ = forward_data_to_client.await;

        {
            let mut peers = peers.lock().unwrap();
            debug!("{} disconnected.", &addr);
            peers.remove(&addr);
            emit!(WsListenerConnectionShutdown {
                client_count: peers.len()
            });
        }

        Ok(())
    }
}

#[async_trait]
impl StreamSink<Event> for WebSocketListenerSink {
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let input = input.fuse().peekable();
        pin_mut!(input);

        let bytes_sent = register!(BytesSent::from(Protocol("websocket".into())));
        let events_sent = register!(EventsSent::from(Output(None)));
        let encode_as_binary = self.should_encode_as_binary();

        let listener = self.tls.bind(&self.address).await.map_err(|_| ())?;

        let message_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(
            self.message_buffering.buffer_capacity(),
        )));
        tokio::spawn(
            Self::handle_connections(
                self.auth,
                self.message_buffering.clone(),
                Arc::clone(&self.peers),
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

                    let message = if encode_as_binary {
                        Message::binary(bytes)
                    } else {
                        Message::text(String::from_utf8_lossy(&bytes))
                    };
                    let message_len = message.len();

                    if self.message_buffering.should_buffer() {
                        let mut buffer = message_buffer.lock().unwrap();
                        if buffer.len() + 1 >= buffer.capacity() {
                            buffer.pop_front();
                        }
                        buffer.push_back((message_id, message.clone()));
                    }

                    let peers = self.peers.lock().unwrap();
                    let broadcast_recipients = peers.iter().map(|(_, ws_sink)| ws_sink);
                    for recp in broadcast_recipients {
                        if let Err(error) = recp.unbounded_send(message.clone()) {
                            emit!(WsListenerSendError { error });
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

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use futures::{channel::mpsc::UnboundedReceiver, SinkExt, Stream, StreamExt};
    use futures_util::stream;
    use std::{future::ready, num::NonZeroUsize};

    use tokio::{task::JoinHandle, time};
    use vector_lib::sink::VectorSink;

    use super::*;

    use crate::{
        event::{Event, LogEvent},
        test_util::{
            components::{run_and_assert_sink_compliance, SINK_TAGS},
            next_addr,
        },
    };

    #[tokio::test]
    async fn test_single_client() {
        let event = Event::Log(LogEvent::from("foo"));

        let (mut sender, input_events) = build_test_event_channel();
        let address = next_addr();
        let port = address.port();

        let websocket_sink = start_websocket_server_sink(
            WebSocketListenerSinkConfig {
                address,
                ..Default::default()
            },
            input_events,
        )
        .await;

        let client_handle = attach_websocket_client(port, vec![event.clone()]).await;
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
        let address = next_addr();
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
        let client_handle = attach_websocket_client(port, vec![event2.clone()]).await;

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
        let address = next_addr();
        let port = address.port();

        let websocket_sink = start_websocket_server_sink(
            WebSocketListenerSinkConfig {
                address,
                ..Default::default()
            },
            input_events,
        )
        .await;

        let client_handle_1 = attach_websocket_client(port, vec![event.clone()]).await;
        let client_handle_2 = attach_websocket_client(port, vec![event.clone()]).await;
        sender.send(event).await.expect("Failed to send.");

        client_handle_1.await.unwrap();
        client_handle_2.await.unwrap();
        drop(sender);
        websocket_sink.await.unwrap();
    }

    #[tokio::test]
    async fn sink_spec_compliance() {
        let event = Event::Log(LogEvent::from("foo"));

        let sink = WebSocketListenerSink::new(
            WebSocketListenerSinkConfig {
                address: next_addr(),
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
        let address = next_addr();
        let port = address.port();

        let websocket_sink = start_websocket_server_sink(
            WebSocketListenerSinkConfig {
                address,
                message_buffering: Some(MessageBufferingConfig {
                    max_events: NonZeroUsize::new(1).unwrap(),
                    message_id_path: None,
                }),
                ..Default::default()
            },
            input_events,
        )
        .await;

        // Sending event 1 before client joined, the client without buffering should not receive it
        sender.send(event1.clone()).await.expect("Failed to send.");

        // Now connect the clients
        let client_handle = attach_websocket_client(port, vec![event2.clone()]).await;
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
        let address = next_addr();
        let port = address.port();

        let websocket_sink = start_websocket_server_sink(
            WebSocketListenerSinkConfig {
                address,
                message_buffering: Some(MessageBufferingConfig {
                    max_events: NonZeroUsize::new(1).unwrap(),
                    message_id_path: None,
                }),
                ..Default::default()
            },
            input_events,
        )
        .await;

        let client_handle =
            attach_websocket_client(port, vec![event1.clone(), event2.clone()]).await;

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

    async fn attach_websocket_client_with_query(
        port: u16,
        query: &str,
        expected_events: Vec<Event>,
    ) -> JoinHandle<()> {
        attach_websocket_client_url(format!("ws://localhost:{port}/?{query}"), expected_events)
            .await
    }

    async fn attach_websocket_client(port: u16, expected_events: Vec<Event>) -> JoinHandle<()> {
        attach_websocket_client_url(format!("ws://localhost:{port}"), expected_events).await
    }

    async fn attach_websocket_client_url(
        url: String,
        expected_events: Vec<Event>,
    ) -> JoinHandle<()> {
        let (ws_stream, _) = tokio_tungstenite::connect_async(url)
            .await
            .expect("Client failed to connect.");
        let (_, rx) = ws_stream.split();
        tokio::spawn(async move {
            let events = expected_events.clone();
            rx.take(events.len())
                .zip(stream::iter(events))
                .for_each(|(msg, expected)| async {
                    let msg_text = msg.unwrap().into_text().unwrap();
                    let expected = serde_json::to_string(expected.into_log().value()).unwrap();
                    assert_eq!(expected, msg_text);
                })
                .await;
        })
    }

    fn build_test_event_channel() -> (UnboundedSender<Event>, UnboundedReceiver<Event>) {
        let (tx, rx) = futures::channel::mpsc::unbounded();
        (tx, rx)
    }
}
