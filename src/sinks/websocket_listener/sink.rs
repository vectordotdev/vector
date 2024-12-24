use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use bytes::BytesMut;
use futures::{
    channel::mpsc::{unbounded, UnboundedSender},
    pin_mut,
    stream::BoxStream,
    FutureExt, StreamExt, TryStreamExt,
};
use futures_util::future;
use http::StatusCode;
use stream_cancel::Tripwire;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::{
    handshake::server::{ErrorResponse, Request, Response},
    Message,
};
use tokio_util::codec::Encoder as _;
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
    common::server_auth::AuthMatcher,
    config::SinkContext,
};

use super::WebSocketListenerSinkConfig;

pub struct WebSocketListenerSink {
    peers: Arc<Mutex<HashMap<SocketAddr, UnboundedSender<Message>>>>,
    tls: MaybeTlsSettings,
    transformer: Transformer,
    encoder: Encoder<()>,
    address: SocketAddr,
    auth: Option<AuthMatcher>,
}

impl WebSocketListenerSink {
    pub fn new(config: WebSocketListenerSinkConfig, cx: &SinkContext) -> crate::Result<Self> {
        let tls = MaybeTlsSettings::from_config(config.tls.as_ref(), true)?;
        let transformer = config.encoding.transformer();
        let serializer = config.encoding.build()?;
        let encoder = Encoder::<()>::new(serializer);
        Ok(Self {
            peers: Arc::new(Mutex::new(HashMap::new())),
            tls,
            address: config.address,
            transformer,
            encoder,
            auth: config
                .auth
                .map(|a| a.build(&cx.enrichment_tables))
                .transpose()?,
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
        auth: Option<AuthMatcher>,
        peers: Arc<Mutex<HashMap<SocketAddr, UnboundedSender<Message>>>>,
        mut listener: MaybeTlsListener,
        tripwire: Tripwire,
    ) {
        while let Ok(stream) = listener.accept().await {
            tokio::spawn(Self::handle_connection(
                auth.clone(),
                Arc::clone(&peers),
                stream,
                tripwire.clone(),
            ));
        }
    }

    async fn handle_connection(
        auth: Option<AuthMatcher>,
        peers: Arc<Mutex<HashMap<SocketAddr, UnboundedSender<Message>>>>,
        stream: MaybeTlsIncomingStream<TcpStream>,
        tripwire: Tripwire,
    ) -> Result<(), ()> {
        let addr = stream.peer_addr();
        debug!("Incoming TCP connection from: {}", addr);

        let header_callback = |req: &Request, response: Response| match auth
            .map(|a| a.handle_auth(req.headers()))
            .unwrap_or(Ok(()))
        {
            Ok(_) => Ok(response),
            Err(message) => {
                let mut response = ErrorResponse::default();
                *response.status_mut() = StatusCode::UNAUTHORIZED;
                *response.body_mut() = Some(message.clone());
                debug!("Websocket handshake auth validation failed: {}", message);
                Err(response)
            }
        };

        let ws_stream = tokio_tungstenite::accept_hdr_async(stream, header_callback)
            .await
            .map_err(|err| {
                debug!("Error during websocket handshake: {}", err);
            })?;
        debug!("WebSocket connection established: {}", addr);

        // Insert the write part of this peer to the peer map.
        let (tx, rx) = unbounded();
        peers.lock().unwrap().insert(addr, tx);

        let (outgoing, incoming) = ws_stream.split();

        let broadcast_incoming = incoming.try_for_each(|msg| {
            debug!(
                "Received a message from {}: {}",
                addr,
                msg.to_text().unwrap()
            );

            future::ok(())
        });

        let receive_from_others = rx.map(Ok).forward(outgoing);

        pin_mut!(broadcast_incoming, receive_from_others);
        future::select(broadcast_incoming, receive_from_others).await;
        tripwire.then(crate::shutdown::tripwire_handler).await;

        debug!("{} disconnected", &addr);
        peers.lock().unwrap().remove(&addr);
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
        let (_trigger, tripwire) = Tripwire::new();

        tokio::spawn(Self::handle_connections(
            self.auth,
            Arc::clone(&self.peers),
            listener,
            tripwire,
        ));

        while input.as_mut().peek().await.is_some() {
            let mut event = input.next().await.unwrap();
            let finalizers = event.take_finalizers();

            self.transformer.transform(&mut event);

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

                    let peers = self.peers.lock().unwrap();
                    let broadcast_recipients = peers.iter().map(|(_, ws_sink)| ws_sink);
                    for recp in broadcast_recipients {
                        recp.unbounded_send(message.clone()).unwrap();
                        events_sent.emit(CountByteSize(1, event_byte_size));
                        bytes_sent.emit(ByteSize(message_len));
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
