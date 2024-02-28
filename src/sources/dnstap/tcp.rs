use ipnet::IpNet;
use std::time::Duration;

use base64::prelude::{Engine as _, BASE64_STANDARD};
use bytes::Bytes;
use serde_with::serde_as;
use vector_lib::configurable::configurable_component;
use vector_lib::internal_event::{
    ByteSize, BytesReceived, InternalEventHandle, Protocol, Registered,
};
use vector_lib::ipallowlist::IpAllowlistConfig;
use vector_lib::lookup::{owned_value_path, path};
use vector_lib::tcp::TcpKeepaliveConfig;
use vector_lib::tls::{CertificateMetadata, MaybeTlsSettings, TlsSourceConfig};
use vector_lib::EstimatedJsonEncodedSizeOf;
use vrl::path::{OwnedValuePath, PathPrefix};
use vrl::value::ObjectMap;

use crate::internal_events::{DnstapParseError, SocketEventsReceived, SocketMode};
use crate::sources::util::framestream::{FrameHandler, TcpFrameHandler};
use crate::{
    event::{Event, LogEvent},
    sources::util::net::SocketListenAddr,
};

use crate::sources::dnstap::parser::DnstapParser;
use crate::sources::dnstap::schema::DNSTAP_VALUE_PATHS;
use vector_lib::config::{log_schema, LegacyKey, LogNamespace};
use vector_lib::lookup::lookup_v2::OptionalValuePath;

/// TCP configuration for the `dnstap` source.
#[serde_as]
#[configurable_component]
#[cfg_attr(unix, serde(tag = "mode", rename_all = "snake_case"))]
#[derive(Clone, Debug)]
pub struct TcpConfig {
    /// Maximum DNSTAP frame length that the source accepts.
    ///
    /// If any frame is longer than this, it is discarded.
    #[serde(default = "default_max_frame_length")]
    #[configurable(metadata(docs::type_unit = "bytes"))]
    pub max_frame_length: usize,

    #[configurable(derived)]
    address: SocketListenAddr,

    #[configurable(derived)]
    keepalive: Option<TcpKeepaliveConfig>,

    /// The timeout before a connection is forcefully closed during shutdown.
    #[serde(default = "default_shutdown_timeout_secs")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[configurable(metadata(docs::human_name = "Shutdown Timeout"))]
    shutdown_timeout_secs: Duration,

    /// Overrides the name of the log field used to add the source path to each event.
    ///
    /// The value is the socket path itself.
    ///
    /// By default, the [global `log_schema.host_key` option][global_host_key] is used.
    ///
    /// [global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
    pub host_key: Option<OptionalValuePath>,

    /// Overrides the name of the log field used to add the peer host's port to each event.
    ///
    /// The value will be the peer host's port i.e. `9000`.
    ///
    /// By default, `"port"` is used.
    ///
    /// Set to `""` to suppress this key.
    #[serde(default = "default_port_key")]
    pub port_key: OptionalValuePath,

    /// List of allowed origin IP networks
    ///
    /// By default, no origin is allowed
    permit_origin: IpAllowlistConfig,

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

    /// Whether or not to skip parsing or decoding of DNSTAP frames.
    ///
    /// If set to `true`, frames are not parsed or decoded. The raw frame data is set as a field on the event
    /// (called `rawData`) and encoded as a base64 string.
    pub raw_data_only: Option<bool>,

    /// Whether or not to concurrently process DNSTAP frames.
    pub multithreaded: Option<bool>,

    /// Maximum number of frames that can be processed concurrently.
    pub max_frame_handling_tasks: Option<u32>,

    /// The namespace to use for logs. This overrides the global settings.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,
}

