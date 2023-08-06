//! Contains common definitions for GELF codec support

use once_cell::sync::Lazy;
use regex::Regex;

/// GELF Message fields. Definitions from <https://docs.graylog.org/docs/gelf>.
pub mod gelf_fields {

    /// (not a field) The latest version of the GELF specification.
    pub const GELF_VERSION: &str = "1.1";

    /// (required) GELF spec version
    pub const VERSION: &str = "version";

    /// (required) The name of the host, source or application that sent this message.
    pub const HOST: &str = "host";

    /// (required) A short descriptive message.
    pub const SHORT_MESSAGE: &str = "short_message";

    /// (optional) A long message that can i.e. contain a backtrace
    pub const FULL_MESSAGE: &str = "full_message";

    /// (optional) Seconds since UNIX epoch with optional decimal places for milliseconds.
    ///  SHOULD be set by client library. Will be set to the current timestamp (now) by the server if absent.
    pub const TIMESTAMP: &str = "timestamp";

    /// (optional) The level equal to the standard syslog levels. default is 1 (ALERT).
    pub const LEVEL: &str = "level";

    /// (optional) (deprecated) Send as additional field instead.
    pub const FACILITY: &str = "facility";

    /// (optional) (deprecated) The line in a file that caused the error (decimal). Send as additional field instead.
    pub const LINE: &str = "line";

    /// (optional) (deprecated) The file (with path if you want) that caused the error. Send as additional field instead.
    pub const FILE: &str = "file";

    // < Every field with an underscore (_) prefix will be treated as an additional field. >
}

/// Regex for matching valid field names. Must contain only word chars, periods and dashes.
/// Additional field names must also be prefixed with an `_` , however that is intentionally
/// omitted from this regex to be checked separately to create a specific error message.
pub static VALID_FIELD_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[\w\.\-]*$").unwrap());
