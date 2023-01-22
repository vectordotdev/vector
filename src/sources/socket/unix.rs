use std::path::PathBuf;

use bytes::Bytes;
use chrono::Utc;
use codecs::decoding::{DeserializerConfig, FramingConfig};
use lookup::{
    lookup_v2::{parse_value_path, OptionalValuePath},
    path,
};
use vector_common::shutdown::ShutdownSignal;
use vector_config::{configurable_component, NamedComponent};
use vector_core::config::{LegacyKey, LogNamespace};

use crate::{
    codecs::Decoder,
    config::log_schema,
    event::Event,
    serde::default_decoding,
    sources::{
        util::{build_unix_datagram_source, build_unix_stream_source},
        Source,
    },
    SourceSender,
};

use super::SocketConfig;

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
    /// By default, the [global `log_schema.host_key` option][global_host_key] is used.
    ///
    /// [global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
    pub host_key: Option<OptionalValuePath>,

    #[configurable(derived)]
    #[serde(default)]
    pub framing: Option<FramingConfig>,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    pub decoding: DeserializerConfig,

    /// The namespace to use for logs. This overrides the global setting.
    #[serde(default)]
    pub log_namespace: Option<bool>,
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
            log_namespace: None,
        }
    }

    pub const fn decoding(&self) -> &DeserializerConfig {
        &self.decoding
    }

    pub const fn host_key(&self) -> &Option<OptionalValuePath> {
        &self.host_key
    }
}

/// Function to pass to `build_unix_*_source`, specific to the basic unix source
/// Takes a single line of a received message and handles an `Event` object.
fn handle_events(
    events: &mut [Event],
    host_key: Option<&OptionalValuePath>,
    received_from: Option<Bytes>,
    log_namespace: LogNamespace,
) {
    let now = Utc::now();

    for event in events {
        let log = event.as_mut_log();

        log_namespace.insert_standard_vector_source_metadata(log, SocketConfig::NAME, now);

        if let Some(ref host) = received_from {
            let legacy_host_key = host_key.map_or_else(
                || parse_value_path(log_schema().host_key()).ok(),
                |k| k.path.clone(),
            );

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
    build_unix_datagram_source(
        config.path,
        config.socket_file_mode,
        config
            .max_length
            .unwrap_or_else(crate::serde::default_max_length),
        decoder,
        move |events, received_from| {
            handle_events(
                events,
                config.host_key.as_ref(),
                received_from,
                log_namespace,
            )
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
            handle_events(
                events,
                config.host_key.as_ref(),
                received_from,
                log_namespace,
            )
        },
        shutdown,
        out,
    )
}
