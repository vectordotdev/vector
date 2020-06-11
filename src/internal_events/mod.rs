mod add_fields;
mod aws_kinesis_streams;
mod blackhole;
mod elasticsearch;
mod file;
mod json;
#[cfg(feature = "transforms-lua")]
mod lua;
#[cfg(feature = "sources-prometheus")]
mod prometheus;
mod regex;
mod splunk_hec;
mod syslog;
mod tcp;
mod udp;
mod unix;
mod vector;
#[cfg(feature = "wasm")]
mod wasm;

pub use self::add_fields::*;
pub use self::aws_kinesis_streams::*;
pub use self::blackhole::*;
pub use self::elasticsearch::*;
pub use self::file::*;
pub use self::json::*;
#[cfg(feature = "transforms-lua")]
pub use self::lua::*;
#[cfg(feature = "sources-prometheus")]
pub use self::prometheus::*;
pub use self::regex::*;
pub use self::splunk_hec::*;
pub use self::syslog::*;
pub use self::tcp::*;
pub use self::udp::*;
pub use self::unix::*;
pub use self::vector::*;
#[cfg(feature = "wasm")]
pub use self::wasm::*;

pub trait InternalEvent: std::fmt::Debug {
    fn emit_logs(&self) {}
    fn emit_metrics(&self) {}
}

pub fn emit(event: impl InternalEvent) {
    event.emit_logs();
    event.emit_metrics();
}

#[macro_export]
macro_rules! emit {
    ($event:expr) => {
        $crate::internal_events::emit($event);
    };
}
