use snafu::Snafu;

#[cfg(feature = "transforms-aggregate")]
pub mod aggregate;
#[cfg(feature = "transforms-aws_cloudwatch_logs_subscription_parser")]
pub mod aws_cloudwatch_logs_subscription_parser;
#[cfg(feature = "transforms-aws_ec2_metadata")]
pub mod aws_ec2_metadata;
#[cfg(feature = "transforms-dedupe")]
pub mod dedupe;
#[cfg(feature = "transforms-field_filter")]
pub mod field_filter;
#[cfg(feature = "transforms-filter")]
pub mod filter;
#[cfg(feature = "transforms-geoip")]
pub mod geoip;
#[cfg(feature = "transforms-log_to_metric")]
pub mod log_to_metric;
#[cfg(feature = "transforms-lua")]
pub mod lua;
#[cfg(feature = "sources-kubernetes_logs")]
pub mod merge;
#[cfg(feature = "transforms-metric_to_log")]
pub mod metric_to_log;
#[cfg(feature = "transforms-pipelines")]
pub mod pipelines;
#[cfg(feature = "transforms-reduce")]
pub mod reduce;
#[cfg(feature = "sources-kubernetes_logs")]
pub mod regex_parser;
#[cfg(feature = "transforms-remap")]
pub mod remap;
#[cfg(feature = "transforms-route")]
pub mod route;
#[cfg(feature = "transforms-sample")]
pub mod sample;
#[cfg(feature = "transforms-tag_cardinality_limit")]
pub mod tag_cardinality_limit;
#[cfg(feature = "transforms-throttle")]
pub mod throttle;

use vector_config::configurable_component;
pub use vector_core::transform::{
    FunctionTransform, OutputBuffer, SyncTransform, TaskTransform, Transform, TransformOutputs,
    TransformOutputsBuf,
};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Invalid regular expression: {}", source))]
    InvalidRegex { source: regex::Error },

    #[snafu(display("Invalid substring expression: {}", name))]
    InvalidSubstring { name: String },
}

/// Configurable transforms in Vector.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Transforms {
    /// Aggregate.
    Aggregate(#[configurable(derived)] aggregate::AggregateConfig),

    /// AWS Cloudwatch Logs subscription parser.
    AwsCloudwatchLogsSubscriptionParser(
        #[configurable(derived)]
        aws_cloudwatch_logs_subscription_parser::AwsCloudwatchLogsSubscriptionParserConfig,
    ),

    /// AWS EC2 metadata.
    AwsEc2Metadata(#[configurable(derived)] aws_ec2_metadata::Ec2Metadata),

    /// Dedupe.
    Dedupe(#[configurable(derived)] dedupe::DedupeConfig),

    /// Field filter.
    FieldFilter(#[configurable(derived)] field_filter::FieldFilterConfig),

    /// Filter.
    Filter(#[configurable(derived)] filter::FilterConfig),

    /// GeoIP.
    Geoip(#[configurable(derived)] geoip::GeoipConfig),

    /// Log to metric.
    LogToMetric(#[configurable(derived)] log_to_metric::LogToMetricConfig),

    /// Lua.
    Lua(#[configurable(derived)] lua::LuaConfig),

    /// Metric to log.
    MetricToLog(#[configurable(derived)] metric_to_log::MetricToLogConfig),

    /// Pipelines.
    Pipelines(#[configurable(derived)] pipelines::PipelinesConfig),

    /// Reduce.
    Reduce(#[configurable(derived)] reduce::ReduceConfig),

    /// Remap.
    Remap(#[configurable(derived)] remap::RemapConfig),

    /// Route.
    Route(#[configurable(derived)] route::RouteConfig),

    /// Sample.
    Sample(#[configurable(derived)] sample::SampleConfig),

    /// Tag cardinality limit.
    TagCardinalityLimit(#[configurable(derived)] tag_cardinality_limit::TagCardinalityLimitConfig),

    /// Throttle.
    Throttle(#[configurable(derived)] throttle::ThrottleConfig),
}

#[cfg(test)]
mod test {
    use vector_core::transform::FunctionTransform;

    use crate::{event::Event, transforms::OutputBuffer};

    /// Transform a single `Event` through the `FunctionTransform`
    ///
    /// # Panics
    ///
    /// If `ft` attempts to emit more than one `Event` on transform this
    /// function will panic.
    // We allow dead_code here to avoid unused warnings when we compile our
    // benchmarks as tests. It's a valid warning -- the benchmarks don't use
    // this function -- but flagging this function off for bench flags will
    // issue a unused warnings about the import above.
    #[allow(dead_code)]
    pub fn transform_one(ft: &mut dyn FunctionTransform, event: Event) -> Option<Event> {
        let mut buf = OutputBuffer::with_capacity(1);
        ft.transform(&mut buf, event);
        assert!(buf.len() <= 1);
        buf.into_events().next()
    }
}
