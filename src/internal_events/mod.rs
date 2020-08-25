mod add_fields;
mod add_tags;
mod ansi_stripper;
mod auto_concurrency;
mod aws_kinesis_streams;
mod blackhole;
#[cfg(feature = "sources-docker")]
mod docker;
mod elasticsearch;
mod heartbeat;
mod http;
#[cfg(all(unix, feature = "sources-journald"))]
mod journald;
#[cfg(feature = "transforms-json_parser")]
mod json_parser;
#[cfg(feature = "sources-kafka")]
mod kafka;
#[cfg(feature = "sources-kubernetes-logs")]
mod kubernetes_logs;
mod logplex;
#[cfg(feature = "transforms-lua")]
mod lua;
mod process;
#[cfg(feature = "sources-prometheus")]
mod prometheus;
mod regex;
mod sampler;
#[cfg(any(
    feature = "sources-socket",
    feature = "sources-syslog",
    feature = "sources-vector"
))]
mod socket;
mod split;
#[cfg(any(feature = "sources-splunk_hec", feature = "sinks-splunk_hec"))]
mod splunk_hec;
#[cfg(feature = "sources-statsd")]
mod statsd;
mod stdin;
mod syslog;
mod tcp;
mod unix;
mod vector;
#[cfg(feature = "wasm")]
mod wasm;

pub mod kubernetes;

pub use self::add_fields::*;
pub use self::add_tags::*;
pub use self::ansi_stripper::*;
pub use self::auto_concurrency::*;
pub use self::aws_kinesis_streams::*;
pub use self::blackhole::*;
#[cfg(feature = "sources-docker")]
pub use self::docker::*;
pub use self::elasticsearch::*;
pub use self::file::*;
pub use self::heartbeat::*;
pub use self::http::*;
#[cfg(all(unix, feature = "sources-journald"))]
pub(crate) use self::journald::*;
#[cfg(feature = "transforms-json_parser")]
pub(crate) use self::json_parser::*;
#[cfg(feature = "sources-kafka")]
pub use self::kafka::*;
#[cfg(feature = "sources-kubernetes-logs")]
pub use self::kubernetes_logs::*;
pub use self::logplex::*;
#[cfg(feature = "transforms-lua")]
pub use self::lua::*;
pub use self::process::*;
#[cfg(feature = "sources-prometheus")]
pub use self::prometheus::*;
pub use self::regex::*;
pub use self::sampler::*;
#[cfg(any(
    feature = "sources-socket",
    feature = "sources-syslog",
    feature = "sources-vector"
))]
pub(crate) use self::socket::*;
pub use self::split::*;
#[cfg(any(feature = "sources-splunk_hec", feature = "sinks-splunk_hec"))]
pub(crate) use self::splunk_hec::*;
#[cfg(feature = "sources-statsd")]
pub use self::statsd::*;
pub use self::stdin::*;
pub use self::syslog::*;
pub use self::tcp::*;
pub use self::unix::*;
pub use self::vector::*;
#[cfg(feature = "wasm")]
pub use self::wasm::*;

pub trait InternalEvent {
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

// Modules that require emit! macro so they need to be defined after the macro.
mod file;
