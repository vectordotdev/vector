use std::net::SocketAddr;

use bytes::{Bytes, BytesMut};
use chrono::Utc;
use futures::{SinkExt, StreamExt};
use getset::{CopyGetters, Getters};
use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tokio_util::codec::FramedRead;

use crate::{
    codecs::{
        self,
        decoding::{DeserializerConfig, FramingConfig},
        Decoder,
    },
    config::log_schema,
    event::Event,
    internal_events::{SocketEventsReceived, SocketMode, SocketReceiveError},
    serde::{default_decoding, default_framing_message_based},
    shutdown::ShutdownSignal,
    sources::{util::StreamDecodingError, Source},
    udp, Pipeline,
};

/// UDP processes messages per packet, where messages are separated by newline.
#[derive(Deserialize, Serialize, Debug, Clone, Getters, CopyGetters)]
#[serde(deny_unknown_fields)]
pub struct UdpConfig {
    #[get_copy = "pub"]
    address: SocketAddr,
    #[serde(default = "crate::serde::default_max_length")]
    #[get_copy = "pub"]
    max_length: usize,
    #[get = "pub"]
    host_key: Option<String>,
    #[get_copy = "pub"]
    receive_buffer_bytes: Option<usize>,
    #[serde(default = "default_framing_message_based")]
    #[get = "pub"]
    framing: Box<dyn FramingConfig>,
    #[serde(default = "default_decoding")]
    #[get = "pub"]
    decoding: Box<dyn DeserializerConfig>,
}

impl UdpConfig {
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
    out: Pipeline,
) -> Source {
    let mut out = out.sink_map_err(|error| error!(message = "Error sending event.", %error));

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

                    let payload = buf.split_to(byte_size);

                    let mut stream = FramedRead::new(payload.as_ref(), decoder.clone());

                    loop {
                        match stream.next().await {
                            Some(Ok((events, byte_size))) => {
                                emit!(&SocketEventsReceived {
                                    mode: SocketMode::Udp,
                                    byte_size,
                                    count: events.len()
                                });

                                let now = Utc::now();

                                for mut event in events {
                                    if let Event::Log(ref mut log) = event {
                                        log.try_insert(log_schema().source_type_key(), Bytes::from("socket"));
                                        log.try_insert(log_schema().timestamp_key(), now);
                                        log.try_insert(host_key.clone(), address.to_string());
                                    }

                                    tokio::select!{
                                        result = out.send(event) => {match result {
                                            Ok(()) => { },
                                            Err(()) => return Ok(()),
                                        }}
                                        _ = &mut shutdown => return Ok(()),
                                    }
                                }
                            }
                            Some(Err(error)) => {
                                // Error is logged by `crate::codecs::Decoder`, no
                                // further handling is needed here.
                                if !error.can_continue() {
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                }
                _ = &mut shutdown => return Ok(()),
            }
        }
    })
}
