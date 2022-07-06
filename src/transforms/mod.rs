#[allow(unused_imports)]
use std::collections::HashSet;

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
    #[cfg(feature = "transforms-aggregate")]
    Aggregate(#[configurable(derived)] aggregate::AggregateConfig),

    /// AWS EC2 metadata.
    #[cfg(feature = "transforms-aws_ec2_metadata")]
    AwsEc2Metadata(#[configurable(derived)] aws_ec2_metadata::Ec2Metadata),

    /// Dedupe.
    #[cfg(feature = "transforms-dedupe")]
    Dedupe(#[configurable(derived)] dedupe::DedupeConfig),

    /// Filter.
    #[cfg(feature = "transforms-filter")]
    Filter(#[configurable(derived)] filter::FilterConfig),

    /// GeoIP.
    #[cfg(feature = "transforms-geoip")]
    Geoip(#[configurable(derived)] geoip::GeoipConfig),

    /// Log to metric.
    #[cfg(feature = "transforms-log_to_metric")]
    LogToMetric(#[configurable(derived)] log_to_metric::LogToMetricConfig),

    /// Lua.
    #[cfg(feature = "transforms-lua")]
    Lua(#[configurable(derived)] lua::LuaConfig),

    /// Metric to log.
    #[cfg(feature = "transforms-metric_to_log")]
    MetricToLog(#[configurable(derived)] metric_to_log::MetricToLogConfig),

    /// Pipelines.
    #[cfg(feature = "transforms-pipelines")]
    Pipelines(#[configurable(derived)] pipelines::PipelinesConfig),

    /// Reduce.
    #[cfg(feature = "transforms-reduce")]
    Reduce(#[configurable(derived)] reduce::ReduceConfig),

    /// Remap.
    #[cfg(feature = "transforms-remap")]
    Remap(#[configurable(derived)] remap::RemapConfig),

    /// Route.
    #[cfg(feature = "transforms-route")]
    #[serde(alias = "swimlanes")]
    Route(#[configurable(derived)] route::RouteConfig),

    /// Sample.
    #[cfg(feature = "transforms-sample")]
    #[serde(alias = "sampler")]
    Sample(#[configurable(derived)] sample::SampleConfig),

    /// Tag cardinality limit.
    #[cfg(feature = "transforms-tag_cardinality_limit")]
    TagCardinalityLimit(#[configurable(derived)] tag_cardinality_limit::TagCardinalityLimitConfig),

    /// Throttle.
    #[cfg(feature = "transforms-throttle")]
    Throttle(#[configurable(derived)] throttle::ThrottleConfig),
}

#[async_trait]
impl TransformConfig for Transforms {
    #[allow(unused_variables)]
    async fn build(&self, globals: &TransformContext) -> crate::Result<Transform> {
        match self {
            #[cfg(feature = "transforms-aggregate")]
            Transforms::Aggregate(inner) => inner.build(globals).await,
            #[cfg(feature = "transforms-aws_ec2_metadata")]
            Transforms::AwsEc2Metadata(inner) => inner.build(globals).await,
            #[cfg(feature = "transforms-dedupe")]
            Transforms::Dedupe(inner) => inner.build(globals).await,
            #[cfg(feature = "transforms-filter")]
            Transforms::Filter(inner) => inner.build(globals).await,
            #[cfg(feature = "transforms-geoip")]
            Transforms::Geoip(inner) => inner.build(globals).await,
            #[cfg(feature = "transforms-log_to_metric")]
            Transforms::LogToMetric(inner) => inner.build(globals).await,
            #[cfg(feature = "transforms-lua")]
            Transforms::Lua(inner) => inner.build(globals).await,
            #[cfg(feature = "transforms-metric_to_log")]
            Transforms::MetricToLog(inner) => inner.build(globals).await,
            #[cfg(feature = "transforms-pipelines")]
            Transforms::Pipelines(inner) => inner.build(globals).await,
            #[cfg(feature = "transforms-reduce")]
            Transforms::Reduce(inner) => inner.build(globals).await,
            #[cfg(feature = "transforms-remap")]
            Transforms::Remap(inner) => inner.build(globals).await,
            #[cfg(feature = "transforms-route")]
            Transforms::Route(inner) => inner.build(globals).await,
            #[cfg(feature = "transforms-sample")]
            Transforms::Sample(inner) => inner.build(globals).await,
            #[cfg(feature = "transforms-tag_cardinality_limit")]
            Transforms::TagCardinalityLimit(inner) => inner.build(globals).await,
            #[cfg(feature = "transforms-throttle")]
            Transforms::Throttle(inner) => inner.build(globals).await,
            #[allow(unreachable_patterns)]
            _ => unimplemented!(),
        }
    }

    fn input(&self) -> Input {
        match self {
            #[cfg(feature = "transforms-aggregate")]
            Transforms::Aggregate(inner) => inner.input(),
            #[cfg(feature = "transforms-aws_ec2_metadata")]
            Transforms::AwsEc2Metadata(inner) => inner.input(),
            #[cfg(feature = "transforms-dedupe")]
            Transforms::Dedupe(inner) => inner.input(),
            #[cfg(feature = "transforms-filter")]
            Transforms::Filter(inner) => inner.input(),
            #[cfg(feature = "transforms-geoip")]
            Transforms::Geoip(inner) => inner.input(),
            #[cfg(feature = "transforms-log_to_metric")]
            Transforms::LogToMetric(inner) => inner.input(),
            #[cfg(feature = "transforms-lua")]
            Transforms::Lua(inner) => inner.input(),
            #[cfg(feature = "transforms-metric_to_log")]
            Transforms::MetricToLog(inner) => inner.input(),
            #[cfg(feature = "transforms-pipelines")]
            Transforms::Pipelines(inner) => inner.input(),
            #[cfg(feature = "transforms-reduce")]
            Transforms::Reduce(inner) => inner.input(),
            #[cfg(feature = "transforms-remap")]
            Transforms::Remap(inner) => inner.input(),
            #[cfg(feature = "transforms-route")]
            Transforms::Route(inner) => inner.input(),
            #[cfg(feature = "transforms-sample")]
            Transforms::Sample(inner) => inner.input(),
            #[cfg(feature = "transforms-tag_cardinality_limit")]
            Transforms::TagCardinalityLimit(inner) => inner.input(),
            #[cfg(feature = "transforms-throttle")]
            Transforms::Throttle(inner) => inner.input(),
            #[allow(unreachable_patterns)]
            _ => unimplemented!(),
        }
    }

    #[allow(unused_variables)]
    fn outputs(&self, merged_definition: &schema::Definition) -> Vec<Output> {
        match self {
            #[cfg(feature = "transforms-aggregate")]
            Transforms::Aggregate(inner) => inner.outputs(merged_definition),
            #[cfg(feature = "transforms-aws_ec2_metadata")]
            Transforms::AwsEc2Metadata(inner) => inner.outputs(merged_definition),
            #[cfg(feature = "transforms-dedupe")]
            Transforms::Dedupe(inner) => inner.outputs(merged_definition),
            #[cfg(feature = "transforms-filter")]
            Transforms::Filter(inner) => inner.outputs(merged_definition),
            #[cfg(feature = "transforms-geoip")]
            Transforms::Geoip(inner) => inner.outputs(merged_definition),
            #[cfg(feature = "transforms-log_to_metric")]
            Transforms::LogToMetric(inner) => inner.outputs(merged_definition),
            #[cfg(feature = "transforms-lua")]
            Transforms::Lua(inner) => inner.outputs(merged_definition),
            #[cfg(feature = "transforms-metric_to_log")]
            Transforms::MetricToLog(inner) => inner.outputs(merged_definition),
            #[cfg(feature = "transforms-pipelines")]
            Transforms::Pipelines(inner) => inner.outputs(merged_definition),
            #[cfg(feature = "transforms-reduce")]
            Transforms::Reduce(inner) => inner.outputs(merged_definition),
            #[cfg(feature = "transforms-remap")]
            Transforms::Remap(inner) => inner.outputs(merged_definition),
            #[cfg(feature = "transforms-route")]
            Transforms::Route(inner) => inner.outputs(merged_definition),
            #[cfg(feature = "transforms-sample")]
            Transforms::Sample(inner) => inner.outputs(merged_definition),
            #[cfg(feature = "transforms-tag_cardinality_limit")]
            Transforms::TagCardinalityLimit(inner) => inner.outputs(merged_definition),
            #[cfg(feature = "transforms-throttle")]
            Transforms::Throttle(inner) => inner.outputs(merged_definition),
            #[allow(unreachable_patterns)]
            _ => unimplemented!(),
        }
    }

    fn transform_type(&self) -> &'static str {
        match self {
            #[cfg(feature = "transforms-aggregate")]
            Transforms::Aggregate(inner) => inner.transform_type(),
            #[cfg(feature = "transforms-aws_ec2_metadata")]
            Transforms::AwsEc2Metadata(inner) => inner.transform_type(),
            #[cfg(feature = "transforms-dedupe")]
            Transforms::Dedupe(inner) => inner.transform_type(),
            #[cfg(feature = "transforms-filter")]
            Transforms::Filter(inner) => inner.transform_type(),
            #[cfg(feature = "transforms-geoip")]
            Transforms::Geoip(inner) => inner.transform_type(),
            #[cfg(feature = "transforms-log_to_metric")]
            Transforms::LogToMetric(inner) => inner.transform_type(),
            #[cfg(feature = "transforms-lua")]
            Transforms::Lua(inner) => inner.transform_type(),
            #[cfg(feature = "transforms-metric_to_log")]
            Transforms::MetricToLog(inner) => inner.transform_type(),
            #[cfg(feature = "transforms-pipelines")]
            Transforms::Pipelines(inner) => inner.transform_type(),
            #[cfg(feature = "transforms-reduce")]
            Transforms::Reduce(inner) => inner.transform_type(),
            #[cfg(feature = "transforms-remap")]
            Transforms::Remap(inner) => inner.transform_type(),
            #[cfg(feature = "transforms-route")]
            Transforms::Route(inner) => inner.transform_type(),
            #[cfg(feature = "transforms-sample")]
            Transforms::Sample(inner) => inner.transform_type(),
            #[cfg(feature = "transforms-tag_cardinality_limit")]
            Transforms::TagCardinalityLimit(inner) => inner.transform_type(),
            #[cfg(feature = "transforms-throttle")]
            Transforms::Throttle(inner) => inner.transform_type(),
            #[allow(unreachable_patterns)]
            _ => unimplemented!(),
        }
    }

    fn typetag_name(&self) -> &'static str {
        match self {
            #[cfg(feature = "transforms-aggregate")]
            Transforms::Aggregate(inner) => inner.typetag_name(),
            #[cfg(feature = "transforms-aws_ec2_metadata")]
            Transforms::AwsEc2Metadata(inner) => inner.typetag_name(),
            #[cfg(feature = "transforms-dedupe")]
            Transforms::Dedupe(inner) => inner.typetag_name(),
            #[cfg(feature = "transforms-filter")]
            Transforms::Filter(inner) => inner.typetag_name(),
            #[cfg(feature = "transforms-geoip")]
            Transforms::Geoip(inner) => inner.typetag_name(),
            #[cfg(feature = "transforms-log_to_metric")]
            Transforms::LogToMetric(inner) => inner.typetag_name(),
            #[cfg(feature = "transforms-lua")]
            Transforms::Lua(inner) => inner.typetag_name(),
            #[cfg(feature = "transforms-metric_to_log")]
            Transforms::MetricToLog(inner) => inner.typetag_name(),
            #[cfg(feature = "transforms-pipelines")]
            Transforms::Pipelines(inner) => inner.typetag_name(),
            #[cfg(feature = "transforms-reduce")]
            Transforms::Reduce(inner) => inner.typetag_name(),
            #[cfg(feature = "transforms-remap")]
            Transforms::Remap(inner) => inner.typetag_name(),
            #[cfg(feature = "transforms-route")]
            Transforms::Route(inner) => inner.typetag_name(),
            #[cfg(feature = "transforms-sample")]
            Transforms::Sample(inner) => inner.typetag_name(),
            #[cfg(feature = "transforms-tag_cardinality_limit")]
            Transforms::TagCardinalityLimit(inner) => inner.typetag_name(),
            #[cfg(feature = "transforms-throttle")]
            Transforms::Throttle(inner) => inner.typetag_name(),
            #[allow(unreachable_patterns)]
            _ => unimplemented!(),
        }
    }

    fn typetag_deserialize(&self) {
        match self {
            #[cfg(feature = "transforms-aggregate")]
            Transforms::Aggregate(inner) => inner.typetag_deserialize(),
            #[cfg(feature = "transforms-aws_ec2_metadata")]
            Transforms::AwsEc2Metadata(inner) => inner.typetag_deserialize(),
            #[cfg(feature = "transforms-dedupe")]
            Transforms::Dedupe(inner) => inner.typetag_deserialize(),
            #[cfg(feature = "transforms-filter")]
            Transforms::Filter(inner) => inner.typetag_deserialize(),
            #[cfg(feature = "transforms-geoip")]
            Transforms::Geoip(inner) => inner.typetag_deserialize(),
            #[cfg(feature = "transforms-log_to_metric")]
            Transforms::LogToMetric(inner) => inner.typetag_deserialize(),
            #[cfg(feature = "transforms-lua")]
            Transforms::Lua(inner) => inner.typetag_deserialize(),
            #[cfg(feature = "transforms-metric_to_log")]
            Transforms::MetricToLog(inner) => inner.typetag_deserialize(),
            #[cfg(feature = "transforms-pipelines")]
            Transforms::Pipelines(inner) => inner.typetag_deserialize(),
            #[cfg(feature = "transforms-reduce")]
            Transforms::Reduce(inner) => inner.typetag_deserialize(),
            #[cfg(feature = "transforms-remap")]
            Transforms::Remap(inner) => inner.typetag_deserialize(),
            #[cfg(feature = "transforms-route")]
            Transforms::Route(inner) => inner.typetag_deserialize(),
            #[cfg(feature = "transforms-sample")]
            Transforms::Sample(inner) => inner.typetag_deserialize(),
            #[cfg(feature = "transforms-tag_cardinality_limit")]
            Transforms::TagCardinalityLimit(inner) => inner.typetag_deserialize(),
            #[cfg(feature = "transforms-throttle")]
            Transforms::Throttle(inner) => inner.typetag_deserialize(),
            #[allow(unreachable_patterns)]
            _ => unimplemented!(),
        }
    }

    #[allow(unused_variables)]
    fn nestable(&self, parents: &HashSet<&'static str>) -> bool {
        match self {
            #[cfg(feature = "transforms-aggregate")]
            Transforms::Aggregate(inner) => inner.nestable(parents),
            #[cfg(feature = "transforms-aws_ec2_metadata")]
            Transforms::AwsEc2Metadata(inner) => inner.nestable(parents),
            #[cfg(feature = "transforms-dedupe")]
            Transforms::Dedupe(inner) => inner.nestable(parents),
            #[cfg(feature = "transforms-filter")]
            Transforms::Filter(inner) => inner.nestable(parents),
            #[cfg(feature = "transforms-geoip")]
            Transforms::Geoip(inner) => inner.nestable(parents),
            #[cfg(feature = "transforms-log_to_metric")]
            Transforms::LogToMetric(inner) => inner.nestable(parents),
            #[cfg(feature = "transforms-lua")]
            Transforms::Lua(inner) => inner.nestable(parents),
            #[cfg(feature = "transforms-metric_to_log")]
            Transforms::MetricToLog(inner) => inner.nestable(parents),
            #[cfg(feature = "transforms-pipelines")]
            Transforms::Pipelines(inner) => inner.nestable(parents),
            #[cfg(feature = "transforms-reduce")]
            Transforms::Reduce(inner) => inner.nestable(parents),
            #[cfg(feature = "transforms-remap")]
            Transforms::Remap(inner) => inner.nestable(parents),
            #[cfg(feature = "transforms-route")]
            Transforms::Route(inner) => inner.nestable(parents),
            #[cfg(feature = "transforms-sample")]
            Transforms::Sample(inner) => inner.nestable(parents),
            #[cfg(feature = "transforms-tag_cardinality_limit")]
            Transforms::TagCardinalityLimit(inner) => inner.nestable(parents),
            #[cfg(feature = "transforms-throttle")]
            Transforms::Throttle(inner) => inner.nestable(parents),
            #[allow(unreachable_patterns)]
            _ => unimplemented!(),
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
