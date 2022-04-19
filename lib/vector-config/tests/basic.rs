use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
};

use schemars::{gen::SchemaGenerator, schema::SchemaObject};
use serde::{de, Deserialize, Deserializer, Serialize};
use vector_config::{
    configurable_component,
    schema::{finalize_schema, generate_root_schema},
    Configurable, Metadata,
};

/// A period of time.
#[derive(Clone, Serialize, Deserialize)]
pub struct SpecialDuration(u64);

/// Controls the batching behavior of events.
#[derive(Clone)]
#[configurable_component]
#[serde(default)]
pub struct BatchConfig {
    /// The maximum number of events in a batch before it is flushed.
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
#[configurable_component]
#[configurable(metadata("status", "beta"))]
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
#[configurable_component]
#[configurable(metadata("status", "beta"))]
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
#[configurable_component]
#[configurable(metadata("status", "stable"))]
pub struct AdvancedSinkConfig {
    /// The endpoint to send events to.
    #[serde(default = "default_advanced_sink_endpoint")]
    endpoint: String,
    #[configurable(derived)]
    #[serde(default = "default_advanced_sink_batch")]
    batch: BatchConfig,
    #[configurable(derived)]
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

impl<'de> Configurable<'de> for SpecialDuration {
    fn metadata() -> Metadata<'de, Self> {
        Metadata::with_description("A period of time.")
    }

    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject {
        let merged_metadata = Self::metadata().merge(overrides);

        // We generate the schema for the inner unnamed field, and then apply the metadata to it.
        let inner_metadata = <u64 as Configurable<'de>>::metadata().merge(
            merged_metadata
                .clone()
                .map_default_value(|default| default.0),
        );

        let mut inner_schema =
            <u64 as Configurable<'de>>::generate_schema(gen, inner_metadata.clone());
        finalize_schema(gen, &mut inner_schema, inner_metadata);

        inner_schema
    }
}

#[test]
fn vector_config() {
    let root_schema = generate_root_schema::<VectorConfig>();
    let json = serde_json::to_string_pretty(&root_schema)
        .expect("rendering root schema to JSON should not fail");

    println!("{}", json);
}

#[test]
fn serde_enum_flexibility() {
    #[derive(Serialize, Deserialize)]
    struct MessagePackConfig {
        mp: u32,
    }

    #[derive(Serialize, Deserialize)]
    struct ProtocolBuffersConfig {
        pb: u32,
    }

    #[derive(Debug, Serialize, Deserialize)]
    //#[serde(tag = "t", content = "c")]
    //#[serde(untagged)]
    enum Encoding {
        Text,
        Woops,
        Json { pretty: bool },
        MessagePack(u64),
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct Container {
        encoding: Encoding,
    }

    let text_container = Container {
        encoding: Encoding::Text,
    };

    let woops_container = Container {
        encoding: Encoding::Woops,
    };

    let json_container = Container {
        encoding: Encoding::Json { pretty: true },
    };

    let msgpack_container = Container {
        encoding: Encoding::MessagePack(42),
    };

    let text_ser = serde_json::to_string_pretty(&text_container).unwrap();
    println!("text ser: {}", text_ser);

    let woops_ser = serde_json::to_string_pretty(&woops_container).unwrap();
    println!("woops ser: {}", woops_ser);

    let msgpack_ser = serde_json::to_string_pretty(&msgpack_container).unwrap();
    println!("msgpack ser: {}", msgpack_ser);

    let json_ser = serde_json::to_string_pretty(&json_container).unwrap();
    println!("json ser: {}", json_ser);

    let text_deser = serde_json::from_str::<'_, Container>(text_ser.as_str()).unwrap();
    println!("text deser: {:?}", text_deser);

    let woops_deser = serde_json::from_str::<'_, Container>(woops_ser.as_str()).unwrap();
    println!("woops deser: {:?}", woops_deser);

    let msgpack_deser = serde_json::from_str::<'_, Container>(msgpack_ser.as_str()).unwrap();
    println!("msgpack deser: {:?}", msgpack_deser);

    let json_deser = serde_json::from_str::<'_, Container>(json_ser.as_str()).unwrap();
    println!("json deser: {:?}", json_deser);
}
