use crate::{
    event::Event,
    internal_events::{SocketEventsReceived, SocketMode},
    shutdown::ShutdownSignal,
    sources::{
        util::{build_unix_datagram_source, build_unix_stream_source, decoding::DecodingConfig},
        Source,
    },
    Pipeline,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio_util::codec::Decoder;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct UnixConfig {
    pub path: PathBuf,
    #[serde(default = "default_max_length")]
    pub max_length: usize,
    pub host_key: Option<String>,
    #[serde(flatten)]
    pub decoding: DecodingConfig,
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
            decoding: Default::default(),
        }
    }
}

pub(super) fn unix_datagram<D>(
    path: PathBuf,
    max_length: usize,
    host_key: String,
    decoder: D,
    shutdown: ShutdownSignal,
    out: Pipeline,
) -> Source
where
    D: Decoder<Item = (Vec<Event>, usize)> + Send + 'static,
    D::Error: From<std::io::Error> + std::fmt::Debug + std::fmt::Display + Send,
{
    build_unix_datagram_source(
        path,
        max_length,
        decoder,
        shutdown,
        out,
        move |events, host, byte_size| {
            emit!(SocketEventsReceived {
                mode: SocketMode::Unix,
                count: events.len(),
                byte_size,
            });

            for event in events {
                let log = event.as_mut_log();
                log.insert(
                    crate::config::log_schema().source_type_key(),
                    Bytes::from("socket"),
                );
                if let Some(host) = host.clone() {
                    log.insert(&host_key, host);
                }
            }
        },
    )
}

pub(super) fn unix_stream<D>(
    path: PathBuf,
    _max_length: usize,
    host_key: String,
    decoder: D,
    shutdown: ShutdownSignal,
    out: Pipeline,
) -> Source
where
    D: Decoder<Item = (Vec<Event>, usize)> + Clone + Send + 'static,
    D::Error: From<std::io::Error> + std::fmt::Debug + std::fmt::Display + Send,
{
    build_unix_stream_source(
        path,
        decoder,
        shutdown,
        out,
        move |events, host, byte_size| {
            emit!(SocketEventsReceived {
                mode: SocketMode::Unix,
                count: events.len(),
                byte_size,
            });

            for event in events {
                let log = event.as_mut_log();
                log.insert(
                    crate::config::log_schema().source_type_key(),
                    Bytes::from("socket"),
                );
                if let Some(host) = host.clone() {
                    log.insert(&host_key, host);
                }
            }
        },
    )
}
