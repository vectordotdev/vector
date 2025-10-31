use std::net::SocketAddr;

use bytes::Bytes;
use futures::StreamExt;
use listenfd::ListenFd;
use smallvec::SmallVec;
use tokio_util::udp::UdpFramed;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::{BytesDecoder, decoding::{self, Framer}},
    config::{LogNamespace, log_schema},
    configurable::configurable_component,
    internal_event::InternalEventHandle,
    lookup::lookup_v2::OptionalValuePath,
};

use crate::{
    SourceSender,
    codecs::Decoder,
    config::{DataType, GenerateConfig, Resource, SourceConfig, SourceContext, SourceOutput},
    event::Event,
    internal_events::{
        EventsReceived, SocketBindError, SocketBytesReceived, SocketMode, SocketReceiveError,
        StreamClosedError,
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

#[derive(Clone)]
struct SnmpTrapDeserializer {
    events_received: vector_lib::internal_event::Registered<EventsReceived>,
}

impl SnmpTrapDeserializer {
    fn new() -> Self {
        Self {
            events_received: register!(EventsReceived),
        }
    }
}

impl decoding::format::Deserializer for SnmpTrapDeserializer {
    fn parse(
        &self,
        bytes: Bytes,
        _log_namespace: LogNamespace,
    ) -> crate::Result<SmallVec<[Event; 1]>> {
        // Emit bytes received metric
        emit!(SocketBytesReceived {
            mode: SocketMode::Udp,
            byte_size: bytes.len(),
        });

        // We need to get the source address from somewhere, but the decoder doesn't have access to it.
        // For now, we'll use a placeholder and set it properly in the frame handler.
        // This is a limitation of the current codec architecture.
        let dummy_addr: SocketAddr = "0.0.0.0:0".parse().unwrap();

        match parse_snmp_trap(&bytes, dummy_addr) {
            Ok(events) => {
                let count = events.len();
                let byte_size = events.estimated_json_encoded_size_of();
                self.events_received.emit(vector_lib::internal_event::CountByteSize(count, byte_size));
                Ok(events)
            }
            Err(error) => {
                emit!(crate::internal_events::SnmpTrapParseError {
                    error: format!("{}", error),
                });
                // Return empty vec on parse error
                Ok(SmallVec::new())
            }
        }
    }
}

async fn snmp_trap_udp(
    address: SocketListenAddr,
    receive_buffer_bytes: Option<usize>,
    host_key: Option<vector_lib::lookup::OwnedValuePath>,
    shutdown: ShutdownSignal,
    _log_namespace: LogNamespace,
    mut out: SourceSender,
) -> Result<(), ()> {
    let listenfd = ListenFd::from_env();
    let socket = try_bind_udp_socket(address, listenfd)
        .await
        .map_err(|error| {
            emit!(SocketBindError {
                mode: SocketMode::Udp,
                error,
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

    let codec = Decoder::new(
        Framer::Bytes(BytesDecoder::new()),
        decoding::Deserializer::Boxed(Box::new(SnmpTrapDeserializer::new())),
    );

    let mut stream = UdpFramed::new(socket, codec).take_until(shutdown);

    while let Some(frame) = stream.next().await {
        match frame {
            Ok(((mut events, _byte_size), peer_addr)) => {
                // Now we have access to the peer address, so we can set it on the events
                for event in &mut events {
                    if let Event::Log(log) = event {
                        // Override the dummy source_address with the real peer address
                        log.insert("source_address", peer_addr.to_string());

                        // Add host field if configured
                        if let Some(host_key) = &host_key {
                            log.insert(
                                (vector_lib::lookup::PathPrefix::Event, host_key),
                                peer_addr.to_string(),
                            );
                        }
                    }
                }

                let count = events.len();
                if out.send_batch(events).await.is_err() {
                    emit!(StreamClosedError { count });
                }
            }
            Err(error) => {
                emit!(SocketReceiveError {
                    mode: SocketMode::Udp,
                    error: &error,
                });
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{
        components::{assert_source_compliance, SOURCE_TAGS},
        next_addr,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<SnmpTrapConfig>();
    }

    #[tokio::test]
    async fn test_udp_socket_bind() {
        let addr = next_addr();
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
    async fn test_source_compliance() {
        let _result = assert_source_compliance(&SOURCE_TAGS, async {
            let addr = next_addr();
            let mut host_path = vector_lib::lookup::OwnedValuePath::root();
            host_path.push_field("host");
            let config = SnmpTrapConfig {
                address: SocketListenAddr::SocketAddr(addr),
                receive_buffer_bytes: Some(65536),
                host_key: Some(OptionalValuePath::from(host_path)),
                log_namespace: None,
            };

            let (tx, _rx) = SourceSender::new_test();
            SourceConfig::build(&config, SourceContext::new_test(tx, None))
                .await
                .unwrap()
        })
        .await;
    }
}

