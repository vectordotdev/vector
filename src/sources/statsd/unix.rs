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
#[derive(Clone, Copy, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum UnixMode {
    /// Stream-oriented (`SOCK_STREAM`).
    #[default]
    Stream,

    /// Datagram-oriented (`SOCK_DGRAM`).
    Datagram,
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

    /// The Unix socket mode to use.
    #[serde(default)]
    #[configurable(derived)]
    pub unix_mode: UnixMode,

    /// Unix file mode bits to be applied to th Unix socket file.
    ///
    /// Note: The file mode value can be specified in any numeric format supported by your
    /// configuration language. Octal notation is common for file permissions (for example, 0o777 or 0o600).
    #[configurable(metadata(docs::examples = 0o777))]
    #[configurable(metadata(docs::examples = 0o600))]
    pub socket_file_mode: Option<u32>,

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

    match config.unix_mode {
        UnixMode::Stream => build_unix_stream_source(
            config.path,
            config.socket_file_mode,
            decoder,
            |_events, _host| {},
            shutdown,
            out,
        ),
        UnixMode::Datagram => build_unix_datagram_source(
            config.path,
            config.socket_file_mode,
            crate::serde::default_max_length(),
            decoder,
            |_events, _host| {},
            shutdown,
            out,
        ),
    }
}
