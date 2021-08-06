mod bytes;
#[cfg(feature = "sources-syslog")]
mod syslog;

pub use self::bytes::BytesParser;
#[cfg(feature = "sources-syslog")]
pub use self::syslog::SyslogParser;
pub use super::Parser;
