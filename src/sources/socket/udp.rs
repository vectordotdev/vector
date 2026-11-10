use std::{
    io,
    net::{Ipv4Addr, SocketAddr},
    pin::Pin,
};
use vector_lib::internal_event::{emit, register};

use chrono::Utc;
use futures::StreamExt;
use listenfd::ListenFd;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::{
        DecoderFramedRead, StreamDecodingError,
        decoding::{DeserializerConfig, FramingConfig},
    },
    config::{LegacyKey, LogNamespace},
    configurable::configurable_component,
    internal_event::{ByteSize, BytesReceived, InternalEventHandle as _, Protocol},
    lookup::{lookup_v2::OptionalValuePath, owned_value_path, path},
    source_sender::chunk_size_events,
};

use super::default_host_key;
use crate::{
    SourceSender,
    codecs::Decoder,
    event::Event,
    internal_events::{
        SocketBindError, SocketEventsReceived, SocketMode, SocketMulticastGroupJoinError,
        SocketOversizedDatagram, SocketReceiveError, StreamClosedError,
    },
    net,
    serde::default_decoding,
    shutdown::ShutdownSignal,
    sources::SourceError,
    sources::{
        Source,
        socket::SocketConfig,
        util::net::{SocketListenAddr, UdpBatchReceiver, try_bind_udp_socket},
    },
};

/// UDP configuration for the `socket` source.
#[configurable_component]
#[serde(deny_unknown_fields)]
#[derive(Clone, Debug)]
pub struct UdpConfig {
    address: SocketListenAddr,

    /// List of IPv4 multicast groups to join on socket's binding process.
    ///
    /// In order to read multicast packets, this source's listening address should be set to `0.0.0.0`.
    /// If any other address is used (such as `127.0.0.1` or an specific interface address), the
    /// listening interface will filter out all multicast packets received,
    /// as their target IP would be the one of the multicast group
    /// and it will not match the socket's bound IP.
    ///
    /// Note that this setting will only work if the source's address
    /// is an IPv4 address (IPv6 and systemd file descriptor as source's address are not supported
    /// with multicast groups).
    #[serde(default)]
    #[configurable(metadata(docs::examples = "['224.0.0.2', '224.0.0.4']"))]
    pub(super) multicast_groups: Vec<Ipv4Addr>,

    /// The IPv4 interface address used when joining multicast groups.
    ///
    /// Specifies which local network interface to use for receiving multicast traffic.
    /// When not set, defaults to the socket's binding address.
    ///
    /// Set this explicitly when the host has multiple interfaces and you need to control
    /// which one receives multicast traffic. For example, `127.0.0.1` restricts multicast
    /// reception to the loopback interface.
    ///
    /// On macOS, specifying `0.0.0.0` only joins on the default network interface (typically
    /// the primary Ethernet or Wi-Fi interface), unlike Linux, which joins on all interfaces.
    /// If multicast traffic is expected on a specific interface (including loopback), set this
    /// field explicitly.
    #[serde(default)]
    pub(super) multicast_interface: Option<Ipv4Addr>,

    /// The maximum buffer size of incoming messages.
    ///
    /// Messages larger than this are truncated.
    #[serde(default = "default_max_length")]
    #[configurable(metadata(docs::type_unit = "bytes"))]
    pub(super) max_length: usize,

    /// Overrides the name of the log field used to add the peer host to each event.
    ///
    /// The value will be the peer host's address, including the port i.e. `1.2.3.4:9000`.
    ///
    /// By default, the [global `log_schema.host_key` option][global_host_key] is used.
    ///
    /// Set to `""` to suppress this key.
    ///
    /// [global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
    host_key: Option<OptionalValuePath>,

    /// Overrides the name of the log field used to add the peer host's port to each event.
    ///
    /// The value will be the peer host's port i.e. `9000`.
    ///
    /// By default, `"port"` is used.
    ///
    /// Set to `""` to suppress this key.
    #[serde(default = "default_port_key")]
    port_key: OptionalValuePath,

    /// The size of the receive buffer used for the listening socket.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    receive_buffer_bytes: Option<usize>,

    pub(super) framing: Option<FramingConfig>,

    #[serde(default = "default_decoding")]
    pub(super) decoding: DeserializerConfig,

    /// The namespace to use for logs. This overrides the global setting.
    #[serde(default)]
    #[configurable(metadata(docs::hidden))]
    pub log_namespace: Option<bool>,
}

fn default_port_key() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("port"))
}

const fn default_max_length() -> usize {
    crate::serde::default_max_length()
}

impl UdpConfig {
    pub(super) fn host_key(&self) -> OptionalValuePath {
        self.host_key.clone().unwrap_or(default_host_key())
    }

    #[must_use]
    pub const fn port_key(&self) -> &OptionalValuePath {
        &self.port_key
    }

    pub(super) const fn framing(&self) -> Option<&FramingConfig> {
        self.framing.as_ref()
    }

