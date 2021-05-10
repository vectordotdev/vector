pub mod http;

use super::config::ConfigBuilder;

/// A provider returns an initial configuration string, if successful.
pub type Result = std::result::Result<ConfigBuilder, &'static str>;
