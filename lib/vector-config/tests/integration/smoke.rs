// We allow dead code because some of the things we're testing are meant to ensure that the macros do the right thing
// for codegen i.e. not doing codegen for fields that `serde` is going to skip, etc.
#![allow(dead_code)]
#![allow(clippy::print_stdout)] // tests
#![allow(clippy::print_stderr)] // tests

use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    num::NonZeroU64,
    path::PathBuf,
    time::Duration,
};

use serde::{de, Deserialize, Deserializer};
use serde_with::serde_as;
use vector_config::{
    component::GenerateConfig, configurable_component, schema::generate_root_schema,
    ConfigurableString,
};

/// A templated string.
#[configurable_component]
#[configurable(metadata(docs::templateable))]
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
#[serde(try_from = "String", into = "String")]
pub struct Template {
    /// The template string.
    src: String,

    #[serde(skip)]
    has_ts: bool,

    #[serde(skip)]
    has_fields: bool,
}

impl ConfigurableString for Template {}

impl TryFrom<String> for Template {
    type Error = String;

    fn try_from(src: String) -> Result<Self, Self::Error> {
        if src.is_empty() {
            Err("wahhh".to_string())
        } else {
            Ok(Self {
                src,
                has_ts: false,
                has_fields: false,
            })
        }
    }
}

impl From<Template> for String {
    fn from(template: Template) -> String {
        template.src
    }
}

/// A period of time.
#[derive(Clone)]
#[configurable_component]
pub struct SpecialDuration(u64);

/// Controls the batching behavior of events.
#[derive(Clone)]
#[configurable_component]
#[serde(default)]
pub struct BatchConfig {
    /// The maximum number of events in a batch before it is flushed.
    #[configurable(validation(range(max = 100000)))]
    max_events: Option<NonZeroU64>,

    /// The maximum number of bytes in a batch before it is flushed.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    max_bytes: Option<NonZeroU64>,

    /// The maximum amount of time a batch can exist before it is flushed.
    timeout: Option<SpecialDuration>,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_events: Some(NonZeroU64::new(1000).expect("must be nonzero")),
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
        /// would be the standard output, which eschews whitespace for the most succinct output.
        pretty: bool,
    },

    #[configurable(description = "MessagePack encoding.")]
    MessagePack(
        /// Starting offset for fields something this is a fake description anyways.
        u64,
    ),
}

/// Enableable TLS configuration.
#[derive(Clone)]
#[configurable_component]
#[configurable(metadata(docs::examples = "Self::default()"))]
pub struct TlsEnablableConfig {
    /// Whether or not TLS is enabled.
    pub enabled: bool,

    #[serde(flatten)]
    pub options: TlsConfig,
}

impl Default for TlsEnablableConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            options: TlsConfig {
                crt_file: None,
                key_file: None,
            },
        }
    }
}

/// TLS configuration.
#[derive(Clone)]
#[configurable_component]
pub struct TlsConfig {
    /// Certificate file.
    pub crt_file: Option<PathBuf>,

    /// Private key file.
    pub key_file: Option<PathBuf>,
}

/// A listening address that can optionally support being passed in by systemd.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[configurable_component]
#[serde(untagged)]
pub enum SocketListenAddr {
    /// A literal socket address.
    SocketAddr(SocketAddr),

    /// A file descriptor identifier passed by systemd.
    #[serde(deserialize_with = "parse_systemd_fd")]
    SystemdFd(usize),
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
#[serde_as]
#[configurable_component(source("simple"))]
#[derive(Clone)]
#[configurable(metadata(status = "beta"))]
pub struct SimpleSourceConfig {
    /// The address to listen on for events.
    #[serde(default = "default_simple_source_listen_addr")]
    listen_addr: SocketListenAddr,

    /// The timeout for waiting for events from the source before closing the source.
    #[serde(default = "default_simple_source_timeout")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    timeout: Duration,
}

impl GenerateConfig for SimpleSourceConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            listen_addr: default_simple_source_listen_addr(),
            timeout: default_simple_source_timeout(),
        })
        .unwrap()
    }
}

const fn default_simple_source_timeout() -> Duration {
    Duration::from_secs(42)
}

fn default_simple_source_listen_addr() -> SocketListenAddr {
    SocketListenAddr::SocketAddr(SocketAddr::V4(SocketAddrV4::new(
        Ipv4Addr::new(127, 0, 0, 1),
        9200,
    )))
}

/// A sink for sending events to the `simple` service.
#[derive(Clone)]
#[configurable_component(sink("simple"))]
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

    /// The filepath to write the events to.
    output_path: Template,

    /// The tags to apply to each event.
    #[configurable(validation(length(max = 32)))]
    tags: HashMap<String, String>,

    #[serde(skip)]
    meaningless_field: String,
}

impl GenerateConfig for SimpleSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            endpoint: default_simple_sink_endpoint(),
            batch: default_simple_sink_batch(),
            encoding: default_simple_sink_encoding(),
            output_path: Template::try_from("basic".to_string()).expect("should not fail to parse"),
            tags: HashMap::new(),
            meaningless_field: "foo".to_string(),
        })
        .unwrap()
    }
}

