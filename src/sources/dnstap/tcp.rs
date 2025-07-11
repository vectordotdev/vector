use ipnet::IpNet;
use std::time::Duration;

use bytes::Bytes;
use serde_with::serde_as;
use vector_lib::configurable::configurable_component;
use vector_lib::ipallowlist::IpAllowlistConfig;
use vector_lib::lookup::{owned_value_path, path};
use vector_lib::tcp::TcpKeepaliveConfig;
use vector_lib::tls::{CertificateMetadata, MaybeTlsSettings, TlsSourceConfig};
use vector_lib::EstimatedJsonEncodedSizeOf;
use vrl::path::OwnedValuePath;
use vrl::value::ObjectMap;

use crate::internal_events::{SocketEventsReceived, SocketMode};
use crate::sources::util::framestream::{FrameHandler, TcpFrameHandler};
use crate::{event::Event, sources::util::net::SocketListenAddr};

use vector_lib::config::{LegacyKey, LogNamespace};
use vector_lib::lookup::lookup_v2::OptionalValuePath;

/// TCP configuration for the `dnstap` source.
#[serde_as]
#[configurable_component]
#[derive(Clone, Debug)]
pub struct TcpConfig {
    #[configurable(derived)]
    address: SocketListenAddr,

    #[configurable(derived)]
    keepalive: Option<TcpKeepaliveConfig>,

    /// The timeout before a connection is forcefully closed during shutdown.
    #[serde(default = "default_shutdown_timeout_secs")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[configurable(metadata(docs::human_name = "Shutdown Timeout"))]
    shutdown_timeout_secs: Duration,

    /// Overrides the name of the log field used to add the peer host's port to each event.
    ///
    /// The value will be the peer host's port i.e. `9000`.
    ///
    /// By default, `"port"` is used.
    ///
    /// Set to `""` to suppress this key.
    #[serde(default = "default_port_key")]
    pub port_key: OptionalValuePath,

    #[configurable(derived)]
    permit_origin: Option<IpAllowlistConfig>,

    #[configurable(derived)]
    tls: Option<TlsSourceConfig>,

    /// The size of the receive buffer used for each connection.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    receive_buffer_bytes: Option<usize>,

    /// Maximum duration to keep each connection open. Connections open for longer than this duration are closed.
    ///
    /// This is helpful for load balancing long-lived connections.
    #[configurable(metadata(docs::type_unit = "seconds"))]
    max_connection_duration_secs: Option<u64>,

    /// The maximum number of TCP connections that are allowed at any given time.
    #[configurable(metadata(docs::type_unit = "connections"))]
    pub connection_limit: Option<u32>,
}

const fn default_shutdown_timeout_secs() -> Duration {
    Duration::from_secs(30)
}

fn default_port_key() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("port"))
}

impl TcpConfig {
    pub fn from_address(address: SocketListenAddr) -> Self {
        Self {
            address,
            keepalive: None,
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
            port_key: default_port_key(),
            permit_origin: None,
            tls: None,
            receive_buffer_bytes: None,
            max_connection_duration_secs: None,
            connection_limit: None,
        }
    }

    pub const fn port_key(&self) -> &OptionalValuePath {
        &self.port_key
    }

    pub const fn tls(&self) -> &Option<TlsSourceConfig> {
        &self.tls
    }

    pub const fn address(&self) -> SocketListenAddr {
        self.address
    }

    pub const fn keepalive(&self) -> Option<TcpKeepaliveConfig> {
        self.keepalive
    }

    pub const fn shutdown_timeout_secs(&self) -> Duration {
        self.shutdown_timeout_secs
    }

    pub const fn receive_buffer_bytes(&self) -> Option<usize> {
        self.receive_buffer_bytes
    }

    pub const fn max_connection_duration_secs(&self) -> Option<u64> {
        self.max_connection_duration_secs
    }
}

#[derive(Clone)]
pub struct DnstapFrameHandler<T: FrameHandler + Clone> {
    frame_handler: T,
    address: SocketListenAddr,
    keepalive: Option<TcpKeepaliveConfig>,
    shutdown_timeout_secs: Duration,
    tls: MaybeTlsSettings,
    tls_client_metadata_key: Option<OwnedValuePath>,
    tls_client_metadata: Option<ObjectMap>,
    receive_buffer_bytes: Option<usize>,
    max_connection_duration_secs: Option<u64>,
    max_connections: Option<u32>,
    allowlist: Option<Vec<IpNet>>,
    log_namespace: LogNamespace,
}

