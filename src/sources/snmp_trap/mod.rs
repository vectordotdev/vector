use bytes::Bytes;
use futures::StreamExt;
use listenfd::ListenFd;
use tokio_util::codec::BytesCodec;
use tokio_util::udp::UdpFramed;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    config::{LogNamespace, log_schema},
    configurable::configurable_component,
    internal_event::{ByteSize, BytesReceived, InternalEventHandle as _, Protocol},
    lookup::lookup_v2::OptionalValuePath,
};

use crate::{
    SourceSender,
    config::{DataType, GenerateConfig, Resource, SourceConfig, SourceContext, SourceOutput},
    event::Event,
    internal_events::{
        SocketBindError, SocketEventsReceived, SocketMode, SocketReceiveError, StreamClosedError,
    },
    net,
    shutdown::ShutdownSignal,
    sources::util::net::{SocketListenAddr, try_bind_udp_socket},
};

mod parser;

use parser::parse_snmp_trap;

/// Configuration for the `snmp_trap` source.
#[configurable_component(source(
    "snmp_trap",
    "Receive SNMP traps over UDP."
))]
#[derive(Clone, Debug)]
pub struct SnmpTrapConfig {
    /// The address to listen for SNMP traps on.
    ///
    /// SNMP traps are typically sent to UDP port 162.
    #[configurable(metadata(docs::examples = "0.0.0.0:162"))]
    #[configurable(metadata(docs::examples = "127.0.0.1:1162"))]
    address: SocketListenAddr,

    /// The size of the receive buffer used for the listening socket.
    ///
    /// This should not typically need to be changed.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    receive_buffer_bytes: Option<usize>,

    /// Overrides the name of the log field used to add the peer host to each event.
    ///
    /// The value is the peer host's address, including the port. For example, `192.168.1.1:162`.
    ///
    /// By default, the [global `log_schema.host_key` option][global_host_key] is used.
    ///
    /// [global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
    host_key: Option<OptionalValuePath>,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    log_namespace: Option<bool>,
}

impl Default for SnmpTrapConfig {
    fn default() -> Self {
        Self {
            address: SocketListenAddr::SocketAddr("0.0.0.0:162".parse().unwrap()),
            receive_buffer_bytes: None,
            host_key: None,
            log_namespace: None,
        }
    }
}

impl GenerateConfig for SnmpTrapConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self::default()).unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "snmp_trap")]
impl SourceConfig for SnmpTrapConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);
        let host_key = self
            .host_key
            .clone()
            .and_then(|k| k.path)
            .or_else(|| log_schema().host_key().cloned());

        Ok(Box::pin(snmp_trap_udp(
            self.address,
            self.receive_buffer_bytes,
            host_key,
            cx.shutdown,
            log_namespace,
            cx.out,
        )))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let _log_namespace = global_log_namespace.merge(self.log_namespace);

        // Define a simple schema for SNMP trap logs
        let schema_definition = vector_lib::schema::Definition::empty_legacy_namespace()
            .with_standard_vector_source_metadata();

        vec![SourceOutput::new_maybe_logs(
            DataType::Log,
            schema_definition,
        )]
    }

    fn resources(&self) -> Vec<Resource> {
        vec![self.address.as_udp_resource()]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

async fn snmp_trap_udp(
    address: SocketListenAddr,
    receive_buffer_bytes: Option<usize>,
    host_key: Option<vector_lib::lookup::OwnedValuePath>,
    shutdown: ShutdownSignal,
    log_namespace: LogNamespace,
    mut out: SourceSender,
) -> Result<(), ()> {
    let listenfd = ListenFd::from_env();
    let socket = try_bind_udp_socket(address, listenfd)
        .await
        .map_err(|error| {
            emit!(SocketBindError {
                mode: SocketMode::Udp,
                error: &error,
            })
        })?;

    if let Some(receive_buffer_bytes) = receive_buffer_bytes {
        if let Err(error) = net::set_receive_buffer_size(&socket, receive_buffer_bytes) {
            warn!(message = "Failed configuring receive buffer size on UDP socket.", %error);
        }
    }

    info!(
        message = "Listening for SNMP traps.",
        addr = %address,
        r#type = "udp"
    );

    let bytes_received = register!(BytesReceived::from(Protocol::UDP));

    let mut stream = UdpFramed::new(socket, BytesCodec::new())
        .take_until(shutdown)
        .filter_map(|frame| {
            let host_key = host_key.clone();
            let bytes_received = bytes_received.clone();
            async move {
                match frame {
                    Ok((bytes, peer_addr)) => {
                        let byte_size = bytes.len();
                        bytes_received.emit(ByteSize(byte_size));

                        match parse_snmp_trap(&Bytes::from(bytes), peer_addr, log_namespace) {
                            Ok(mut events) => {
                                let count = events.len();
                                emit!(SocketEventsReceived {
                                    mode: SocketMode::Udp,
                                    byte_size: events.estimated_json_encoded_size_of(),
                                    count,
                                });

                                // Add host field if configured
                                for event in &mut events {
                                    if let Event::Log(log) = event {
                                        if let Some(host_key) = &host_key {
                                            log.insert(
                                                (vector_lib::lookup::PathPrefix::Event, host_key),
                                                peer_addr.ip().to_string(),
                                            );
                                        }
                                    }
                                }

                                if events.len() == 1 {
                                    Some(events.remove(0))
                                } else {
                                    // For now, we only return the first event
                                    // In a more complete implementation, we'd handle multiple events
                                    events.into_iter().next()
                                }
                            }
                            Err(error) => {
                                emit!(crate::internal_events::SnmpTrapParseError {
                                    error: format!("{}", error),
                                });
                                None
                            }
                        }
                    }
                    Err(error) => {
                        emit!(SocketReceiveError {
                            mode: SocketMode::Udp,
                            error: &error,
                        });
                        None
                    }
                }
            }
        })
        .boxed();

    match out.send_event_stream(&mut stream).await {
        Ok(()) => {
            debug!("Finished sending.");
            Ok(())
        }
        Err(_) => {
            let (count, _) = stream.size_hint();
            emit!(StreamClosedError { count });
            Err(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::addr::next_addr;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<SnmpTrapConfig>();
    }

    #[tokio::test]
    async fn test_udp_socket_bind() {
        let (_guard, addr) = next_addr();
        let config = SnmpTrapConfig {
            address: SocketListenAddr::SocketAddr(addr),
            receive_buffer_bytes: None,
            host_key: None,
            log_namespace: None,
        };

        let (tx, _rx) = SourceSender::new_test();
        // This should successfully bind
        let source = SourceConfig::build(&config, SourceContext::new_test(tx, None))
            .await
            .expect("Failed to build source");

        // Just verify we can create the source
        drop(source);
    }

    #[tokio::test]
    async fn test_config_default() {
        let config = SnmpTrapConfig::default();
        assert_eq!(
            config.address,
            SocketListenAddr::SocketAddr("0.0.0.0:162".parse().unwrap())
        );
    }

    #[tokio::test]
    async fn test_config_with_options() {
        let (_guard, addr) = next_addr();
        let mut host_path = vector_lib::lookup::OwnedValuePath::root();
        host_path.push_field("host");
        let config = SnmpTrapConfig {
            address: SocketListenAddr::SocketAddr(addr),
            receive_buffer_bytes: Some(65536),
            host_key: Some(OptionalValuePath::from(host_path)),
            log_namespace: None,
        };

        let (tx, _rx) = SourceSender::new_test();
        let source = SourceConfig::build(&config, SourceContext::new_test(tx, None))
            .await
            .expect("Failed to build source with options");

        drop(source);
    }
}
