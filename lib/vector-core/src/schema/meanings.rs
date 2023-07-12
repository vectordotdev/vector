///! Constants for commonly used meanings.

/// The service typically represents the application that generated the event.
pub const SERVICE: &'static str = "service";

/// The main text message of the event.
pub const MESSAGE: &'static str = "message";

/// The main timestamp of the event.
pub const TIMESTAMP: &'static str = "timestamp";

/// The hostname of the machine where the event was generated.
pub const HOST: &'static str = "host";

pub const SOURCE: &'static str = "source";
pub const SEVERITY: &'static str = "severity";
pub const TRACE_ID: &'static str = "trace_id";
