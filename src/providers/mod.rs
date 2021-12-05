pub mod http;

use super::config::ConfigBuilder;

/// A provider returns a `ConfigBuilder` and config warnings, if successful.
pub type Result = std::result::Result<ConfigBuilder, Vec<String>>;
