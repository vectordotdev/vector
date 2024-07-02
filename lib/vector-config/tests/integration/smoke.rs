// We allow dead code because some of the things we're testing are meant to ensure that the macros do the right thing
// for codegen i.e. not doing codegen for fields that `serde` is going to skip, etc.
#![allow(dead_code)]
#![allow(clippy::print_stdout)] // tests
#![allow(clippy::print_stderr)] // tests

use std::{
    collections::HashMap,
    fmt,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    num::NonZeroU64,
    path::PathBuf,
    time::Duration,
};

use indexmap::IndexMap;
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

impl fmt::Display for Template {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.src)
    }
}

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

/// IMDS Client Configuration for authenticating with AWS.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct ImdsAuthentication {
    /// Number of IMDS retries for fetching tokens and metadata.
    #[serde(default = "default_max_attempts")]
    max_attempts: u32,
}

impl Default for ImdsAuthentication {
    fn default() -> Self {
        Self {
            max_attempts: default_max_attempts(),
        }
    }
}

const fn default_max_attempts() -> u32 {
    5
}

/// Configuration of the authentication strategy for interacting with AWS services.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(untagged)]
pub enum AwsAuthentication {
    /// Authenticate using a fixed access key and secret pair.
    AccessKey {
        /// The AWS access key ID.
        access_key_id: String,

        /// The AWS secret access key.
        secret_access_key: String,

        /// The ARN of an IAM role to assume.
        assume_role: Option<String>,

        /// The AWS region to send STS requests to.
        region: Option<String>,
    },

    /// Authenticate using credentials stored in a file.
    File {
        /// Path to the credentials file.
        credentials_file: String,

        /// The credentials profile to use.
        #[serde(default = "default_profile")]
        profile: String,
    },

    /// Assume the given role ARN.
    Role {
        /// The ARN of an IAM role to assume.
        assume_role: String,

        /// Timeout for assuming the role, in seconds.
        load_timeout_secs: Option<u64>,

        /// Configuration for authenticating with AWS through IMDS.
        #[serde(default)]
        imds: ImdsAuthentication,

        /// The AWS region to send STS requests to.
        region: Option<String>,
    },

    /// Default authentication strategy which tries a variety of substrategies in a one-after-the-other fashion.
    Default {
        /// Timeout for successfully loading any credentials, in seconds.
        load_timeout_secs: Option<u64>,

        /// Configuration for authenticating with AWS through IMDS.
        #[serde(default)]
        imds: ImdsAuthentication,
    },
}

impl Default for AwsAuthentication {
    fn default() -> Self {
        Self::Default {
            load_timeout_secs: None,
            imds: ImdsAuthentication::default(),
        }
    }
}

fn default_profile() -> String {
    "default".to_string()
}

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
pub struct TlsEnableableConfig {
    /// Whether or not TLS is enabled.
    pub enabled: bool,

    #[serde(flatten)]
    pub options: TlsConfig,
}

impl Default for TlsEnableableConfig {
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
#[configurable_component]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[serde(try_from = "String", into = "String")]
#[configurable(metadata(docs::examples = "0.0.0.0:9000"))]
#[configurable(metadata(docs::examples = "systemd"))]
#[configurable(metadata(docs::examples = "systemd#3"))]
pub enum SocketListenAddr {
    /// An IPv4/IPv6 address and port.
    SocketAddr(SocketAddr),

    /// A file descriptor identifier that is given from, and managed by, the socket activation feature of `systemd`.
    SystemdFd(usize),
}

impl From<SocketAddr> for SocketListenAddr {
    fn from(addr: SocketAddr) -> Self {
        Self::SocketAddr(addr)
    }
}

impl From<usize> for SocketListenAddr {
    fn from(fd: usize) -> Self {
        Self::SystemdFd(fd)
    }
}

impl TryFrom<String> for SocketListenAddr {
    type Error = String;

