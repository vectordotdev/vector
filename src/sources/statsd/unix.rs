use crate::{
    internal_events::{StatsdEventReceived, StatsdInvalidRecord},
    shutdown::ShutdownSignal,
    sources::util::build_unix_source,
    sources::Source,
    Event, Pipeline,
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
    match super::parser::parse(line) {
        Ok(metric) => {
            emit!(StatsdEventReceived {
                byte_size: line.len()
            });
            Some(Event::Metric(metric))
        }
        Err(error) => {
            emit!(StatsdInvalidRecord { error, text: &line });
            None
        }
    }
}

pub fn statsd_unix(config: UnixConfig, shutdown: ShutdownSignal, out: Pipeline) -> Source {
    build_unix_source(
        config.path,
        LinesCodec::new(),
        String::new(),
        shutdown,
        out,
        build_event,
    )
}
