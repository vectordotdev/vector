use std::path::PathBuf;

use base64::prelude::{Engine as _, BASE64_STANDARD};
use bytes::Bytes;
use vector_lib::configurable::configurable_component;
use vector_lib::internal_event::{
    ByteSize, BytesReceived, InternalEventHandle as _, Protocol, Registered,
};
use vector_lib::lookup::{path, OwnedValuePath};
use vrl::path::PathPrefix;

use crate::sources::util::framestream::FrameHandler;
use crate::{
    config::log_schema,
    event::{Event, LogEvent},
    internal_events::{DnstapParseError, SocketEventsReceived, SocketMode},
    sources::util::framestream::UnixFrameHandler,
};

pub use super::schema::DnstapEventSchema;
use crate::sources::dnstap::parser::DnstapParser;
use crate::sources::dnstap::schema::DNSTAP_VALUE_PATHS;
use vector_lib::lookup::lookup_v2::OptionalValuePath;
use vector_lib::{
    config::{LegacyKey, LogNamespace},
    EstimatedJsonEncodedSizeOf,
};

/// Unix domain socket configuration for the `dnstap` source.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct UnixConfig {
    /// Maximum DNSTAP frame length that the source accepts.
    ///
    /// If any frame is longer than this, it is discarded.
    #[serde(default = "default_max_frame_length")]
    #[configurable(metadata(docs::type_unit = "bytes"))]
    pub max_frame_length: usize,

    /// Overrides the name of the log field used to add the source path to each event.
    ///
    /// The value is the socket path itself.
    ///
    /// By default, the [global `log_schema.host_key` option][global_host_key] is used.
    ///
    /// [global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
    pub host_key: Option<OptionalValuePath>,

    /// Absolute path to the socket file to read DNSTAP data from.
    ///
    /// The DNS server must be configured to send its DNSTAP data to this socket file. The socket file is created
    /// if it doesn't already exist when the source first starts.
    pub socket_path: PathBuf,

    /// Whether or not to skip parsing or decoding of DNSTAP frames.
    ///
    /// If set to `true`, frames are not parsed or decoded. The raw frame data is set as a field on the event
    /// (called `rawData`) and encoded as a base64 string.
    pub raw_data_only: Option<bool>,

    /// Whether or not to concurrently process DNSTAP frames.
    pub multithreaded: Option<bool>,

    /// Maximum number of frames that can be processed concurrently.
    pub max_frame_handling_tasks: Option<u32>,

    /// Unix file mode bits to be applied to the unix socket file as its designated file permissions.
    ///
    /// Note: The file mode value can be specified in any numeric format supported by your configuration
    /// language, but it is most intuitive to use an octal number.
    pub socket_file_mode: Option<u32>,

    /// The size, in bytes, of the receive buffer used for the socket.
    ///
    /// This should not typically needed to be changed.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    pub socket_receive_buffer_size: Option<usize>,

    /// The size, in bytes, of the send buffer used for the socket.
    ///
    /// This should not typically needed to be changed.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    pub socket_send_buffer_size: Option<usize>,

    /// The namespace to use for logs. This overrides the global settings.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,
}

fn default_max_frame_length() -> usize {
    bytesize::kib(100u64) as usize
}

impl UnixConfig {
    pub fn new(socket_path: PathBuf) -> Self {
        Self {
            host_key: None,
            socket_path,
            ..Self::default()
        }
    }

    fn content_type(&self) -> String {
        "protobuf:dnstap.Dnstap".to_string() //content-type for framestream
    }
}

impl Default for UnixConfig {
    fn default() -> Self {
        Self {
            host_key: None,
            max_frame_length: default_max_frame_length(),
            socket_path: PathBuf::from("/run/bind/dnstap.sock"),
            raw_data_only: None,
            multithreaded: None,
            max_frame_handling_tasks: None,
            socket_file_mode: None,
            socket_receive_buffer_size: None,
            socket_send_buffer_size: None,
            log_namespace: None,
        }
    }
}

#[derive(Clone)]
pub struct DnstapFrameHandler {
    max_frame_length: usize,
    socket_path: PathBuf,
    content_type: String,
    raw_data_only: bool,
    multithreaded: bool,
    max_frame_handling_tasks: u32,
    socket_file_mode: Option<u32>,
    socket_receive_buffer_size: Option<usize>,
    socket_send_buffer_size: Option<usize>,
    host_key: Option<OwnedValuePath>,
    timestamp_key: Option<OwnedValuePath>,
    source_type_key: Option<OwnedValuePath>,
    bytes_received: Registered<BytesReceived>,
    log_namespace: LogNamespace,
}

impl DnstapFrameHandler {
    pub fn new(config: UnixConfig, log_namespace: LogNamespace) -> Self {
        let source_type_key = log_schema().source_type_key();
        let timestamp_key = log_schema().timestamp_key();

        let host_key = config
            .host_key
            .clone()
            .map_or(log_schema().host_key().cloned(), |k| k.path);

        Self {
            max_frame_length: config.max_frame_length,
            socket_path: config.socket_path.clone(),
            content_type: config.content_type(),
            raw_data_only: config.raw_data_only.unwrap_or(false),
            multithreaded: config.multithreaded.unwrap_or(false),
            max_frame_handling_tasks: config.max_frame_handling_tasks.unwrap_or(1000),
            socket_file_mode: config.socket_file_mode,
            socket_receive_buffer_size: config.socket_receive_buffer_size,
            socket_send_buffer_size: config.socket_send_buffer_size,
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
            mode: SocketMode::Unix,
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

impl UnixFrameHandler for DnstapFrameHandler {
    fn socket_path(&self) -> PathBuf {
        self.socket_path.clone()
    }

    fn socket_file_mode(&self) -> Option<u32> {
        self.socket_file_mode
    }

    fn socket_receive_buffer_size(&self) -> Option<usize> {
        self.socket_receive_buffer_size
    }

    fn socket_send_buffer_size(&self) -> Option<usize> {
        self.socket_send_buffer_size
    }
}
