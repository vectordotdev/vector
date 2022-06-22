use async_trait::async_trait;
use snafu::Snafu;

#[cfg(feature = "transforms-aggregate")]
pub mod aggregate;
#[cfg(feature = "transforms-aws_ec2_metadata")]
pub mod aws_ec2_metadata;
#[cfg(feature = "transforms-dedupe")]
pub mod dedupe;
#[cfg(feature = "transforms-filter")]
pub mod filter;
#[cfg(feature = "transforms-geoip")]
pub mod geoip;
#[cfg(feature = "transforms-log_to_metric")]
pub mod log_to_metric;
#[cfg(feature = "transforms-lua")]
pub mod lua;
#[cfg(feature = "transforms-metric_to_log")]
pub mod metric_to_log;
#[cfg(feature = "transforms-pipelines")]
pub mod pipelines;
#[cfg(feature = "transforms-reduce")]
pub mod reduce;
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
use vector_core::{
    config::{Input, Output},
    schema,
    transform::{TransformConfig, TransformContext},
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

    /// AWS EC2 metadata.
    AwsEc2Metadata(#[configurable(derived)] aws_ec2_metadata::Ec2Metadata),

    /// Dedupe.
    Dedupe(#[configurable(derived)] dedupe::DedupeConfig),

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
    #[serde(alias = "swimlanes")]
    Route(#[configurable(derived)] route::RouteConfig),

    /// Sample.
    #[serde(alias = "sampler")]
    Sample(#[configurable(derived)] sample::SampleConfig),

    /// Tag cardinality limit.
    TagCardinalityLimit(#[configurable(derived)] tag_cardinality_limit::TagCardinalityLimitConfig),

    /// Throttle.
    Throttle(#[configurable(derived)] throttle::ThrottleConfig),
}

#[async_trait]
impl TransformConfig for Transforms {
    async fn build(&self, globals: &TransformContext) -> crate::Result<Transform> {
        match self {
            Transforms::Aggregate(inner) => inner.build(globals).await,
            Transforms::AwsEc2Metadata(inner) => inner.build(globals).await,
            Transforms::Dedupe(inner) => inner.build(globals).await,
            Transforms::Filter(inner) => inner.build(globals).await,
            Transforms::Geoip(inner) => inner.build(globals).await,
            Transforms::LogToMetric(inner) => inner.build(globals).await,
            Transforms::Lua(inner) => inner.build(globals).await,
            Transforms::MetricToLog(inner) => inner.build(globals).await,
            Transforms::Pipelines(inner) => inner.build(globals).await,
            Transforms::Reduce(inner) => inner.build(globals).await,
            Transforms::Remap(inner) => inner.build(globals).await,
            Transforms::Route(inner) => inner.build(globals).await,
            Transforms::Sample(inner) => inner.build(globals).await,
            Transforms::TagCardinalityLimit(inner) => inner.build(globals).await,
            Transforms::Throttle(inner) => inner.build(globals).await,
        }
    }

    fn input(&self) -> Input {
        match self {
            Transforms::Aggregate(inner) => inner.input(),
            Transforms::AwsEc2Metadata(inner) => inner.input(),
            Transforms::Dedupe(inner) => inner.input(),
            Transforms::Filter(inner) => inner.input(),
            Transforms::Geoip(inner) => inner.input(),
            Transforms::LogToMetric(inner) => inner.input(),
            Transforms::Lua(inner) => inner.input(),
            Transforms::MetricToLog(inner) => inner.input(),
            Transforms::Pipelines(inner) => inner.input(),
            Transforms::Reduce(inner) => inner.input(),
            Transforms::Remap(inner) => inner.input(),
            Transforms::Route(inner) => inner.input(),
            Transforms::Sample(inner) => inner.input(),
            Transforms::TagCardinalityLimit(inner) => inner.input(),
            Transforms::Throttle(inner) => inner.input(),
        }
    }

    fn outputs(&self, merged_definition: &schema::Definition) -> Vec<Output> {
        match self {
            Transforms::Aggregate(inner) => inner.outputs(merged_definition),
            Transforms::AwsEc2Metadata(inner) => inner.outputs(merged_definition),
            Transforms::Dedupe(inner) => inner.outputs(merged_definition),
            Transforms::Filter(inner) => inner.outputs(merged_definition),
            Transforms::Geoip(inner) => inner.outputs(merged_definition),
            Transforms::LogToMetric(inner) => inner.outputs(merged_definition),
            Transforms::Lua(inner) => inner.outputs(merged_definition),
            Transforms::MetricToLog(inner) => inner.outputs(merged_definition),
            Transforms::Pipelines(inner) => inner.outputs(merged_definition),
            Transforms::Reduce(inner) => inner.outputs(merged_definition),
            Transforms::Remap(inner) => inner.outputs(merged_definition),
            Transforms::Route(inner) => inner.outputs(merged_definition),
            Transforms::Sample(inner) => inner.outputs(merged_definition),
            Transforms::TagCardinalityLimit(inner) => inner.outputs(merged_definition),
            Transforms::Throttle(inner) => inner.outputs(merged_definition),
        }
    }

    fn transform_type(&self) -> &'static str {
        match self {
            Transforms::Aggregate(inner) => inner.transform_type(),
            Transforms::AwsEc2Metadata(inner) => inner.transform_type(),
            Transforms::Dedupe(inner) => inner.transform_type(),
            Transforms::Filter(inner) => inner.transform_type(),
            Transforms::Geoip(inner) => inner.transform_type(),
            Transforms::LogToMetric(inner) => inner.transform_type(),
            Transforms::Lua(inner) => inner.transform_type(),
            Transforms::MetricToLog(inner) => inner.transform_type(),
            Transforms::Pipelines(inner) => inner.transform_type(),
            Transforms::Reduce(inner) => inner.transform_type(),
            Transforms::Remap(inner) => inner.transform_type(),
            Transforms::Route(inner) => inner.transform_type(),
            Transforms::Sample(inner) => inner.transform_type(),
            Transforms::TagCardinalityLimit(inner) => inner.transform_type(),
            Transforms::Throttle(inner) => inner.transform_type(),
        }
    }

    fn typetag_name(&self) -> &'static str {
        match self {
            Transforms::Aggregate(inner) => inner.typetag_name(),
            Transforms::AwsEc2Metadata(inner) => inner.typetag_name(),
            Transforms::Dedupe(inner) => inner.typetag_name(),
            Transforms::Filter(inner) => inner.typetag_name(),
            Transforms::Geoip(inner) => inner.typetag_name(),
            Transforms::LogToMetric(inner) => inner.typetag_name(),
            Transforms::Lua(inner) => inner.typetag_name(),
            Transforms::MetricToLog(inner) => inner.typetag_name(),
            Transforms::Pipelines(inner) => inner.typetag_name(),
            Transforms::Reduce(inner) => inner.typetag_name(),
            Transforms::Remap(inner) => inner.typetag_name(),
            Transforms::Route(inner) => inner.typetag_name(),
            Transforms::Sample(inner) => inner.typetag_name(),
            Transforms::TagCardinalityLimit(inner) => inner.typetag_name(),
            Transforms::Throttle(inner) => inner.typetag_name(),
        }
    }

    fn typetag_deserialize(&self) {
        match self {
            Transforms::Aggregate(inner) => inner.typetag_deserialize(),
            Transforms::AwsEc2Metadata(inner) => inner.typetag_deserialize(),
            Transforms::Dedupe(inner) => inner.typetag_deserialize(),
            Transforms::Filter(inner) => inner.typetag_deserialize(),
            Transforms::Geoip(inner) => inner.typetag_deserialize(),
            Transforms::LogToMetric(inner) => inner.typetag_deserialize(),
            Transforms::Lua(inner) => inner.typetag_deserialize(),
            Transforms::MetricToLog(inner) => inner.typetag_deserialize(),
            Transforms::Pipelines(inner) => inner.typetag_deserialize(),
            Transforms::Reduce(inner) => inner.typetag_deserialize(),
            Transforms::Remap(inner) => inner.typetag_deserialize(),
            Transforms::Route(inner) => inner.typetag_deserialize(),
            Transforms::Sample(inner) => inner.typetag_deserialize(),
            Transforms::TagCardinalityLimit(inner) => inner.typetag_deserialize(),
            Transforms::Throttle(inner) => inner.typetag_deserialize(),
        }
    }
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
