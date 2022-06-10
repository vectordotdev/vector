use snafu::Snafu;

#[cfg(feature = "transforms-add_fields")]
pub mod add_fields;
#[cfg(feature = "transforms-add_tags")]
pub mod add_tags;
#[cfg(feature = "transforms-aggregate")]
pub mod aggregate;
#[cfg(feature = "transforms-ansi_stripper")]
pub mod ansi_stripper;
#[cfg(feature = "transforms-aws_cloudwatch_logs_subscription_parser")]
pub mod aws_cloudwatch_logs_subscription_parser;
#[cfg(feature = "transforms-aws_ec2_metadata")]
pub mod aws_ec2_metadata;
#[cfg(feature = "transforms-coercer")]
pub mod coercer;
#[cfg(feature = "transforms-concat")]
pub mod concat;
#[cfg(feature = "transforms-dedupe")]
pub mod dedupe;
#[cfg(feature = "transforms-field_filter")]
pub mod field_filter;
#[cfg(feature = "transforms-filter")]
pub mod filter;
#[cfg(feature = "transforms-geoip")]
pub mod geoip;
#[cfg(feature = "transforms-grok_parser")]
pub mod grok_parser;
#[cfg(feature = "transforms-json_parser")]
pub mod json_parser;
#[cfg(feature = "transforms-key_value_parser")]
pub mod key_value_parser;
#[cfg(feature = "transforms-log_to_metric")]
pub mod log_to_metric;
#[cfg(feature = "transforms-logfmt_parser")]
pub mod logfmt_parser;
#[cfg(feature = "transforms-lua")]
pub mod lua;
#[cfg(feature = "transforms-merge")]
pub mod merge;
#[cfg(feature = "transforms-metric_to_log")]
pub mod metric_to_log;
#[cfg(feature = "transforms-pipelines")]
pub mod pipelines;
#[cfg(feature = "transforms-reduce")]
pub mod reduce;
#[cfg(feature = "transforms-regex_parser")]
pub mod regex_parser;
#[cfg(feature = "transforms-remap")]
pub mod remap;
#[cfg(feature = "transforms-remove_fields")]
pub mod remove_fields;
#[cfg(feature = "transforms-remove_tags")]
pub mod remove_tags;
#[cfg(feature = "transforms-rename_fields")]
pub mod rename_fields;
#[cfg(feature = "transforms-route")]
pub mod route;
#[cfg(feature = "transforms-sample")]
pub mod sample;
#[cfg(feature = "transforms-split")]
pub mod split;
#[cfg(feature = "transforms-tag_cardinality_limit")]
pub mod tag_cardinality_limit;
#[cfg(feature = "transforms-throttle")]
pub mod throttle;
#[cfg(feature = "transforms-tokenizer")]
pub mod tokenizer;

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
    // Deprecated transforms that _also_ have types with cyclical dependencies, and would require more changes to the
    // schema generation logic to break cycles. We may want to actually support them, but they're commented out for now
    // to avoid making the compiler angry.
    // AddFields(#[configurable(derived)] add_fields::AddFieldsConfig),
    // RenameFields(#[configurable(derived)] rename_fields::RenameFieldsConfig),
    /// Aggregate.
    Aggregate(#[configurable(derived)] aggregate::AggregateConfig),

    /// ANSI stripper.
    AnsiStripper(#[configurable(derived)] ansi_stripper::AnsiStripperConfig),

    /// AWS Cloudwatch Logs subscription parser.
    AwsCloudwatchLogsSubscriptionParser(
        #[configurable(derived)]
        aws_cloudwatch_logs_subscription_parser::AwsCloudwatchLogsSubscriptionParserConfig,
    ),

    /// AWS EC2 metadata.
    AwsEc2Metadata(#[configurable(derived)] aws_ec2_metadata::Ec2Metadata),

    /// Coercer.
    Coercer(#[configurable(derived)] coercer::CoercerConfig),

    /// Concat.
    Concat(#[configurable(derived)] concat::ConcatConfig),

    /// Dedupe.
    Dedupe(#[configurable(derived)] dedupe::DedupeConfig),

    /// Field filter.
    FieldFilter(#[configurable(derived)] field_filter::FieldFilterConfig),

    /// Filter.
    Filter(#[configurable(derived)] filter::FilterConfig),

    /// GeoIP.
    Geoip(#[configurable(derived)] geoip::GeoipConfig),

    /// Grok parser.
    GrokParser(#[configurable(derived)] grok_parser::GrokParserConfig),

    /// JSON parser.
    JsonParser(#[configurable(derived)] json_parser::JsonParserConfig),

    /// Key value parser.
    KeyValueParser(#[configurable(derived)] key_value_parser::KeyValueConfig),

    /// Log to metric.
    LogToMetric(#[configurable(derived)] log_to_metric::LogToMetricConfig),

    /// Logfmt parser.
    LogfmtParser(#[configurable(derived)] logfmt_parser::LogfmtConfig),

    /// Lua.
    Lua(#[configurable(derived)] lua::LuaConfig),

    /// Merge.
    Merge(#[configurable(derived)] merge::MergeConfig),

    /// Metric to log.
    MetricToLog(#[configurable(derived)] metric_to_log::MetricToLogConfig),

    /// Pipelines.
    Pipelines(#[configurable(derived)] pipelines::PipelinesConfig),

    /// Reduce.
    Reduce(#[configurable(derived)] reduce::ReduceConfig),

    /// Regex parser.
    RegexParser(#[configurable(derived)] regex_parser::RegexParserConfig),

    /// Remap.
    Remap(#[configurable(derived)] remap::RemapConfig),

    /// Remove fields.
    RemoveFields(#[configurable(derived)] remove_fields::RemoveFieldsConfig),

    /// Remove tags.
    RemoveTags(#[configurable(derived)] remove_tags::RemoveTagsConfig),

    /// Route.
    Route(#[configurable(derived)] route::RouteConfig),

    /// Sample.
    Sample(#[configurable(derived)] sample::SampleConfig),

    /// Split.
    Split(#[configurable(derived)] split::SplitConfig),

    /// Tag cardinality limit.
    TagCardinalityLimit(#[configurable(derived)] tag_cardinality_limit::TagCardinalityLimitConfig),

    /// Throttle.
    Throttle(#[configurable(derived)] throttle::ThrottleConfig),

    /// Tokenizer.
    Tokenizer(#[configurable(derived)] tokenizer::TokenizerConfig),
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
