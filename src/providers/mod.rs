pub mod http;

use super::config::ConfigBuilder;

/// A provider returns a `ConfigBuilder` and config warnings, if successful.
pub(crate) type Result = std::result::Result<ConfigBuilder, Vec<String>>;
