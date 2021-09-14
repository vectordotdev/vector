use crate::{
    codecs::NewlineDelimitedCodec, event::Event, shutdown::ShutdownSignal,
    sources::util::build_unix_stream_source, sources::Source, Pipeline,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct UnixConfig {
    pub path: PathBuf,
}

fn build_event(_: &str, _: Option<Bytes>, bytes: Bytes) -> Option<Event> {
    super::parse_event(bytes)
}

pub fn statsd_unix(config: UnixConfig, shutdown: ShutdownSignal, out: Pipeline) -> Source {
    build_unix_stream_source(
        config.path,
        NewlineDelimitedCodec::new(),
        String::new(),
        shutdown,
        out,
        build_event,
    )
}
