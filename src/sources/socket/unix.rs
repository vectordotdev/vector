use std::path::PathBuf;

use bytes::Bytes;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
    codecs::{
        decoding::{DeserializerConfig, FramingConfig},
        Decoder,
    },
    config::log_schema,
    event::Event,
    internal_events::{SocketEventsReceived, SocketMode},
    serde::default_decoding,
    shutdown::ShutdownSignal,
    sources::{
        util::{build_unix_datagram_source, build_unix_stream_source},
        Source,
    },
    Pipeline,
};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct UnixConfig {
    pub path: PathBuf,
    pub max_length: Option<usize>,
    pub host_key: Option<String>,
    #[serde(default)]
    pub framing: Option<Box<dyn FramingConfig>>,
    #[serde(default = "default_decoding")]
    pub decoding: Box<dyn DeserializerConfig>,
}

impl UnixConfig {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            max_length: Some(crate::serde::default_max_length()),
            host_key: None,
            framing: None,
            decoding: default_decoding(),
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

    let now = Utc::now();

    for event in events {
        let log = event.as_mut_log();

        log.try_insert(log_schema().source_type_key(), Bytes::from("socket"));
        log.try_insert(log_schema().timestamp_key(), now);

        if let Some(ref host) = received_from {
            log.try_insert(host_key, host.clone());
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
