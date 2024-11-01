use std::path::PathBuf;

use vector_lib::codecs::{
    decoding::{Deserializer, Framer},
    NewlineDelimitedDecoder,
};
use vector_lib::configurable::configurable_component;

use super::{default_sanitize, StatsdDeserializer};
use crate::{
    codecs::Decoder,
    shutdown::ShutdownSignal,
    sources::{util::build_unix_stream_source, Source},
    SourceSender,
};

/// Unix domain socket configuration for the `statsd` source.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct UnixConfig {
    /// The Unix socket path.
    ///
    /// This should be an absolute path.
    #[configurable(metadata(docs::examples = "/path/to/socket"))]
    pub path: PathBuf,

    #[serde(default = "default_sanitize")]
    #[configurable(derived)]
    pub sanitize: bool,
}

pub fn statsd_unix(
    config: UnixConfig,
    shutdown: ShutdownSignal,
    out: SourceSender,
) -> crate::Result<Source> {
    let decoder = Decoder::new(
        Framer::NewlineDelimited(NewlineDelimitedDecoder::new()),
        Deserializer::Boxed(Box::new(StatsdDeserializer::unix(config.sanitize))),
    );

    build_unix_stream_source(
        config.path,
        None,
        decoder,
        |_events, _host| {},
        shutdown,
        out,
    )
}
