use std::borrow::Cow;

mod add_fields;
mod add_tags;
mod ansi_stripper;
#[cfg(feature = "sources-apache_metrics")]
mod apache_metrics;
#[cfg(feature = "api")]
mod api;
mod auto_concurrency;
#[cfg(feature = "transforms-aws_cloudwatch_logs_subscription_parser")]
mod aws_cloudwatch_logs_subscription_parser;
#[cfg(feature = "sources-aws_kinesis_firehose")]
mod aws_kinesis_firehose;
mod aws_kinesis_streams;
mod blackhole;
#[cfg(feature = "transforms-coercer")]
mod coercer;
#[cfg(feature = "transforms-concat")]
mod concat;
#[cfg(feature = "sinks-console")]
mod console;
#[cfg(feature = "transforms-dedupe")]
mod dedupe;
#[cfg(feature = "sources-docker")]
mod docker;
mod elasticsearch;
#[cfg(feature = "sources-generator")]
mod generator;
#[cfg(feature = "transforms-grok_parser")]
mod grok_parser;
mod heartbeat;
#[cfg(feature = "sources-host_metrics")]
mod host_metrics;
mod http;
#[cfg(all(unix, feature = "sources-journald"))]
mod journald;
#[cfg(feature = "transforms-json_parser")]
mod json_parser;
#[cfg(feature = "sources-kafka")]
mod kafka;
#[cfg(feature = "sources-kubernetes-logs")]
mod kubernetes_logs;
#[cfg(feature = "transforms-log_to_metric")]
mod log_to_metric;
mod logplex;
#[cfg(feature = "transforms-lua")]
mod lua;
#[cfg(feature = "transforms-metric_to_log")]
mod metric_to_log;
mod process;
#[cfg(feature = "sources-prometheus")]
mod prometheus;
#[cfg(feature = "transforms-reduce")]
mod reduce;
#[cfg(feature = "transforms-regex_parser")]
mod regex_parser;
mod remap;
#[cfg(feature = "transforms-remove_fields")]
mod remove_fields;
#[cfg(feature = "transforms-remove_tags")]
mod remove_tags;
#[cfg(feature = "transforms-rename_fields")]
mod rename_fields;
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
#[cfg(feature = "sinks-statsd")]
mod statsd_sink;
#[cfg(feature = "sources-statsd")]
mod statsd_source;
mod stdin;
#[cfg(feature = "transforms-swimlanes")]
mod swimlanes;
mod syslog;
#[cfg(feature = "transforms-tag_cardinality_limit")]
mod tag_cardinality_limit;
mod tcp;
#[cfg(feature = "transforms-tokenizer")]
mod tokenizer;
mod udp;
mod unix;
mod vector;
#[cfg(feature = "wasm")]
mod wasm;

pub mod kubernetes;

pub use self::add_fields::*;
pub use self::add_tags::*;
pub use self::ansi_stripper::*;
#[cfg(feature = "sources-apache_metrics")]
pub use self::apache_metrics::*;
#[cfg(feature = "api")]
pub use self::api::*;
pub use self::auto_concurrency::*;
#[cfg(feature = "transforms-aws_cloudwatch_logs_subscription_parser")]
pub(crate) use self::aws_cloudwatch_logs_subscription_parser::*;
#[cfg(feature = "sources-aws_kinesis_firehose")]
pub use self::aws_kinesis_firehose::*;
pub use self::aws_kinesis_streams::*;
pub use self::blackhole::*;
#[cfg(feature = "transforms-coercer")]
pub(crate) use self::coercer::*;
#[cfg(feature = "transforms-concat")]
pub use self::concat::*;
#[cfg(feature = "sinks-console")]
pub use self::console::*;
#[cfg(feature = "transforms-dedupe")]
pub(crate) use self::dedupe::*;
#[cfg(feature = "sources-docker")]
pub use self::docker::*;
pub use self::elasticsearch::*;
#[cfg(any(feature = "sources-file", feature = "sources-kubernetes-logs"))]
pub use self::file::*;
#[cfg(feature = "sources-generator")]
pub use self::generator::*;
#[cfg(feature = "transforms-grok_parser")]
pub(crate) use self::grok_parser::*;
pub use self::heartbeat::*;
#[cfg(feature = "sources-host_metrics")]
pub(crate) use self::host_metrics::*;
pub use self::http::*;
#[cfg(all(unix, feature = "sources-journald"))]
pub(crate) use self::journald::*;
#[cfg(feature = "transforms-json_parser")]
pub(crate) use self::json_parser::*;
#[cfg(feature = "sources-kafka")]
pub use self::kafka::*;
#[cfg(feature = "sources-kubernetes-logs")]
pub use self::kubernetes_logs::*;
#[cfg(feature = "transforms-log_to_metric")]
pub(crate) use self::log_to_metric::*;
pub use self::logplex::*;
#[cfg(feature = "transforms-lua")]
pub use self::lua::*;
#[cfg(feature = "transforms-metric_to_log")]
pub(crate) use self::metric_to_log::*;
pub use self::process::*;
#[cfg(feature = "sources-prometheus")]
pub use self::prometheus::*;
#[cfg(feature = "transforms-reduce")]
pub(crate) use self::reduce::*;
#[cfg(feature = "transforms-regex_parser")]
pub(crate) use self::regex_parser::*;
pub use self::remap::*;
#[cfg(feature = "transforms-remove_fields")]
pub use self::remove_fields::*;
#[cfg(feature = "transforms-remove_tags")]
pub use self::remove_tags::*;
#[cfg(feature = "transforms-rename_fields")]
pub use self::rename_fields::*;
pub use self::sampler::*;
#[cfg(any(feature = "sources-socket", feature = "sources-syslog"))]
pub(crate) use self::socket::*;
pub use self::split::*;
#[cfg(any(feature = "sources-splunk_hec", feature = "sinks-splunk_hec"))]
pub(crate) use self::splunk_hec::*;
#[cfg(feature = "sinks-statsd")]
pub use self::statsd_sink::*;
#[cfg(feature = "sources-statsd")]
pub use self::statsd_source::*;
pub use self::stdin::*;
#[cfg(feature = "transforms-swimlanes")]
pub use self::swimlanes::*;
pub use self::syslog::*;
#[cfg(feature = "transforms-tag_cardinality_limit")]
pub(crate) use self::tag_cardinality_limit::*;
pub use self::tcp::*;
#[cfg(feature = "transforms-tokenizer")]
pub(crate) use self::tokenizer::*;
pub use self::udp::*;
pub use self::unix::*;
pub use self::vector::*;
#[cfg(feature = "wasm")]
pub use self::wasm::*;
#[cfg(windows)]
pub use self::windows::*;

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
#[cfg(any(feature = "sources-file", feature = "sources-kubernetes-logs"))]
mod file;
mod windows;

const ELLIPSIS: &str = "[...]";

pub fn truncate_string_at(s: &str, maxlen: usize) -> Cow<str> {
    if s.len() >= maxlen {
        let mut len = maxlen - ELLIPSIS.len();
        while !s.is_char_boundary(len) {
            len -= 1;
        }
        format!("{}{}", &s[..len], ELLIPSIS).into()
    } else {
        s.into()
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn truncate_utf8() {
        let message = "hello 😁 this is test";
        assert_eq!("hello [...]", super::truncate_string_at(&message, 13));
    }
}
