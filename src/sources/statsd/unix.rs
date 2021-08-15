use super::StatsdParser;
use crate::{
    codec::{Decoder, NewlineDelimitedCodec},
    shutdown::ShutdownSignal,
    sources::util::build_unix_stream_source,
    sources::Source,
    Pipeline,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct UnixConfig {
    pub path: PathBuf,
}

pub fn statsd_unix(config: UnixConfig, shutdown: ShutdownSignal, out: Pipeline) -> Source {
    let decoder = Decoder::new(
        Box::new(NewlineDelimitedCodec::new()),
        Box::new(StatsdParser),
    );

    build_unix_stream_source(
        config.path,
        decoder,
        shutdown,
        out,
        |_event, _host, _byte_size| {},
    )
}
