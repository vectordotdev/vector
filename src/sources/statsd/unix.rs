use std::path::PathBuf;

use vector_lib::{
    codecs::{
        NewlineDelimitedDecoder,
        decoding::{Deserializer, Framer},
    },
    configurable::configurable_component,
};

use super::{ConversionUnit, StatsdDeserializer, default_convert_to, default_sanitize};
use crate::{
    SourceSender,
    codecs::Decoder,
    shutdown::ShutdownSignal,
    sources::{
        Source,
        util::{build_unix_datagram_source, build_unix_stream_source},
    },
};

/// The type of Unix socket to use.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UnixSocketType {
    /// Stream socket (connection-oriented).
    #[default]
    Stream,

    /// Datagram socket (connectionless).
    Datagram,
}

fn default_max_length() -> usize {
    crate::serde::default_max_length()
}

/// Unix domain socket configuration for the `statsd` source.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct UnixConfig {
    /// The Unix socket path.
    ///
    /// This should be an absolute path.
    #[configurable(metadata(docs::examples = "/path/to/socket"))]
    pub path: PathBuf,

    /// The type of Unix socket to use.
    #[serde(default)]
    #[configurable(derived)]
    pub socket_type: UnixSocketType,

    /// The maximum buffer size of incoming messages, in bytes.
    ///
    /// Messages larger than this are truncated.
    #[serde(default = "default_max_length")]
    #[configurable(metadata(docs::type_unit = "bytes"))]
    pub max_length: usize,

    #[serde(default = "default_sanitize")]
    #[configurable(derived)]
    pub sanitize: bool,

    #[serde(default = "default_convert_to")]
    #[configurable(derived)]
    pub convert_to: ConversionUnit,
}

pub fn statsd_unix(
    config: UnixConfig,
    shutdown: ShutdownSignal,
    out: SourceSender,
) -> crate::Result<Source> {
    let decoder = Decoder::new(
        Framer::NewlineDelimited(NewlineDelimitedDecoder::new()),
        Deserializer::Boxed(Box::new(StatsdDeserializer::unix(
            config.sanitize,
            config.convert_to,
        ))),
    );

    match config.socket_type {
        UnixSocketType::Stream => build_unix_stream_source(
            config.path,
            None,
            decoder,
            |_events, _host| {},
            shutdown,
            out,
        ),
        UnixSocketType::Datagram => build_unix_datagram_source(
            config.path,
            None,
            config.max_length,
            decoder,
            |_events, _host| {},
            shutdown,
            out,
        ),
    }
}