fn default_simple_sink_batch() -> BatchConfig {
    BatchConfig {
        max_events: Some(NonZeroU64::new(10000).expect("must be nonzero")),
        max_bytes: Some(NonZeroU64::new(16_000_000).expect("must be nonzero")),
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
#[configurable_component(sink("advanced"))]
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

    #[configurable(deprecated, derived)]
    #[serde(default = "default_advanced_sink_encoding")]
    encoding: Encoding,

    /// Overridden TLS description.
    #[configurable(derived)]
    tls: Option<TlsEnablableConfig>,

    /// The partition key to use for each event.
    #[configurable(metadata(docs::templateable))]
    #[serde(default = "default_partition_key")]
    partition_key: String,

    /// The tags to apply to each event.
    tags: HashMap<String, TagConfig>,
}

/// Specification of the value of a created tag.
///
/// This may be a single value, a `null` for a bare tag, or an array of either.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(untagged)]
pub enum TagConfig {
    /// A single tag value.
    Plain(Option<Template>),

    /// An array of values to give to the same tag name.
    Multi(Vec<Option<Template>>),
}

impl GenerateConfig for AdvancedSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            endpoint: default_advanced_sink_endpoint(),
            agent_version: String::from("v1.2.3"),
            batch: default_advanced_sink_batch(),
            encoding: default_advanced_sink_encoding(),
            tls: None,
            partition_key: default_partition_key(),
            tags: HashMap::new(),
        })
        .unwrap()
    }
}

fn default_advanced_sink_batch() -> BatchConfig {
    BatchConfig {
        max_events: Some(NonZeroU64::new(5678).expect("must be nonzero")),
        max_bytes: Some(NonZeroU64::new(36_000_000).expect("must be nonzero")),
        timeout: Some(SpecialDuration(15)),
    }
}

fn default_partition_key() -> String {
    "foo".to_string()
}

const fn default_advanced_sink_encoding() -> Encoding {
    Encoding::Json { pretty: true }
}

fn default_advanced_sink_endpoint() -> String {
    String::from("https://zalgohtml5.io")
}

pub mod vector_v2 {
    use std::net::SocketAddr;

    use vector_config::{component::GenerateConfig, configurable_component};

    /// Configuration for version two of the `vector` source.
    #[configurable_component]
    #[derive(Clone, Debug)]
    #[serde(deny_unknown_fields)]
    pub struct VectorConfig {
        /// The socket address to listen for connections on.
        ///
        /// It _must_ include a port.
        pub address: SocketAddr,

        /// The timeout, in seconds, before a connection is forcefully closed during shutdown.
        #[serde(default = "default_shutdown_timeout_secs")]
        pub shutdown_timeout_secs: u64,
    }

    const fn default_shutdown_timeout_secs() -> u64 {
        30
    }

    impl GenerateConfig for VectorConfig {
        fn generate_config() -> toml::Value {
            toml::Value::try_from(Self {
                address: "0.0.0.0:6000".parse().unwrap(),
                shutdown_timeout_secs: default_shutdown_timeout_secs(),
            })
            .unwrap()
        }
    }
}

/// Marker type for the version two of the configuration for the `vector` source.
#[configurable_component]
#[derive(Clone, Debug)]
enum V2 {
    /// Marker value for version two.
    #[serde(rename = "2")]
    V2,
}

/// Configuration for version two of the `vector` source.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct VectorConfigV2 {
    /// Version of the configuration.
    version: Option<V2>,

    #[serde(flatten)]
    config: self::vector_v2::VectorConfig,
}

/// Configurable for the `vector` source.
#[configurable_component(source("vector"))]
#[derive(Clone, Debug)]
#[serde(untagged)]
pub enum VectorSourceConfig {
    /// Configuration for version two.
    V2(VectorConfigV2),
}

impl GenerateConfig for VectorSourceConfig {
    fn generate_config() -> toml::Value {
        let config = toml::Value::try_into::<self::vector_v2::VectorConfig>(
            self::vector_v2::VectorConfig::generate_config(),
        )
        .unwrap();
        toml::Value::try_from(VectorConfigV2 {
            version: Some(V2::V2),
            config,
        })
        .unwrap()
    }
}

/// Collection of various sources available in Vector.
#[derive(Clone)]
#[configurable_component]
#[serde(tag = "type")]
pub enum SourceConfig {
    /// Simple source.
    Simple(SimpleSourceConfig),

    /// Vector source.
    Vector(VectorSourceConfig),
}

/// Collection of various sinks available in Vector.
#[derive(Clone)]
#[configurable_component]
#[serde(tag = "type")]
pub enum SinkConfig {
    /// Simple sink.
    Simple(SimpleSinkConfig),

    /// Advanced sink.
    Advanced(AdvancedSinkConfig),
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
fn generate_semi_real_schema() {
    match generate_root_schema::<VectorConfig>() {
        Ok(schema) => {
            let json = serde_json::to_string_pretty(&schema)
                .expect("rendering root schema to JSON should not fail");

            println!("{}", json);
        }
        Err(e) => eprintln!("error while generating schema: {:?}", e),
    }
}
