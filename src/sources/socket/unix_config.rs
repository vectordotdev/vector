use std::path::PathBuf;

use vector_lib::{
    codecs::decoding::{DeserializerConfig, FramingConfig},
    configurable::configurable_component,
    lookup::lookup_v2::OptionalValuePath,
};

use super::default_host_key;
use crate::serde::default_decoding;

/// Unix domain socket configuration for the `socket` source.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct UnixConfig {
    /// The Unix socket path.
    ///
    /// This should be an absolute path.
    #[configurable(metadata(docs::examples = "/path/to/socket"))]
    pub path: PathBuf,

    /// Unix file mode bits to be applied to the unix socket file as its designated file permissions.
    ///
    /// Note: The file mode value can be specified in any numeric format supported by your configuration
    /// language, but it is most intuitive to use an octal number.
    #[configurable(metadata(docs::examples = 0o777))]
    #[configurable(metadata(docs::examples = 0o600))]
    #[configurable(metadata(docs::examples = 508))]
    pub socket_file_mode: Option<u32>,

    /// Overrides the name of the log field used to add the peer host to each event.
    ///
    /// The value will be the peer host's address, including the port i.e. `1.2.3.4:9000`.
    ///
    /// By default, the [global `log_schema.host_key` option][global_host_key] is used.
    ///
    /// Set to `""` to suppress this key.
    ///
    /// [global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
    #[serde(default = "default_host_key")]
    pub host_key: OptionalValuePath,

    #[serde(default)]
    pub framing: Option<FramingConfig>,

    #[serde(default = "default_decoding")]
    pub decoding: DeserializerConfig,

    /// The namespace to use for logs. This overrides the global setting.
    #[serde(default)]
    #[configurable(metadata(docs::hidden))]
    pub log_namespace: Option<bool>,
}

impl UnixConfig {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            socket_file_mode: None,
            host_key: default_host_key(),
            framing: None,
            decoding: default_decoding(),
            log_namespace: None,
        }
    }

    pub const fn decoding(&self) -> &DeserializerConfig {
        &self.decoding
    }

    pub const fn host_key(&self) -> &OptionalValuePath {
        &self.host_key
    }
}