fn default_max_frame_length() -> usize {
    bytesize::kib(100u64) as usize
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
            max_frame_length: default_max_frame_length(),
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
            host_key: None,
            port_key: default_port_key(),
            raw_data_only: None,
            multithreaded: None,
            max_frame_handling_tasks: None,
            permit_origin: IpAllowlistConfig(Vec::new()),
            tls: None,
            receive_buffer_bytes: None,
            max_connection_duration_secs: None,
            connection_limit: None,
            log_namespace: None,
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
pub struct DnstapFrameHandler {
    max_frame_length: usize,
    content_type: String,
    raw_data_only: bool,
    multithreaded: bool,
    address: SocketListenAddr,
    keepalive: Option<TcpKeepaliveConfig>,
    shutdown_timeout_secs: Duration,
    tls: MaybeTlsSettings,
    tls_client_metadata_key: Option<OwnedValuePath>,
    tls_client_metadata: Option<ObjectMap>,
    receive_buffer_bytes: Option<usize>,
    max_connection_duration_secs: Option<u64>,
    max_connections: Option<u32>,
    max_frame_handling_tasks: u32,
    allowlist: Vec<IpNet>,
    host_key: Option<OwnedValuePath>,
    timestamp_key: Option<OwnedValuePath>,
    source_type_key: Option<OwnedValuePath>,
    bytes_received: Registered<BytesReceived>,
    log_namespace: LogNamespace,
}

impl DnstapFrameHandler {
    pub fn new(config: TcpConfig, tls: MaybeTlsSettings, log_namespace: LogNamespace) -> Self {
        let source_type_key = log_schema().source_type_key();
        let timestamp_key = log_schema().timestamp_key();
        let tls_client_metadata_key = config
            .tls()
            .as_ref()
            .and_then(|tls| tls.client_metadata_key.clone())
            .and_then(|k| k.path);

        let host_key = config
            .host_key
            .clone()
            .map_or(log_schema().host_key().cloned(), |k| k.path);
        Self {
            max_frame_length: config.max_frame_length,
            content_type: "protobuf:dnstap.Dnstap".to_string(),
            raw_data_only: config.raw_data_only.unwrap_or(false),
            multithreaded: config.multithreaded.unwrap_or(false),
            max_frame_handling_tasks: config.max_frame_handling_tasks.unwrap_or(1000),
            address: config.address,
            keepalive: config.keepalive,
            shutdown_timeout_secs: config.shutdown_timeout_secs,
            tls,
            tls_client_metadata_key,
            tls_client_metadata: None,
            receive_buffer_bytes: config.receive_buffer_bytes,
            max_connection_duration_secs: config.max_connection_duration_secs,
            max_connections: config.connection_limit,
            allowlist: config.permit_origin.0.iter().map(|net| net.0).collect(),
            host_key,
            timestamp_key: timestamp_key.cloned(),
            source_type_key: source_type_key.cloned(),
            bytes_received: register!(BytesReceived::from(Protocol::from("protobuf"))),
            log_namespace,
        }
    }
}

impl FrameHandler for DnstapFrameHandler {
    fn content_type(&self) -> String {
        self.content_type.clone()
    }

    fn max_frame_length(&self) -> usize {
        self.max_frame_length
    }

    /**
     * Function to pass into util::framestream::build_framestream_unix_source
     * Takes a data frame from the unix socket and turns it into a Vector Event.
     **/
    fn handle_event(&self, received_from: Option<Bytes>, frame: Bytes) -> Option<Event> {
        self.bytes_received.emit(ByteSize(frame.len()));

        let mut log_event = LogEvent::default();

        if let Some(host) = received_from {
            self.log_namespace.insert_source_metadata(
                super::DnstapConfig::NAME,
                &mut log_event,
                self.host_key.as_ref().map(LegacyKey::Overwrite),
                path!("host"),
                host,
            );
        }

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

        if self.raw_data_only {
            log_event.insert(
                (PathPrefix::Event, &DNSTAP_VALUE_PATHS.raw_data),
                BASE64_STANDARD.encode(&frame),
            );
        } else if let Err(err) = DnstapParser::parse(&mut log_event, frame) {
            emit!(DnstapParseError {
                error: format!("Dnstap protobuf decode error {:?}.", err)
            });
            return None;
        }

        emit!(SocketEventsReceived {
            mode: SocketMode::Tcp,
            byte_size: log_event.estimated_json_encoded_size_of(),
            count: 1
        });

        if self.log_namespace == LogNamespace::Vector {
            // The timestamp is inserted by the parser which caters for the Legacy namespace.
            self.log_namespace.insert_vector_metadata(
                &mut log_event,
                self.timestamp_key(),
                path!("ingest_timestamp"),
                chrono::Utc::now(),
            );
        }

        self.log_namespace.insert_vector_metadata(
            &mut log_event,
            self.source_type_key(),
            path!("source_type"),
            super::DnstapConfig::NAME,
        );

        Some(Event::from(log_event))
    }

    fn multithreaded(&self) -> bool {
        self.multithreaded
    }

    fn max_frame_handling_tasks(&self) -> u32 {
        self.max_frame_handling_tasks
    }

    fn host_key(&self) -> &Option<OwnedValuePath> {
        &self.host_key
    }

    fn source_type_key(&self) -> Option<&OwnedValuePath> {
        self.source_type_key.as_ref()
    }

    fn timestamp_key(&self) -> Option<&OwnedValuePath> {
        self.timestamp_key.as_ref()
    }
}

impl TcpFrameHandler for DnstapFrameHandler {
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

    fn allowed_origins(&self) -> &[IpNet] {
        &self.allowlist
    }
}