impl<T: FrameHandler + Clone> DnstapFrameHandler<T> {
    pub fn new(
        config: TcpConfig,
        tls: MaybeTlsSettings,
        frame_handler: T,
        log_namespace: LogNamespace,
    ) -> Self {
        let tls_client_metadata_key = config
            .tls()
            .as_ref()
            .and_then(|tls| tls.client_metadata_key.clone())
            .and_then(|k| k.path);

        Self {
            frame_handler,
            address: config.address,
            keepalive: config.keepalive,
            shutdown_timeout_secs: config.shutdown_timeout_secs,
            tls,
            tls_client_metadata_key,
            tls_client_metadata: None,
            receive_buffer_bytes: config.receive_buffer_bytes,
            max_connection_duration_secs: config.max_connection_duration_secs,
            max_connections: config.connection_limit,
            allowlist: config.permit_origin.map(Into::into),
            log_namespace,
        }
    }
}

impl<T: FrameHandler + Clone> FrameHandler for DnstapFrameHandler<T> {
    fn content_type(&self) -> String {
        self.frame_handler.content_type()
    }

    fn max_frame_length(&self) -> usize {
        self.frame_handler.max_frame_length()
    }

    /**
     * Function to pass into util::framestream::build_framestream_unix_source
     * Takes a data frame from the unix socket and turns it into a Vector Event.
     **/
    fn handle_event(&self, received_from: Option<Bytes>, frame: Bytes) -> Option<Event> {
        self.frame_handler
            .handle_event(received_from, frame)
            .map(|mut event| {
                if let Event::Log(mut log_event) = event {
                    if let Some(tls_client_metadata) = &self.tls_client_metadata {
                        self.log_namespace.insert_source_metadata(
                            super::DnstapConfig::NAME,
                            &mut log_event,
                            self.tls_client_metadata_key
                                .as_ref()
                                .map(LegacyKey::Overwrite),
                            path!("tls_client_metadata"),
                            tls_client_metadata.clone(),
                        );
                    }

                    emit!(SocketEventsReceived {
                        mode: SocketMode::Tcp,
                        byte_size: log_event.estimated_json_encoded_size_of(),
                        count: 1
                    });

                    event = Event::from(log_event);
                }
                event
            })
    }

    fn multithreaded(&self) -> bool {
        self.frame_handler.multithreaded()
    }

    fn max_frame_handling_tasks(&self) -> u32 {
        self.frame_handler.max_frame_handling_tasks()
    }

    fn host_key(&self) -> &Option<OwnedValuePath> {
        self.frame_handler.host_key()
    }

    fn source_type_key(&self) -> Option<&OwnedValuePath> {
        self.frame_handler.source_type_key()
    }

    fn timestamp_key(&self) -> Option<&OwnedValuePath> {
        self.frame_handler.timestamp_key()
    }
}

impl<T: FrameHandler + Clone> TcpFrameHandler for DnstapFrameHandler<T> {
    fn address(&self) -> SocketListenAddr {
        self.address
    }

    fn keepalive(&self) -> Option<TcpKeepaliveConfig> {
        self.keepalive
    }

    fn shutdown_timeout_secs(&self) -> Duration {
        self.shutdown_timeout_secs
    }

    fn tls(&self) -> MaybeTlsSettings {
        self.tls.clone()
    }

    fn tls_client_metadata_key(&self) -> Option<OwnedValuePath> {
        self.tls_client_metadata_key.clone()
    }

    fn receive_buffer_bytes(&self) -> Option<usize> {
        self.receive_buffer_bytes
    }

    fn max_connection_duration_secs(&self) -> Option<u64> {
        self.max_connection_duration_secs
    }

    fn max_connections(&self) -> Option<u32> {
        self.max_connections
    }

    fn insert_tls_client_metadata(&mut self, metadata: Option<CertificateMetadata>) {
        self.tls_client_metadata = metadata.map(|c| {
            let mut metadata = ObjectMap::new();
            metadata.insert("subject".into(), c.subject().into());
            metadata
        });
    }

    fn allowed_origins(&self) -> Option<&[IpNet]> {
        self.allowlist.as_deref()
    }
}