    pub(super) const fn decoding(&self) -> &DeserializerConfig {
        &self.decoding
    }

    pub(super) const fn address(&self) -> SocketListenAddr {
        self.address
    }

    #[must_use]
    pub fn from_address(address: SocketListenAddr) -> Self {
        Self {
            address,
            multicast_groups: Vec::new(),
            multicast_interface: None,
            max_length: default_max_length(),
            host_key: None,
            port_key: default_port_key(),
            receive_buffer_bytes: None,
            framing: None,
            decoding: default_decoding(),
            log_namespace: None,
        }
    }

    pub const fn set_log_namespace(&mut self, val: Option<bool>) -> &mut Self {
        self.log_namespace = val;
        self
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "Splitting this source routine is deferred to keep the lint cleanup mechanical. https://github.com/vectordotdev/vector/issues/23659"
)]
pub(super) fn udp(
    config: UdpConfig,
    decoder: Decoder,
    mut shutdown: ShutdownSignal,
    mut out: SourceSender,
    log_namespace: LogNamespace,
) -> Source {
    Box::pin(async move {
        let listenfd = ListenFd::from_env();
        let socket = try_bind_udp_socket(config.address, listenfd)
            .await
            .map_err(|error| {
                emit(SocketBindError {
                    mode: SocketMode::Udp,
                    error: &error,
                });
                SourceError::from(error)
            })?;

        if !config.multicast_groups.is_empty() {
            socket.set_multicast_loop_v4(true).unwrap();
            let listen_addr = match config.address() {
                SocketListenAddr::SocketAddr(SocketAddr::V4(addr)) => addr,
                SocketListenAddr::SocketAddr(SocketAddr::V6(_)) => {
                    // We could support Ipv6 multicast with the
                    // https://doc.rust-lang.org/std/net/struct.UdpSocket.html#method.join_multicast_v6 method
                    // and specifying the interface index as `0`, in order to bind all interfaces.
                    unimplemented!("IPv6 multicast is not supported")
                }
                SocketListenAddr::SystemdFd(_) => {
                    unimplemented!("Multicast for systemd fd sockets is not supported")
                }
            };
            for group_addr in config.multicast_groups {
                let interface = config.multicast_interface.unwrap_or(*listen_addr.ip());
                socket
                    .join_multicast_v4(group_addr, interface)
                    .map_err(|error| {
                        emit(SocketMulticastGroupJoinError {
                            error: &error,
                            group_addr,
                            interface,
                        });
                        SourceError::from(error)
                    })?;
                info!(message = "Joined multicast group.", group = %group_addr);
            }
        }

        if let Some(receive_buffer_bytes) = config.receive_buffer_bytes
            && let Err(error) = net::set_receive_buffer_size(&socket, receive_buffer_bytes)
        {
            warn!(message = "Failed configuring receive buffer size on UDP socket.", %error);
        }

        let mut max_length = config.max_length;

        if let Some(receive_buffer_bytes) = config.receive_buffer_bytes {
            max_length = std::cmp::min(max_length, receive_buffer_bytes);
        }

        // The receiver handles platform-specific over-length detection internally. Pass the
        // configured limit unchanged so a datagram of `max_length + 1` is rejected.
        let mut receiver = UdpBatchReceiver::from_socket(socket, max_length).map_err(|error| {
            emit(SocketReceiveError {
                mode: SocketMode::Udp,
                error: &error,
            });
            SourceError::from(error)
        })?;

        let bytes_received = register(BytesReceived::from(Protocol::UDP));
        let send_batch_size = chunk_size_events().max(1);
        let legacy_host_key = config.host_key.clone().unwrap_or(default_host_key()).path;
        let legacy_port_key = config.port_key.clone().path;

        info!(message = "Listening.", address = %config.address);
        loop {
            let batch = tokio::select! {
                batch_result = receiver.recv_batch() => match batch_result {
                    Ok(batch) => batch,
                    Err(error) => {
                        #[cfg(windows)]
                        if error.raw_os_error() == Some(10040) {
                            emit(SocketOversizedDatagram {
                                mode: SocketMode::Udp,
                            });
                            warn!(
                                message = "Discarding frame larger than max_length.",
                                max_length
                            );
                            continue;
                        }

                        emit(SocketReceiveError {
                            mode: SocketMode::Udp,
                            error: &error,
                        });
                        return Err(SourceError::from(error));
                    }
                },
                _ = &mut shutdown => return Ok(()),
            };

            bytes_received.emit(ByteSize(batch.received_bytes()));
            let mut output = Vec::with_capacity(batch.len().min(send_batch_size));
            for datagram in batch.iter() {
                if datagram.ancillary_truncated {
                    let error = io::Error::new(
                        io::ErrorKind::InvalidData,
                        "UDP ancillary data was truncated",
                    );
                    emit(SocketReceiveError {
                        mode: SocketMode::Udp,
                        error: &error,
                    });
                    continue;
                }
                if datagram.truncated {
                    emit(SocketOversizedDatagram {
                        mode: SocketMode::Udp,
                    });
                }
                let address = datagram.address;
                let source_host = address.ip().to_string();
                let now = Utc::now();
                let mut stream =
                    DecoderFramedRead::new(datagram.payload, decoder.clone()).peekable();

                while let Some(result) = stream.next().await {
                    let last = Pin::new(&mut stream).peek().await.is_none();
                    match result {
                        Ok((mut events, _byte_size)) => {
                            if last && datagram.truncated {
                                // Keep complete frames at the start of the datagram, but discard
                                // the final frame because it may have been cut at max_length.
                                _ = events.pop();
                                warn!(
                                    message = "Discarding frame larger than max_length.",
                                    max_length
                                );
                            }

                            if events.is_empty() {
                                continue;
                            }

                            let count = events.len();
                            emit(SocketEventsReceived {
                                mode: SocketMode::Udp,
                                byte_size: events.estimated_json_encoded_size_of(),
                                count,
                            });

                            for event in &mut events {
                                if let Event::Log(log) = event {
                                    log_namespace.insert_standard_vector_source_metadata(
                                        log,
                                        SocketConfig::NAME,
                                        now,
                                    );
                                    log_namespace.insert_source_metadata(
                                        SocketConfig::NAME,
                                        log,
                                        legacy_host_key.as_ref().map(LegacyKey::InsertIfEmpty),
                                        path!("host"),
                                        source_host.clone(),
                                    );
                                    log_namespace.insert_source_metadata(
                                        SocketConfig::NAME,
                                        log,
                                        legacy_port_key.as_ref().map(LegacyKey::InsertIfEmpty),
                                        path!("port"),
                                        address.port(),
                                    );
                                }
                            }
                            for event in events {
                                output.push(event);
                                if output.len() >= send_batch_size
                                    && !send_buffered_events(&mut out, &mut output).await
                                {
                                    return Ok(());
                                }
                            }
                        }
                        Err(error) => {
                            // Error is logged by `vector_lib::codecs::Decoder`, no further
                            // handling is needed here.
                            if !error.can_continue() {
                                break;
                            }
                        }
                    }
                }
            }

            drop(batch);
            if output.is_empty() {
                continue;
            }

            if !send_buffered_events(&mut out, &mut output).await {
                return Ok(());
            }
        }
    })
}

