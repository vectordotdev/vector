use crate::{
    event::Event, shutdown::ShutdownSignal, sources::util::build_unix_stream_source,
    sources::Source, Pipeline,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio_util::codec::LinesCodec;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct UnixConfig {
    pub path: PathBuf,
}

fn build_event(_: &str, _: Option<Bytes>, line: &str) -> Option<Event> {
    super::parse_event(line)
}

pub fn statsd_unix(config: UnixConfig, shutdown: ShutdownSignal, out: Pipeline) -> Source {
    build_unix_stream_source(
        config.path,
        LinesCodec::new(),
        String::new(),
        shutdown,
        out,
        build_event,
    )
}
