use std::borrow::Cow;

mod adaptive_concurrency;
mod add_fields;
mod add_tags;
mod aggregate;
mod ansi_stripper;
#[cfg(feature = "sources-apache_metrics")]
mod apache_metrics;
#[cfg(feature = "api")]
mod api;
#[cfg(feature = "transforms-aws_cloudwatch_logs_subscription_parser")]
mod aws_cloudwatch_logs_subscription_parser;
#[cfg(feature = "transforms-aws_ec2_metadata")]
mod aws_ec2_metadata;
#[cfg(feature = "sources-aws_ecs_metrics")]
mod aws_ecs_metrics;
#[cfg(feature = "sources-aws_kinesis_firehose")]
mod aws_kinesis_firehose;
#[cfg(feature = "sinks-aws_kinesis_streams")]
mod aws_kinesis_streams;
#[cfg(feature = "sources-aws_s3")]
pub(crate) mod aws_s3;
#[cfg(feature = "sinks-aws_s3")]
pub(crate) mod aws_s3_sink;
#[cfg(feature = "sinks-aws_sqs")]
mod aws_sqs;
#[cfg(any(feature = "sinks-azure_blob", feature = "sinks-datadog_archives"))]
pub(crate) mod azure_blob;
mod batch;
mod blackhole;
#[cfg(feature = "transforms-coercer")]
mod coercer;
mod common;
#[cfg(feature = "transforms-concat")]
mod concat;
mod conditions;
#[cfg(feature = "sinks-console")]
mod console;
#[cfg(feature = "sinks-datadog_events")]
mod datadog_events;
#[cfg(feature = "sinks-datadog_logs")]
mod datadog_logs;
#[cfg(feature = "sinks-datadog_metrics")]
mod datadog_metrics;
#[cfg(any(feature = "codecs"))]
mod decoder;
#[cfg(feature = "transforms-dedupe")]
mod dedupe;
#[cfg(feature = "sources-demo_logs")]
mod demo_logs;
#[cfg(feature = "sources-dnstap")]
mod dnstap;
#[cfg(feature = "sources-docker_logs")]
mod docker_logs;
mod elasticsearch;
mod encoding_transcode;
#[cfg(feature = "sources-eventstoredb_metrics")]
mod eventstoredb_metrics;
#[cfg(feature = "sources-exec")]
mod exec;
#[cfg(feature = "transforms-filter")]
mod filter;
#[cfg(feature = "sources-fluent")]
mod fluent;
#[cfg(feature = "transforms-geoip")]
mod geoip;
#[cfg(feature = "transforms-grok_parser")]
mod grok_parser;
mod heartbeat;
#[cfg(feature = "sources-host_metrics")]
mod host_metrics;
mod http;
pub mod http_client;
#[cfg(all(unix, feature = "sources-journald"))]
mod journald;
#[cfg(feature = "transforms-json_parser")]
mod json_parser;
#[cfg(any(feature = "sources-kafka", feature = "sinks-kafka"))]
mod kafka;
#[cfg(feature = "transforms-key_value_parser")]
mod key_value_parser;
#[cfg(feature = "sources-kubernetes_logs")]
mod kubernetes_logs;
#[cfg(feature = "transforms-log_to_metric")]
mod log_to_metric;
#[cfg(feature = "transforms-logfmt_parser")]
mod logfmt_parser;
mod logplex;
#[cfg(feature = "sinks-loki")]
mod loki;
#[cfg(feature = "transforms-lua")]
mod lua;
#[cfg(feature = "transforms-metric_to_log")]
mod metric_to_log;
#[cfg(feature = "sources-mongodb_metrics")]
mod mongodb_metrics;
#[cfg(any(feature = "sources-nats", feature = "sinks-nats"))]
mod nats;
#[cfg(feature = "sources-nginx_metrics")]
mod nginx_metrics;
mod open;
#[cfg(feature = "sources-postgresql_metrics")]
mod postgresql_metrics;
mod process;
#[cfg(any(feature = "sources-prometheus", feature = "sinks-prometheus"))]
mod prometheus;
mod pulsar;
#[cfg(feature = "sinks-redis")]
mod redis;
#[cfg(feature = "transforms-reduce")]
mod reduce;
#[cfg(feature = "transforms-regex_parser")]
mod regex_parser;
mod remap;
#[cfg(feature = "transforms-remove_fields")]
mod remove_fields;
#[cfg(feature = "transforms-rename_fields")]
mod rename_fields;
#[cfg(feature = "transforms-route")]
mod route;
mod sample;
#[cfg(feature = "sinks-sematext")]
mod sematext_metrics;
mod socket;
mod split;
#[cfg(any(feature = "sources-splunk_hec", feature = "sinks-splunk_hec"))]
mod splunk_hec;
#[cfg(feature = "sinks-statsd")]
mod statsd_sink;
#[cfg(feature = "sources-statsd")]
mod statsd_source;
mod stdin;
mod syslog;
#[cfg(feature = "transforms-tag_cardinality_limit")]
mod tag_cardinality_limit;
mod tcp;
mod template;
#[cfg(feature = "transforms-throttle")]
mod throttle;
#[cfg(feature = "transforms-tokenizer")]
mod tokenizer;
mod udp;
mod unix;
mod vector;

