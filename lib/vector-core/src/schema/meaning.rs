//! Constants for commonly used semantic meanings.

/// The service typically represents the application that generated the event.
pub const SERVICE: &str = "service";

/// The main text message of the event.
pub const MESSAGE: &str = "message";

/// The main timestamp of the event.
pub const TIMESTAMP: &str = "timestamp";

/// The hostname of the machine where the event was generated.
pub const HOST: &str = "host";

/// The tags of an event, generally a key-value paired list.
pub const TAGS: &str = "tags";

pub const SOURCE: &str = "source";
pub const SEVERITY: &str = "severity";
pub const TRACE_ID: &str = "trace_id";
