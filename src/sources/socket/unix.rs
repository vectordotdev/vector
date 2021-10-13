use crate::{
    codecs::{Decoder, DecodingConfig},
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
    #[serde(default = "crate::serde::default_max_length")]
    pub max_length: usize,
    pub host_key: Option<String>,
    pub receive_buffer_bytes: Option<usize>,
    #[serde(flatten, default)]
    pub decoding: DecodingConfig,
}

impl UnixConfig {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            max_length: crate::serde::default_max_length(),
            host_key: None,
            receive_buffer_bytes: None,
            decoding: Default::default(),
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
    emit!(&SocketEventsReceived {
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
    host_key: String,
    receive_buffer_bytes: usize,
    decoder: Decoder,
    shutdown: ShutdownSignal,
    out: Pipeline,
) -> Source {
    build_unix_datagram_source(
        path,
        decoder,
        receive_buffer_bytes,
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
    receive_buffer_bytes: Option<usize>,
    shutdown: ShutdownSignal,
    out: Pipeline,
) -> Source {
    build_unix_stream_source(
        path,
        decoder,
        receive_buffer_bytes,
        move |events, received_from, byte_size| {
            handle_events(events, &host_key, received_from, byte_size)
        },
        shutdown,
        out,
    )
}
