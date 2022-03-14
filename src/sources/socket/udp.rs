use std::net::SocketAddr;

use bytes::{Bytes, BytesMut};
use chrono::Utc;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tokio_util::codec::FramedRead;
use vector_core::ByteSizeOf;

use crate::{
    codecs::{
        self,
        decoding::{DeserializerConfig, FramingConfig},
        Decoder,
    },
    config::log_schema,
    event::Event,
    internal_events::{
        BytesReceived, SocketEventsReceived, SocketMode, SocketReceiveError, StreamClosedError,
    },
    serde::{default_decoding, default_framing_message_based},
    shutdown::ShutdownSignal,
    sources::{util::StreamDecodingError, Source},
    udp, SourceSender,
};

/// UDP processes messages per packet, where messages are separated by newline.
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct UdpConfig {
    address: SocketAddr,
    #[serde(default = "crate::serde::default_max_length")]
    max_length: usize,
    host_key: Option<String>,
    receive_buffer_bytes: Option<usize>,
    #[serde(default = "default_framing_message_based")]
    framing: FramingConfig,
    #[serde(default = "default_decoding")]
    decoding: DeserializerConfig,
}

impl UdpConfig {
    pub const fn host_key(&self) -> &Option<String> {
        &self.host_key
    }

    pub const fn framing(&self) -> &FramingConfig {
        &self.framing
    }

    pub const fn decoding(&self) -> &DeserializerConfig {
        &self.decoding
    }

    pub const fn address(&self) -> SocketAddr {
        self.address
    }

    pub const fn max_length(&self) -> usize {
        self.max_length
    }

    pub const fn receive_buffer_bytes(&self) -> Option<usize> {
        self.receive_buffer_bytes
    }

    pub fn from_address(address: SocketAddr) -> Self {
        Self {
            address,
            max_length: crate::serde::default_max_length(),
            host_key: None,
            receive_buffer_bytes: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
        }
    }
}

pub fn udp(
    address: SocketAddr,
    max_length: usize,
    host_key: String,
    receive_buffer_bytes: Option<usize>,
    decoder: Decoder,
    mut shutdown: ShutdownSignal,
    mut out: SourceSender,
) -> Source {
    Box::pin(async move {
        let socket = UdpSocket::bind(&address)
            .await
            .expect("Failed to bind to udp listener socket");

        if let Some(receive_buffer_bytes) = receive_buffer_bytes {
            if let Err(error) = udp::set_receive_buffer_size(&socket, receive_buffer_bytes) {
                warn!(message = "Failed configuring receive buffer size on UDP socket.", %error);
            }
        }

        let max_length = if let Some(receive_buffer_bytes) = receive_buffer_bytes {
            std::cmp::min(max_length, receive_buffer_bytes)
        } else {
            max_length
        };

        info!(message = "Listening.", address = %address);

        let mut buf = BytesMut::with_capacity(max_length);
        loop {
            buf.resize(max_length, 0);
            tokio::select! {
                recv = socket.recv_from(&mut buf) => {
                    let (byte_size, address) = recv.map_err(|error| {
                        let error = codecs::decoding::Error::FramingError(error.into());
                        emit!(&SocketReceiveError {
                            mode: SocketMode::Udp,
                            error: &error
                        })
                    })?;

                    emit!(&BytesReceived { byte_size, protocol: "udp" });

                    let payload = buf.split_to(byte_size);

                    let mut stream = FramedRead::new(payload.as_ref(), decoder.clone());

                    while let Some(result) = stream.next().await {
                        match result {
                            Ok((mut events, _byte_size)) => {
                                let count = events.len();
                                emit!(&SocketEventsReceived {
                                    mode: SocketMode::Udp,
                                    byte_size: events.size_of(),
                                    count,
                                });

                                let now = Utc::now();

                                for event in &mut events {
                                    if let Event::Log(ref mut log) = event {
                                        log.try_insert(log_schema().source_type_key(), Bytes::from("socket"));
                                        log.try_insert(log_schema().timestamp_key(), now);
                                        log.try_insert(host_key.as_str(), address.to_string());
                                    }
                                }

                                tokio::select!{
                                    result = out.send_batch(events) => {
                                        if let Err(error) = result {
                                            emit!(&StreamClosedError { error, count });
                                            return Ok(())
                                        }
                                    }
                                    _ = &mut shutdown => return Ok(()),
                                }
                            }
                            Err(error) => {
                                // Error is logged by `crate::codecs::Decoder`, no
                                // further handling is needed here.
                                if !error.can_continue() {
                                    break;
                                }
                            }
                        }
                    }
                }
                _ = &mut shutdown => return Ok(()),
            }
        }
    })
}
