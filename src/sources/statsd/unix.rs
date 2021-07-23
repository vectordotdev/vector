use super::StatsdParser;
use crate::{
    shutdown::ShutdownSignal,
    sources::util::{
        build_unix_stream_source,
        decoding::{BytesDecoder, Decoder},
    },
    sources::Source,
    Pipeline,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio_util::codec::LinesCodec;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct UnixConfig {
    pub path: PathBuf,
}

pub fn statsd_unix(config: UnixConfig, shutdown: ShutdownSignal, out: Pipeline) -> Source {
    let build_decoder = || {
        Decoder::new(
            Box::new(BytesDecoder::new(LinesCodec::new())),
            Box::new(StatsdParser),
        )
    };

    build_unix_stream_source(
        config.path,
        build_decoder,
        shutdown,
        out,
        |_event, _host, _byte_size| {},
    )
}