async fn send_buffered_events(out: &mut SourceSender, output: &mut Vec<Event>) -> bool {
    let events = std::mem::take(output);
    let count = events.len();
    debug_assert!(count > 0);

    // Drain decoded events during graceful shutdown. The topology enforces the deadline.
    if out.send_batch(events).await.is_err() {
        emit(StreamClosedError { count });
        false
    } else {
        true
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures::StreamExt;
    use tokio::{
        net::UdpSocket,
        sync::Notify,
        time::{Duration, timeout},
    };
    use vector_lib::{
        event::{EventContainer, LogEvent},
        source_sender::PostProcessor,
    };

    use super::*;
    use crate::config::{SourceConfig, SourceContext};

    struct NotifyOnSend(Notify);

    impl PostProcessor for NotifyOnSend {
        fn process_log(&self, _: &mut LogEvent) {
            self.0.notify_one();
        }
    }

    #[tokio::test]
    async fn shutdown_drains_backpressured_batch() {
        let (mut out, mut output) = SourceSender::new_test_sender_with_options(1, None);
        out.send_event(crate::event::LogEvent::from("buffered"))
            .await
            .unwrap();
        let sending = Arc::new(NotifyOnSend(Notify::new()));
        out.set_post_processor(&(Arc::clone(&sending) as Arc<dyn PostProcessor>));
        let (stop, shutdown, _done) = ShutdownSignal::new_wired();
        let (_guard, address) = crate::test_util::addr::next_addr();
        let config = SocketConfig::from(UdpConfig::from_address(address.into()));
        let mut context = SourceContext::new_test(out, None);
        context.shutdown = shutdown;
        let mut source = config.build(context).await.unwrap();
        // Poll once to bind the socket before sending.
        assert!(futures::poll!(&mut source).is_pending());
        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        sender.send_to(b"pending", address).await.unwrap();
        timeout(Duration::from_secs(5), async {
            tokio::select! {
                result = &mut source => panic!("source exited before sending: {result:?}"),
                () = sending.0.notified() => {},
            }
        })
        .await
        .unwrap();

        stop.cancel();
        assert!(futures::poll!(&mut source).is_pending());
        // Free the occupied output slot only after shutdown has been observed.
        output.next().await.unwrap();
        let (result, events) = timeout(Duration::from_secs(5), async {
            tokio::join!(source, async {
                output
                    .into_stream()
                    .flat_map(|item| futures::stream::iter(item.events.into_events()))
                    .collect::<Vec<_>>()
                    .await
            })
        })
        .await
        .unwrap();
        result.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].as_log()[crate::config::log_schema()
                .message_key()
                .unwrap()
                .to_string()],
            "pending".into()
        );
    }
}