    fn try_from(input: String) -> Result<Self, Self::Error> {
        // first attempt to parse the string into a SocketAddr directly
        match input.parse::<SocketAddr>() {
            Ok(socket_addr) => Ok(socket_addr.into()),

            // then attempt to parse a systemd file descriptor
            Err(_) => {
                let fd: usize = match input.as_str() {
                    "systemd" => Ok(0),
                    s if s.starts_with("systemd#") => s[8..]
                        .parse::<usize>()
                        .map_err(|_| "failed to parse usize".to_string())?
                        .checked_sub(1)
                        .ok_or_else(|| "systemd indices start at 1".to_string()),

                    // otherwise fail
                    _ => Err("unable to parse".to_string()),
                }?;

                Ok(fd.into())
            }
        }
    }
}

impl From<SocketListenAddr> for String {
    fn from(addr: SocketListenAddr) -> String {
        match addr {
            SocketListenAddr::SocketAddr(addr) => addr.to_string(),
            SocketListenAddr::SystemdFd(fd) => {
                if fd == 0 {
                    "systemd".to_owned()
                } else {
                    format!("systemd#{}", fd)
                }
            }
        }
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
    SocketListenAddr::SocketAddr(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 9200)))
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

/// A sink for sending events to the AWS Bleep Bloop service.
#[derive(Clone)]
#[configurable_component(sink("aws_bleep_bloop"))]
#[configurable(metadata(status = "stable"))]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct AwsBleepBloopSinkConfig {
    #[configurable(derived)]
    #[serde(default)]
    auth: AwsAuthentication,

    /// The Bleep Bloop folder ID.
    #[configurable(validation(pattern = "foo\\d+"))]
    folder_id: String,

    #[configurable(derived)]
    #[serde(default = "default_aws_bleep_bloop_sink_batch")]
    batch: BatchConfig,

    #[configurable(deprecated, derived)]
    #[serde(default = "default_aws_bleep_bloop_sink_encoding")]
    encoding: Encoding,

    /// Overridden TLS description.
    #[configurable(derived)]
    tls: Option<TlsEnableableConfig>,

    /// The partition key to use for each event.
    #[configurable(metadata(docs::templateable))]
    #[serde(default = "default_partition_key")]
    partition_key: String,

    /// The tags to apply to each event.
    tags: HashMap<String, TagConfig>,

    /// The headers to apply to each event.
    headers: HashMap<String, Vec<String>>,
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

impl GenerateConfig for AwsBleepBloopSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            auth: AwsAuthentication::default(),
            folder_id: String::from("foo12"),
            batch: default_aws_bleep_bloop_sink_batch(),
            encoding: default_aws_bleep_bloop_sink_encoding(),
            tls: None,
            partition_key: default_partition_key(),
            tags: HashMap::new(),
            headers: HashMap::new(),
        })
        .unwrap()
    }
}

fn default_aws_bleep_bloop_sink_batch() -> BatchConfig {
    BatchConfig {
        max_events: Some(NonZeroU64::new(5678).expect("must be nonzero")),
        max_bytes: Some(NonZeroU64::new(36_000_000).expect("must be nonzero")),
        timeout: Some(SpecialDuration(15)),
    }
}

fn default_partition_key() -> String {
    "foo".to_string()
}

const fn default_aws_bleep_bloop_sink_encoding() -> Encoding {
    Encoding::Json { pretty: true }
}

fn default_aws_bleep_bloop_sink_endpoint() -> String {
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

    /// AWS Bleep Bloop sink.
    AwsBleepBloop(AwsBleepBloopSinkConfig),
}

#[derive(Clone)]
#[configurable_component]
#[configurable(description = "Global options for configuring Vector.")]
pub struct GlobalOptions {
    /// The data directory where Vector will store state.
    data_dir: Option<String>,

    /// A map of additional tags for metrics.
    tags: Option<IndexMap<String, String>>,
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