#[cfg(any(
    feature = "sources-file",
    feature = "sources-kubernetes_logs",
    feature = "sinks-file",
))]
mod file;
mod windows;

pub mod kubernetes;

#[cfg(feature = "sources-mongodb_metrics")]
pub use mongodb_metrics::*;

#[cfg(feature = "sources-apache_metrics")]
pub use self::apache_metrics::*;
#[cfg(feature = "api")]
pub use self::api::*;
#[cfg(feature = "transforms-aws_cloudwatch_logs_subscription_parser")]
pub(crate) use self::aws_cloudwatch_logs_subscription_parser::*;
#[cfg(feature = "transforms-aws_ec2_metadata")]
pub use self::aws_ec2_metadata::*;
#[cfg(feature = "sources-aws_ecs_metrics")]
pub use self::aws_ecs_metrics::*;
#[cfg(feature = "sources-aws_kinesis_firehose")]
pub use self::aws_kinesis_firehose::*;
#[cfg(feature = "sinks-aws_kinesis_streams")]
pub use self::aws_kinesis_streams::*;
#[cfg(feature = "sinks-aws_s3")]
pub use self::aws_s3_sink::*;
#[cfg(feature = "sinks-aws_sqs")]
pub use self::aws_sqs::*;
#[cfg(feature = "transforms-coercer")]
pub(crate) use self::coercer::*;
#[cfg(feature = "transforms-concat")]
pub use self::concat::*;
#[cfg(feature = "sinks-console")]
pub use self::console::*;
#[cfg(feature = "sinks-datadog_events")]
pub use self::datadog_events::*;
#[cfg(feature = "sinks-datadog_logs")]
pub use self::datadog_logs::*;
#[cfg(feature = "sinks-datadog_metrics")]
pub use self::datadog_metrics::*;
#[cfg(any(feature = "codecs"))]
pub use self::decoder::*;
#[cfg(feature = "transforms-dedupe")]
pub(crate) use self::dedupe::*;
#[cfg(feature = "sources-demo_logs")]
pub use self::demo_logs::*;
#[cfg(feature = "sources-dnstap")]
pub(crate) use self::dnstap::*;
#[cfg(feature = "sources-docker_logs")]
pub use self::docker_logs::*;
#[cfg(feature = "sources-eventstoredb_metrics")]
pub use self::eventstoredb_metrics::*;
#[cfg(feature = "sources-exec")]
pub use self::exec::*;
#[cfg(any(
    feature = "sources-file",
    feature = "sources-kubernetes_logs",
    feature = "sinks-file",
))]
pub use self::file::*;
#[cfg(feature = "transforms-filter")]
pub use self::filter::*;
#[cfg(feature = "sources-fluent")]
pub use self::fluent::*;
#[cfg(feature = "transforms-geoip")]
pub(crate) use self::geoip::*;
#[cfg(feature = "transforms-grok_parser")]
pub(crate) use self::grok_parser::*;
#[cfg(feature = "sources-host_metrics")]
pub(crate) use self::host_metrics::*;
#[cfg(any(
    feature = "sources-utils-http",
    feature = "sources-utils-http-encoding",
    feature = "sinks-http",
    feature = "sources-datadog_agent",
    feature = "sources-splunk_hec",
    feature = "sources-aws_ecs_metrics",
))]
pub(crate) use self::http::*;
#[cfg(all(unix, feature = "sources-journald"))]
pub(crate) use self::journald::*;
#[cfg(feature = "transforms-json_parser")]
pub(crate) use self::json_parser::*;
#[cfg(any(feature = "sources-kafka", feature = "sinks-kafka"))]
pub use self::kafka::*;
#[cfg(feature = "transforms-key_value_parser")]
pub(crate) use self::key_value_parser::*;
#[cfg(feature = "sources-kubernetes_logs")]
pub use self::kubernetes_logs::*;
#[cfg(feature = "transforms-log_to_metric")]
pub(crate) use self::log_to_metric::*;
#[cfg(feature = "transforms-logfmt_parser")]
pub use self::logfmt_parser::*;
#[cfg(feature = "sinks-loki")]
pub(crate) use self::loki::*;
#[cfg(feature = "transforms-lua")]
pub use self::lua::*;
#[cfg(feature = "transforms-metric_to_log")]
pub(crate) use self::metric_to_log::*;
#[cfg(any(feature = "sources-nats", feature = "sinks-nats"))]
pub use self::nats::*;
#[cfg(feature = "sources-nginx_metrics")]
pub(crate) use self::nginx_metrics::*;
#[cfg(feature = "sources-postgresql_metrics")]
pub(crate) use self::postgresql_metrics::*;
#[cfg(any(feature = "sources-prometheus", feature = "sinks-prometheus"))]
pub(crate) use self::prometheus::*;
#[cfg(feature = "sinks-redis")]
pub use self::redis::*;
#[cfg(feature = "transforms-reduce")]
pub(crate) use self::reduce::*;
#[cfg(feature = "transforms-regex_parser")]
pub(crate) use self::regex_parser::*;
#[cfg(feature = "transforms-remove_fields")]
pub use self::remove_fields::*;
#[cfg(feature = "transforms-rename_fields")]
pub use self::rename_fields::*;
#[cfg(feature = "transforms-route")]
pub use self::route::*;
#[cfg(feature = "sinks-sematext")]
pub use self::sematext_metrics::*;
pub(crate) use self::socket::*;
#[cfg(any(feature = "sources-splunk_hec", feature = "sinks-splunk_hec"))]
pub(crate) use self::splunk_hec::*;
#[cfg(feature = "sinks-statsd")]
pub use self::statsd_sink::*;
#[cfg(feature = "sources-statsd")]
pub use self::statsd_source::*;
#[cfg(feature = "transforms-tag_cardinality_limit")]
pub(crate) use self::tag_cardinality_limit::*;
#[cfg(feature = "transforms-throttle")]
pub use self::throttle::*;
#[cfg(feature = "transforms-tokenizer")]
pub(crate) use self::tokenizer::*;
#[cfg(windows)]
pub use self::windows::*;
pub use self::{
    adaptive_concurrency::*, add_fields::*, add_tags::*, aggregate::*, ansi_stripper::*, batch::*,
    blackhole::*, common::*, conditions::*, elasticsearch::*, encoding_transcode::*, heartbeat::*,
    logplex::*, open::*, process::*, pulsar::*, remap::*, sample::*, split::*, stdin::*, syslog::*,
    tcp::*, template::*, udp::*, unix::*, vector::*,
};

// this version won't be needed once all `InternalEvent`s implement `name()`
#[cfg(test)]
#[macro_export]
macro_rules! emit {
    ($event:expr) => {
        vector_core::internal_event::emit(&vector_core::internal_event::DefaultName {
            event: $event,
            name: stringify!($event),
        })
    };
}

#[cfg(not(test))]
#[macro_export]
macro_rules! emit {
    ($event:expr) => {
        vector_core::internal_event::emit($event)
    };
}

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
        let message = "Hello 😁 this is test.";
        assert_eq!("Hello [...]", super::truncate_string_at(message, 13));
    }
}
