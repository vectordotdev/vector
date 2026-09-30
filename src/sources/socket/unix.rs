use bytes::Bytes;
use chrono::Utc;
use vector_lib::{
    codecs::decoding::FramingConfig,
    config::{LegacyKey, LogNamespace},
    lookup::{lookup_v2::OptionalValuePath, path},
    shutdown::ShutdownSignal,
};

use super::{SocketConfig, UnixConfig};
use crate::{
    SourceSender,
    codecs::Decoder,
    event::Event,
    sources::{
        Source,
        util::{build_unix_datagram_source, build_unix_stream_source},
    },
};

/// Function to pass to `build_unix_*_source`, specific to the basic unix source
/// Takes a single line of a received message and handles an `Event` object.
fn handle_events(
    events: &mut [Event],
    host_key: &OptionalValuePath,
    received_from: Option<Bytes>,
    log_namespace: LogNamespace,
) {
    let now = Utc::now();

    for event in events {
        if let Event::Log(log) = event {
            log_namespace.insert_standard_vector_source_metadata(log, SocketConfig::NAME, now);

            if let Some(ref host) = received_from {
                let legacy_host_key = host_key.clone().path;

                log_namespace.insert_source_metadata(
                    SocketConfig::NAME,
                    log,
                    legacy_host_key.as_ref().map(LegacyKey::InsertIfEmpty),
                    path!("host"),
                    host.clone(),
                );
            }
        }
    }
}

pub(super) fn unix_datagram(
    config: UnixConfig,
    decoder: Decoder,
    shutdown: ShutdownSignal,
    out: SourceSender,
    log_namespace: LogNamespace,
) -> crate::Result<Source> {
    let max_length = config
        .framing
        .and_then(|framing| match framing {
            FramingConfig::CharacterDelimited(config) => config.character_delimited.max_length,
            FramingConfig::NewlineDelimited(config) => config.newline_delimited.max_length,
            FramingConfig::OctetCounting(config) => config.octet_counting.max_length,
            _ => None,
        })
        .unwrap_or_else(crate::serde::default_max_length);

    build_unix_datagram_source(
        config.path,
        config.socket_file_mode,
        max_length,
        decoder,
        move |events, received_from| {
            handle_events(events, &config.host_key, received_from, log_namespace)
        },
        shutdown,
        out,
    )
}

pub(super) fn unix_stream(
    config: UnixConfig,
    decoder: Decoder,
    shutdown: ShutdownSignal,
    out: SourceSender,
    log_namespace: LogNamespace,
) -> crate::Result<Source> {
    build_unix_stream_source(
        config.path,
        config.socket_file_mode,
        decoder,
        move |events, received_from| {
            handle_events(events, &config.host_key, received_from, log_namespace)
        },
        shutdown,
        out,
    )
}
