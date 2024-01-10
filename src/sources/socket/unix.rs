use std::path::PathBuf;

use bytes::Bytes;
use chrono::Utc;
use vector_lib::codecs::decoding::{DeserializerConfig, FramingConfig};
use vector_lib::config::{LegacyKey, LogNamespace};
use vector_lib::configurable::configurable_component;
use vector_lib::lookup::{lookup_v2::OptionalValuePath, path};
use vector_lib::shutdown::ShutdownSignal;

use crate::{
    codecs::Decoder,
    event::Event,
    serde::default_decoding,
    sources::{
        util::{build_unix_datagram_source, build_unix_stream_source},
        Source,
    },
    SourceSender,
};

use super::{default_host_key, SocketConfig};

/// Unix domain socket configuration for the `socket` source.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct UnixConfig {
    /// The Unix socket path.
    ///
    /// This should be an absolute path.
    #[configurable(metadata(docs::examples = "/path/to/socket"))]
    pub path: PathBuf,

    /// Unix file mode bits to be applied to the unix socket file as its designated file permissions.
    ///
    /// Note: The file mode value can be specified in any numeric format supported by your configuration
    /// language, but it is most intuitive to use an octal number.
    #[configurable(metadata(docs::examples = 0o777))]
    #[configurable(metadata(docs::examples = 0o600))]
    #[configurable(metadata(docs::examples = 508))]
    pub socket_file_mode: Option<u32>,

    /// Overrides the name of the log field used to add the peer host to each event.
    ///
    /// The value will be the peer host's address, including the port i.e. `1.2.3.4:9000`.
    ///
    /// By default, the [global `log_schema.host_key` option][global_host_key] is used.
    ///
    /// Set to `""` to suppress this key.
    ///
    /// [global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
    #[serde(default = "default_host_key")]
    pub host_key: OptionalValuePath,

    #[configurable(derived)]
    #[serde(default)]
    pub framing: Option<FramingConfig>,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    pub decoding: DeserializerConfig,

    /// The namespace to use for logs. This overrides the global setting.
    #[serde(default)]
    #[configurable(metadata(docs::hidden))]
    pub log_namespace: Option<bool>,
}

impl UnixConfig {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            socket_file_mode: None,
            host_key: default_host_key(),
            framing: None,
            decoding: default_decoding(),
            log_namespace: None,
        }
    }

    pub const fn decoding(&self) -> &DeserializerConfig {
        &self.decoding
    }

    pub const fn host_key(&self) -> &OptionalValuePath {
        &self.host_key
    }
}

/// Function to pass to `build_unix_*_source`, specific to the basic unix source
/// Takes a single line of a received message and handles an `Event` object.
fn handle_events(
    events: &mut [Event],
    host_key: &OptionalValuePath,
    received_from: Option<Bytes>,
    log_namespace: LogNamespace,
) {
    let now = Utc::now();

    for event in events {
        let log = event.as_mut_log();

        log_namespace.insert_standard_vector_source_metadata(log, SocketConfig::NAME, now);

        if let Some(ref host) = received_from {
            let legacy_host_key = host_key.clone().path;

            log_namespace.insert_source_metadata(
                SocketConfig::NAME,
                log,
                legacy_host_key.as_ref().map(LegacyKey::InsertIfEmpty),
                path!("host"),
                host.clone(),
            );
        }
    }
}

pub(super) fn unix_datagram(
    config: UnixConfig,
    decoder: Decoder,
    shutdown: ShutdownSignal,
    out: SourceSender,
    log_namespace: LogNamespace,
) -> crate::Result<Source> {
    let max_length = config
        .framing
        .and_then(|framing| match framing {
            FramingConfig::CharacterDelimited(config) => config.character_delimited.max_length,
            FramingConfig::NewlineDelimited(config) => config.newline_delimited.max_length,
            FramingConfig::OctetCounting(config) => config.octet_counting.max_length,
            _ => None,
        })
        .unwrap_or_else(crate::serde::default_max_length);

    build_unix_datagram_source(
        config.path,
        config.socket_file_mode,
        max_length,
        decoder,
        move |events, received_from| {
            handle_events(events, &config.host_key, received_from, log_namespace)
        },
        shutdown,
        out,
    )
}

pub(super) fn unix_stream(
    config: UnixConfig,
    decoder: Decoder,
    shutdown: ShutdownSignal,
    out: SourceSender,
    log_namespace: LogNamespace,
) -> crate::Result<Source> {
    build_unix_stream_source(
        config.path,
        config.socket_file_mode,
        decoder,
        move |events, received_from| {
            handle_events(events, &config.host_key, received_from, log_namespace)
        },
        shutdown,
        out,
    )
}
