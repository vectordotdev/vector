use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::StatsdDeserializer;
use crate::{
    codecs::{
        decoding::{Deserializer, Framer},
        Decoder, NewlineDelimitedDecoder,
    },
    shutdown::ShutdownSignal,
    sources::{util::build_unix_stream_source, Source},
    SourceSender,
};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct UnixConfig {
    pub path: PathBuf,
}

pub fn statsd_unix(config: UnixConfig, shutdown: ShutdownSignal, out: SourceSender) -> Source {
    let decoder = Decoder::new(
        Framer::NewlineDelimited(NewlineDelimitedDecoder::new()),
        Deserializer::Boxed(Box::new(StatsdDeserializer)),
    );

    build_unix_stream_source(config.path, decoder, |_events, _host| {}, shutdown, out)
}
