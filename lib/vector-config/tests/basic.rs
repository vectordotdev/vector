// We allow dead code because some of the things we're testing are meant to ensure that the macros do the right thing
// for codegen i.e. not doing codegen for fields that `serde` is going to skip, etc.
#![allow(dead_code)]

use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
};

use serde::{de, Deserialize, Deserializer};
use vector_config::{configurable_component, schema::generate_root_schema};

/// A period of time.
#[derive(Clone)]
#[configurable_component]
pub struct SpecialDuration(#[configurable(transparent)] u64);

/// Controls the batching behavior of events.
#[derive(Clone)]
#[configurable_component]
#[serde(default)]
pub struct BatchConfig {
    /// The maximum number of events in a batch before it is flushed.
    #[configurable(validation(range(max = 100000)))]
    max_events: Option<u64>,
    /// The maximum number of bytes in a batch before it is flushed.
    max_bytes: Option<u64>,
    /// The maximum amount of time a batch can exist before it is flushed.
    timeout: Option<SpecialDuration>,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_events: Some(1000),
            max_bytes: None,
            timeout: Some(SpecialDuration(10)),
        }
    }
}

/// The encoding to decode/encode events with.
#[derive(Clone)]
#[configurable_component]
#[serde(tag = "t", content = "c")]
pub enum Encoding {
    /// Text encoding.
    Text,
    /// JSON encoding.
    Json {
        /// Whether or not to render the output in a "pretty" form.
        ///
        /// If enabled, this will generally cause the output to be spread across more lines, with
        /// more indentation, resulting in an easy-to-read form for humans.  The opposite of this
        /// would be the standard output, which eschews whitespace for the most succient output.
        pretty: bool,
    },
    #[configurable(description = "MessagePack encoding.")]
    MessagePack(
        /// Starting offset for fields something something this is a fake description anyways.
        u64,
    ),
}

/// A listening address that can optionally support being passed in by systemd.
#[derive(Clone, Copy, Debug, PartialEq)]
#[configurable_component]
#[serde(untagged)]
pub enum SocketListenAddr {
    /// A literal socket address.
    SocketAddr(#[configurable(derived)] SocketAddr),

    /// A file descriptor identifier passed by systemd.
    #[serde(deserialize_with = "parse_systemd_fd")]
    SystemdFd(#[configurable(transparent)] usize),
}

fn parse_systemd_fd<'de, D>(des: D) -> Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &'de str = Deserialize::deserialize(des)?;
    match s {
        "systemd" => Ok(0),
        s if s.starts_with("systemd#") => s[8..]
            .parse::<usize>()
            .map_err(de::Error::custom)?
            .checked_sub(1)
            .ok_or_else(|| de::Error::custom("systemd indices start from 1, found 0")),
        _ => Err(de::Error::custom("must start with \"systemd\"")),
    }
}

/// A source for collecting events over TCP.
#[derive(Clone)]
#[configurable_component(source)]
#[configurable(metadata(status = "beta"))]
pub struct SimpleSourceConfig {
    /// The address to listen on for events.
    #[serde(default = "default_simple_source_listen_addr")]
    listen_addr: SocketListenAddr,
}

fn default_simple_source_listen_addr() -> SocketListenAddr {
    SocketListenAddr::SocketAddr(SocketAddr::V4(SocketAddrV4::new(
        Ipv4Addr::new(127, 0, 0, 1),
        9200,
    )))
}

/// A sink for sending events to the `simple` service.
#[derive(Clone)]
#[configurable_component(sink)]
#[configurable(metadata(status = "beta"))]
pub struct SimpleSinkConfig {
    /// The endpoint to send events to.
    #[serde(default = "default_simple_sink_endpoint")]
    endpoint: String,
    #[configurable(derived)]
    #[serde(default = "default_simple_sink_batch")]
    batch: BatchConfig,
    #[configurable(derived)]
    #[serde(default = "default_simple_sink_encoding")]
    encoding: Encoding,
    /// The tags to apply to each event.
    #[configurable(validation(length(max = 32)))]
    tags: HashMap<String, String>,
    #[serde(skip)]
    meaningless_field: String,
}

const fn default_simple_sink_batch() -> BatchConfig {
    BatchConfig {
        max_events: Some(10000),
        max_bytes: Some(16_000_000),
        timeout: Some(SpecialDuration(5)),
    }
}

const fn default_simple_sink_encoding() -> Encoding {
    Encoding::Json { pretty: true }
}

fn default_simple_sink_endpoint() -> String {
    String::from("https://zalgo.io")
}

/// A sink for sending events to the `advanced` service.
#[derive(Clone)]
#[configurable_component(sink)]
#[configurable(metadata(status = "stable"))]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct AdvancedSinkConfig {
    /// The endpoint to send events to.
    #[serde(default = "default_advanced_sink_endpoint")]
    endpoint: String,
    /// The agent version to simulate when sending events to the downstream service.
    ///
    /// Must match the pattern of "v\d+\.\d+\.\d+", which allows for values such as `v1.23.0` or `v0.1.3`, and so on.
    #[configurable(validation(pattern = "foo"))]
    agent_version: String,
    #[configurable(derived)]
    #[serde(default = "default_advanced_sink_batch")]
    batch: BatchConfig,
    #[configurable(derived)]
    #[deprecated]
    #[serde(default = "default_advanced_sink_encoding")]
    encoding: Encoding,
    /// The tags to apply to each event.
    tags: HashMap<String, String>,
}

const fn default_advanced_sink_batch() -> BatchConfig {
    BatchConfig {
        max_events: Some(5678),
        max_bytes: Some(36_000_000),
        timeout: Some(SpecialDuration(15)),
    }
}

const fn default_advanced_sink_encoding() -> Encoding {
    Encoding::Json { pretty: true }
}

fn default_advanced_sink_endpoint() -> String {
    String::from("https://zalgohtml5.io")
}

/// Collection of various sources available in Vector.
#[derive(Clone)]
#[configurable_component]
pub enum SourceConfig {
    /// Simple source.
    Simple(#[configurable(derived)] SimpleSourceConfig),
}

/// Collection of various sinks available in Vector.
#[derive(Clone)]
#[configurable_component]
pub enum SinkConfig {
    /// Simple sink.
    Simple(#[configurable(derived)] SimpleSinkConfig),

    /// Advanced sink.
    Advanced(#[configurable(derived)] AdvancedSinkConfig),
}

#[derive(Clone)]
#[configurable_component]
#[configurable(description = "Global options for configuring Vector.")]
pub struct GlobalOptions {
    /// The data directory where Vector will store state.
    data_dir: Option<String>,
}

/// The overall configuration for Vector.
#[derive(Clone)]
#[configurable_component]
pub struct VectorConfig {
    #[configurable(derived)]
    global: GlobalOptions,
    /// Any configured sources.
    sources: Vec<SourceConfig>,
    /// Any configured sinks.
    sinks: Vec<SinkConfig>,
}

#[test]
fn vector_config() {
    let root_schema = generate_root_schema::<VectorConfig>();
    let json = serde_json::to_string_pretty(&root_schema)
        .expect("rendering root schema to JSON should not fail");

    println!("{}", json);
}
