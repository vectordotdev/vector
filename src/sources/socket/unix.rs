use std::path::PathBuf;

use chrono::Utc;
use codecs::decoding::{DeserializerConfig, FramingConfig};
use lookup::{lookup_v2::OptionalValuePath, path};
use vector_common::shutdown::ShutdownSignal;
use vector_config::configurable_component;
use vector_core::config::{LegacyKey, LogNamespace};

use crate::{
    codecs::Decoder,
    event::Event,
    serde::default_decoding,
    sources::{
        util::{build_unix_datagram_source, build_unix_stream_source},
        util::unix::{UnixSocketMetadata,UnixSocketMetadataCollectTypes},
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

    /// If set, output events will contain the device & inode number of the
    /// socket under this key. For stream sockets, this will be a unique
    /// identifier of each incoming connection; for datagram sockets, this
    /// will be the same value for every incoming message (but can uniquely
    /// identify this source).
    ///
    /// The key will be set with an object containing `"dev"` and `"ino"` keys
    /// representing the device & inode number of the socket.
    ///
    /// By default, this key is not emitted. Set to `""` to explicitly suppress
    /// this key.
    #[serde(default = "default_inode_key")]
    pub inode_key: OptionalValuePath,
}

fn default_inode_key() -> OptionalValuePath {
    OptionalValuePath::none()
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
            inode_key: default_inode_key(),
        }
    }

    pub const fn decoding(&self) -> &DeserializerConfig {
        &self.decoding
    }

    pub const fn host_key(&self) -> &OptionalValuePath {
        &self.host_key
    }

    fn socket_collect_metadata(&self) -> UnixSocketMetadataCollectTypes {
        let mut types = UnixSocketMetadataCollectTypes::default();
        types.peer_path = self.host_key.path.is_some();
        types.socket_inode = self.inode_key.path.is_some();
        types
    }
}

/// Function to pass to `build_unix_*_source`, specific to the basic unix source
/// Takes a single line of a received message and handles an `Event` object.
fn handle_events(
    events: &mut [Event],
    host_key: &OptionalValuePath,
    inode_key: &OptionalValuePath,
    socket_metadata: &UnixSocketMetadata,
    log_namespace: LogNamespace,
) {
    let now = Utc::now();

    for event in events {
        let log = event.as_mut_log();

        log_namespace.insert_standard_vector_source_metadata(log, SocketConfig::NAME, now);

        let legacy_host_key = host_key.clone().path;
        log_namespace.insert_source_metadata(
            SocketConfig::NAME,
            log,
            legacy_host_key.as_ref().map(LegacyKey::InsertIfEmpty),
            path!("host"),
            socket_metadata.peer_path_or_default().clone(),
        );

        let legacy_inode_key = inode_key.clone().path;
        log_namespace.insert_source_metadata(
            SocketConfig::NAME,
            log,
            legacy_inode_key.as_ref().map(LegacyKey::InsertIfEmpty),
            path!("inode"),
            socket_metadata.socket_inode,
        );
    }
}

pub(super) fn unix_datagram(
    config: UnixConfig,
    decoder: Decoder,
    shutdown: ShutdownSignal,
    out: SourceSender,
    log_namespace: LogNamespace,
) -> crate::Result<Source> {
    let collect_metadata = config.socket_collect_metadata();
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
        collect_metadata,
        max_length,
        decoder,
        move |events, socket_metadata| {
            handle_events(events, &config.host_key, &config.inode_key, socket_metadata, log_namespace)
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
    let collect_metadata = config.socket_collect_metadata();
    build_unix_stream_source(
        config.path,
        config.socket_file_mode,
        collect_metadata,
        decoder,
        move |events, socket_metadata| {
            handle_events(events, &config.host_key, &config.inode_key, socket_metadata, log_namespace)
        },
        shutdown,
        out,
    )
}
