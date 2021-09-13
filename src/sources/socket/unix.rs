use crate::{
    codecs::Decoder,
    event::Event,
    internal_events::{SocketEventsReceived, SocketMode},
    shutdown::ShutdownSignal,
    sources::{
        util::{build_unix_datagram_source, build_unix_stream_source},
        Source,
    },
    Pipeline,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct UnixConfig {
    pub path: PathBuf,
    #[serde(default = "default_max_length")]
    pub max_length: usize,
    pub host_key: Option<String>,
}

fn default_max_length() -> usize {
    bytesize::kib(100u64) as usize
}

impl UnixConfig {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            max_length: default_max_length(),
            host_key: None,
        }
    }
}

/// Function to pass to `build_unix_*_source`, specific to the basic unix source
/// Takes a single line of a received message and handles an `Event` object.
fn handle_events(
    events: &mut [Event],
    host_key: &str,
    received_from: Option<Bytes>,
    byte_size: usize,
) {
    emit!(SocketEventsReceived {
        mode: SocketMode::Unix,
        byte_size,
        count: events.len()
    });

    for event in events {
        let log = event.as_mut_log();

        log.insert(
            crate::config::log_schema().source_type_key(),
            Bytes::from("socket"),
        );

        if let Some(ref host) = received_from {
            log.insert(host_key, host.clone());
        }
    }
}

pub(super) fn unix_datagram(
    path: PathBuf,
    max_length: usize,
    host_key: String,
    decoder: Decoder,
    shutdown: ShutdownSignal,
    out: Pipeline,
) -> Source {
    build_unix_datagram_source(
        path,
        max_length,
        decoder,
        move |events, received_from, byte_size| {
            handle_events(events, &host_key, received_from, byte_size)
        },
        shutdown,
        out,
    )
}

pub(super) fn unix_stream(
    path: PathBuf,
    host_key: String,
    decoder: Decoder,
    shutdown: ShutdownSignal,
    out: Pipeline,
) -> Source {
    build_unix_stream_source(
        path,
        decoder,
        move |events, received_from, byte_size| {
            handle_events(events, &host_key, received_from, byte_size)
        },
        shutdown,
        out,
    )
}
