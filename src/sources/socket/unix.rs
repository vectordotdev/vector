use std::path::PathBuf;

use bytes::Bytes;
use chrono::Utc;
use codecs::decoding::{DeserializerConfig, FramingConfig};
use vector_config::configurable_component;

use crate::{
    codecs::Decoder,
    config::log_schema,
    event::Event,
    serde::default_decoding,
    shutdown::ShutdownSignal,
    sources::{
        util::{build_unix_datagram_source, build_unix_stream_source},
        Source,
    },
    SourceSender,
};

/// Unix domain socket configuration for the `socket` source.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct UnixConfig {
    /// The Unix socket path.
    ///
    /// This should be an absolute path.
    pub path: PathBuf,

    /// Unix file mode bits to be applied to the unix socket file as its designated file permissions.
    ///
    /// Note that the file mode value can be specified in any numeric format supported by your configuration
    /// language, but it is most intuitive to use an octal number.
    pub socket_file_mode: Option<u32>,

    /// The maximum buffer size, in bytes, of incoming messages.
    ///
    /// Messages larger than this are truncated.
    pub max_length: Option<usize>,

    /// Overrides the name of the log field used to add the peer host to each event.
    ///
    /// The value will be the socket path itself.
    ///
    /// By default, the [global `host_key` option](https://vector.dev/docs/reference/configuration//global-options#log_schema.host_key) is used.pub host_key: Option<String>,
    pub host_key: Option<String>,

    #[configurable(derived)]
    #[serde(default)]
    pub framing: Option<FramingConfig>,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    pub decoding: DeserializerConfig,
}

impl UnixConfig {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            socket_file_mode: None,
            max_length: Some(crate::serde::default_max_length()),
            host_key: None,
            framing: None,
            decoding: default_decoding(),
        }
    }
}

/// Function to pass to `build_unix_*_source`, specific to the basic unix source
/// Takes a single line of a received message and handles an `Event` object.
fn handle_events(events: &mut [Event], host_key: &str, received_from: Option<Bytes>) {
    let now = Utc::now();

    for event in events {
        let log = event.as_mut_log();

        log.try_insert(log_schema().source_type_key(), Bytes::from("socket"));
        log.try_insert(log_schema().timestamp_key(), now);

        if let Some(ref host) = received_from {
            log.try_insert(host_key, host.clone());
        }
    }
}

pub(super) fn unix_datagram(
    path: PathBuf,
    socket_file_mode: Option<u32>,
    max_length: usize,
    host_key: String,
    decoder: Decoder,
    shutdown: ShutdownSignal,
    out: SourceSender,
) -> crate::Result<Source> {
    build_unix_datagram_source(
        path,
        socket_file_mode,
        max_length,
        decoder,
        move |events, received_from| handle_events(events, &host_key, received_from),
        shutdown,
        out,
    )
}

pub(super) fn unix_stream(
    path: PathBuf,
    socket_file_mode: Option<u32>,
    host_key: String,
    decoder: Decoder,
    shutdown: ShutdownSignal,
    out: SourceSender,
) -> crate::Result<Source> {
    build_unix_stream_source(
        path,
        socket_file_mode,
        decoder,
        move |events, received_from| handle_events(events, &host_key, received_from),
        shutdown,
        out,
    )
}
