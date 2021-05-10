pub mod http;

/// A provider returns an initial configuration string, if successful.
pub type Result = std::result::Result<String, &'static str>;
