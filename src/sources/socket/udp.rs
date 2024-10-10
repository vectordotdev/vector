use super::default_host_key;
use bytes::BytesMut;
use chrono::Utc;
use futures::StreamExt;
use listenfd::ListenFd;
use tokio_util::codec::FramedRead;
use vector_lib::codecs::{
    decoding::{DeserializerConfig, FramingConfig},
    StreamDecodingError,
};
use vector_lib::configurable::configurable_component;
use vector_lib::internal_event::{ByteSize, BytesReceived, InternalEventHandle as _, Protocol};
use vector_lib::lookup::{lookup_v2::OptionalValuePath, owned_value_path, path};
use vector_lib::{
    config::{LegacyKey, LogNamespace},
    EstimatedJsonEncodedSizeOf,
};

use crate::{
    codecs::Decoder,
    event::Event,
    internal_events::{
        SocketBindError, SocketEventsReceived, SocketMode, SocketReceiveError, StreamClosedError,
    },
    net,
    serde::default_decoding,
    shutdown::ShutdownSignal,
    sources::{
        socket::SocketConfig,
        util::net::{try_bind_udp_socket, SocketListenAddr},
        Source,
    },
    SourceSender,
};

/// UDP configuration for the `socket` source.
#[configurable_component]
#[serde(deny_unknown_fields)]
#[derive(Clone, Debug)]
pub struct UdpConfig {
    #[configurable(derived)]
    address: SocketListenAddr,

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

    #[configurable(derived)]
    pub(super) framing: Option<FramingConfig>,

    #[configurable(derived)]
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

fn default_max_length() -> usize {
    crate::serde::default_max_length()
}

impl UdpConfig {
    pub(super) fn host_key(&self) -> OptionalValuePath {
        self.host_key.clone().unwrap_or(default_host_key())
    }

    pub const fn port_key(&self) -> &OptionalValuePath {
        &self.port_key
    }

    pub(super) const fn framing(&self) -> &Option<FramingConfig> {
        &self.framing
    }

    pub(super) const fn decoding(&self) -> &DeserializerConfig {
        &self.decoding
    }

    pub(super) const fn address(&self) -> SocketListenAddr {
        self.address
    }

    pub fn from_address(address: SocketListenAddr) -> Self {
        Self {
            address,
            max_length: default_max_length(),
            host_key: None,
            port_key: default_port_key(),
            receive_buffer_bytes: None,
            framing: None,
            decoding: default_decoding(),
            log_namespace: None,
        }
    }

    pub fn set_log_namespace(&mut self, val: Option<bool>) -> &mut Self {
        self.log_namespace = val;
        self
    }
}

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
                emit!(SocketBindError {
                    mode: SocketMode::Udp,
                    error,
                })
            })?;

        if let Some(receive_buffer_bytes) = config.receive_buffer_bytes {
            if let Err(error) = net::set_receive_buffer_size(&socket, receive_buffer_bytes) {
                warn!(message = "Failed configuring receive buffer size on UDP socket.", %error);
            }
        }

        let mut max_length = config.max_length;

        if let Some(receive_buffer_bytes) = config.receive_buffer_bytes {
            max_length = std::cmp::min(max_length, receive_buffer_bytes);
        }

        let bytes_received = register!(BytesReceived::from(Protocol::UDP));

        info!(message = "Listening.", address = %config.address);
        // We add 1 to the max_length in order to determine if the received data has been truncated.
        let mut buf = BytesMut::with_capacity(max_length + 1);
        loop {
            buf.resize(max_length + 1, 0);
            tokio::select! {
                recv = socket.recv_from(&mut buf) => {
                    let (byte_size, address) = match recv {
                        Ok(res) => res,
                        Err(error) => {
                            #[cfg(windows)]
                            if let Some(err) = error.raw_os_error() {
                                if err == 10040 {
                                    // 10040 is the Windows error that the Udp message has exceeded max_length
                                    warn!(
                                        message = "Discarding frame larger than max_length.",
                                        max_length = max_length,
                                        internal_log_rate_limit = true
                                    );
                                    continue;
                                }
                            }

                            return Err(emit!(SocketReceiveError {
                                mode: SocketMode::Udp,
                                error
                            }));
                       }
                    };

                    bytes_received.emit(ByteSize(byte_size));
                    let payload = buf.split_to(byte_size);
                    let truncated = byte_size == max_length + 1;
                    let mut stream = FramedRead::new(payload.as_ref(), decoder.clone()).peekable();

                    while let Some(result) = stream.next().await {
                        let last = Pin::new(&mut stream).peek().await.is_none();
                        match result {
                            Ok((mut events, _byte_size)) => {
                                if last && truncated {
                                    // The last event in this payload was truncated, so we want to drop it.
                                    _ = events.pop();
                                    warn!(
                                        message = "Discarding frame larger than max_length.",
                                        max_length = max_length,
                                        internal_log_rate_limit = true
                                    );
                                }

                                if events.is_empty() {
                                    continue;
                                }

                                let count = events.len();
                                emit!(SocketEventsReceived {
                                    mode: SocketMode::Udp,
                                    byte_size: events.estimated_json_encoded_size_of(),
                                    count,
                                });

                                let now = Utc::now();

                                for event in &mut events {
                                    if let Event::Log(ref mut log) = event {
                                        log_namespace.insert_standard_vector_source_metadata(
                                            log,
                                            SocketConfig::NAME,
                                            now,
                                        );

                                        let legacy_host_key = config
                                            .host_key
                                            .clone()
                                            .unwrap_or(default_host_key())
                                            .path;

                                        log_namespace.insert_source_metadata(
                                            SocketConfig::NAME,
                                            log,
                                            legacy_host_key.as_ref().map(LegacyKey::InsertIfEmpty),
                                            path!("host"),
                                            address.ip().to_string()
                                        );

                                        let legacy_port_key = config.port_key.clone().path;

                                        log_namespace.insert_source_metadata(
                                            SocketConfig::NAME,
                                            log,
                                            legacy_port_key.as_ref().map(LegacyKey::InsertIfEmpty),
                                            path!("port"),
                                            address.port()
                                        );
                                    }
                                }

                                tokio::select!{
                                    result = out.send_batch(events) => {
                                        if result.is_err() {
                                            emit!(StreamClosedError { count });
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
