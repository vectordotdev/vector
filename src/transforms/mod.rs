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

use vector_common::config::ComponentKey;
use vector_config::{configurable_component, NamedComponent};
pub use vector_core::transform::{
    FunctionTransform, OutputBuffer, SyncTransform, TaskTransform, Transform, TransformOutputs,
    TransformOutputsBuf,
};
use vector_core::{
    config::{Input, Output},
    schema,
};

use crate::config::{InnerTopology, TransformConfig, TransformContext};

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

    /// Pipelines. (inner)
    #[cfg(feature = "transforms-pipelines")]
    #[configurable(metadata(skip_docs))]
    Pipeline(#[configurable(derived)] pipelines::PipelineConfig),

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
    Route(#[configurable(derived)] route::RouteConfig),

    /// Sample.
    #[cfg(feature = "transforms-sample")]
    Sample(#[configurable(derived)] sample::SampleConfig),

    /// Tag cardinality limit.
    #[cfg(feature = "transforms-tag_cardinality_limit")]
    TagCardinalityLimit(#[configurable(derived)] tag_cardinality_limit::TagCardinalityLimitConfig),

    /// Test (basic).
    #[cfg(test)]
    TestBasic(#[configurable(derived)] crate::test_util::mock::transforms::BasicTransformConfig),

    /// Test (noop).
    #[cfg(test)]
    TestNoop(#[configurable(derived)] crate::test_util::mock::transforms::NoopTransformConfig),

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
            Transforms::Aggregate(config) => config.build(globals).await,
            #[cfg(feature = "transforms-aws_ec2_metadata")]
            Transforms::AwsEc2Metadata(config) => config.build(globals).await,
            #[cfg(feature = "transforms-dedupe")]
            Transforms::Dedupe(config) => config.build(globals).await,
            #[cfg(feature = "transforms-filter")]
            Transforms::Filter(config) => config.build(globals).await,
            #[cfg(feature = "transforms-geoip")]
            Transforms::Geoip(config) => config.build(globals).await,
            #[cfg(feature = "transforms-log_to_metric")]
            Transforms::LogToMetric(config) => config.build(globals).await,
            #[cfg(feature = "transforms-lua")]
            Transforms::Lua(config) => config.build(globals).await,
            #[cfg(feature = "transforms-metric_to_log")]
            Transforms::MetricToLog(config) => config.build(globals).await,
            #[cfg(feature = "transforms-pipelines")]
            Transforms::Pipeline(config) => config.build(globals).await,
            #[cfg(feature = "transforms-pipelines")]
            Transforms::Pipelines(config) => config.build(globals).await,
            #[cfg(feature = "transforms-reduce")]
            Transforms::Reduce(config) => config.build(globals).await,
            #[cfg(feature = "transforms-remap")]
            Transforms::Remap(config) => config.build(globals).await,
            #[cfg(feature = "transforms-route")]
            Transforms::Route(config) => config.build(globals).await,
            #[cfg(feature = "transforms-sample")]
            Transforms::Sample(config) => config.build(globals).await,
            #[cfg(feature = "transforms-tag_cardinality_limit")]
            Transforms::TagCardinalityLimit(config) => config.build(globals).await,
            #[cfg(test)]
            Transforms::TestBasic(config) => config.build(globals).await,
            #[cfg(test)]
            Transforms::TestNoop(config) => config.build(globals).await,
            #[cfg(feature = "transforms-throttle")]
            Transforms::Throttle(config) => config.build(globals).await,
            #[allow(unreachable_patterns)]
            _ => unimplemented!(),
        }
    }

    fn input(&self) -> Input {
        match self {
            #[cfg(feature = "transforms-aggregate")]
            Transforms::Aggregate(config) => config.input(),
            #[cfg(feature = "transforms-aws_ec2_metadata")]
            Transforms::AwsEc2Metadata(config) => config.input(),
            #[cfg(feature = "transforms-dedupe")]
            Transforms::Dedupe(config) => config.input(),
            #[cfg(feature = "transforms-filter")]
            Transforms::Filter(config) => config.input(),
            #[cfg(feature = "transforms-geoip")]
            Transforms::Geoip(config) => config.input(),
            #[cfg(feature = "transforms-log_to_metric")]
            Transforms::LogToMetric(config) => config.input(),
            #[cfg(feature = "transforms-lua")]
            Transforms::Lua(config) => config.input(),
            #[cfg(feature = "transforms-metric_to_log")]
            Transforms::MetricToLog(config) => config.input(),
            #[cfg(feature = "transforms-pipelines")]
            Transforms::Pipeline(config) => config.input(),
            #[cfg(feature = "transforms-pipelines")]
            Transforms::Pipelines(config) => config.input(),
            #[cfg(feature = "transforms-reduce")]
            Transforms::Reduce(config) => config.input(),
            #[cfg(feature = "transforms-remap")]
            Transforms::Remap(config) => config.input(),
            #[cfg(feature = "transforms-route")]
            Transforms::Route(config) => config.input(),
            #[cfg(feature = "transforms-sample")]
            Transforms::Sample(config) => config.input(),
            #[cfg(feature = "transforms-tag_cardinality_limit")]
            Transforms::TagCardinalityLimit(config) => config.input(),
            #[cfg(test)]
            Transforms::TestBasic(config) => config.input(),
            #[cfg(test)]
            Transforms::TestNoop(config) => config.input(),
            #[cfg(feature = "transforms-throttle")]
            Transforms::Throttle(config) => config.input(),
            #[allow(unreachable_patterns)]
            _ => unimplemented!(),
        }
    }

    #[allow(unused_variables)]
    fn outputs(&self, merged_definition: &schema::Definition) -> Vec<Output> {
        match self {
            #[cfg(feature = "transforms-aggregate")]
            Transforms::Aggregate(config) => config.outputs(merged_definition),
            #[cfg(feature = "transforms-aws_ec2_metadata")]
            Transforms::AwsEc2Metadata(config) => config.outputs(merged_definition),
            #[cfg(feature = "transforms-dedupe")]
            Transforms::Dedupe(config) => config.outputs(merged_definition),
            #[cfg(feature = "transforms-filter")]
            Transforms::Filter(config) => config.outputs(merged_definition),
            #[cfg(feature = "transforms-geoip")]
            Transforms::Geoip(config) => config.outputs(merged_definition),
            #[cfg(feature = "transforms-log_to_metric")]
            Transforms::LogToMetric(config) => config.outputs(merged_definition),
            #[cfg(feature = "transforms-lua")]
            Transforms::Lua(config) => config.outputs(merged_definition),
            #[cfg(feature = "transforms-metric_to_log")]
            Transforms::MetricToLog(config) => config.outputs(merged_definition),
            #[cfg(feature = "transforms-pipelines")]
            Transforms::Pipeline(config) => config.outputs(merged_definition),
            #[cfg(feature = "transforms-pipelines")]
            Transforms::Pipelines(config) => config.outputs(merged_definition),
            #[cfg(feature = "transforms-reduce")]
            Transforms::Reduce(config) => config.outputs(merged_definition),
            #[cfg(feature = "transforms-remap")]
            Transforms::Remap(config) => config.outputs(merged_definition),
            #[cfg(feature = "transforms-route")]
            Transforms::Route(config) => config.outputs(merged_definition),
            #[cfg(feature = "transforms-sample")]
            Transforms::Sample(config) => config.outputs(merged_definition),
            #[cfg(feature = "transforms-tag_cardinality_limit")]
            Transforms::TagCardinalityLimit(config) => config.outputs(merged_definition),
            #[cfg(test)]
            Transforms::TestBasic(config) => config.outputs(merged_definition),
            #[cfg(test)]
            Transforms::TestNoop(config) => config.outputs(merged_definition),
            #[cfg(feature = "transforms-throttle")]
            Transforms::Throttle(config) => config.outputs(merged_definition),
            #[allow(unreachable_patterns)]
            _ => unimplemented!(),
        }
    }

    fn enable_concurrency(&self) -> bool {
        match self {
            #[cfg(feature = "transforms-aggregate")]
            Transforms::Aggregate(config) => config.enable_concurrency(),
            #[cfg(feature = "transforms-aws_ec2_metadata")]
            Transforms::AwsEc2Metadata(config) => config.enable_concurrency(),
            #[cfg(feature = "transforms-dedupe")]
            Transforms::Dedupe(config) => config.enable_concurrency(),
            #[cfg(feature = "transforms-filter")]
            Transforms::Filter(config) => config.enable_concurrency(),
            #[cfg(feature = "transforms-geoip")]
            Transforms::Geoip(config) => config.enable_concurrency(),
            #[cfg(feature = "transforms-log_to_metric")]
            Transforms::LogToMetric(config) => config.enable_concurrency(),
            #[cfg(feature = "transforms-lua")]
            Transforms::Lua(config) => config.enable_concurrency(),
            #[cfg(feature = "transforms-metric_to_log")]
            Transforms::MetricToLog(config) => config.enable_concurrency(),
            #[cfg(feature = "transforms-pipelines")]
            Transforms::Pipeline(config) => config.enable_concurrency(),
            #[cfg(feature = "transforms-pipelines")]
            Transforms::Pipelines(config) => config.enable_concurrency(),
            #[cfg(feature = "transforms-reduce")]
            Transforms::Reduce(config) => config.enable_concurrency(),
            #[cfg(feature = "transforms-remap")]
            Transforms::Remap(config) => config.enable_concurrency(),
            #[cfg(feature = "transforms-route")]
            Transforms::Route(config) => config.enable_concurrency(),
            #[cfg(feature = "transforms-sample")]
            Transforms::Sample(config) => config.enable_concurrency(),
            #[cfg(feature = "transforms-tag_cardinality_limit")]
            Transforms::TagCardinalityLimit(config) => config.enable_concurrency(),
            #[cfg(test)]
            Transforms::TestBasic(config) => config.enable_concurrency(),
            #[cfg(test)]
            Transforms::TestNoop(config) => config.enable_concurrency(),
            #[cfg(feature = "transforms-throttle")]
            Transforms::Throttle(config) => config.enable_concurrency(),
            #[allow(unreachable_patterns)]
            _ => unimplemented!(),
        }
    }

    #[allow(unused_variables)]
    fn nestable(&self, parents: &HashSet<&'static str>) -> bool {
        match self {
            #[cfg(feature = "transforms-aggregate")]
            Transforms::Aggregate(config) => config.nestable(parents),
            #[cfg(feature = "transforms-aws_ec2_metadata")]
            Transforms::AwsEc2Metadata(config) => config.nestable(parents),
            #[cfg(feature = "transforms-dedupe")]
            Transforms::Dedupe(config) => config.nestable(parents),
            #[cfg(feature = "transforms-filter")]
            Transforms::Filter(config) => config.nestable(parents),
            #[cfg(feature = "transforms-geoip")]
            Transforms::Geoip(config) => config.nestable(parents),
            #[cfg(feature = "transforms-log_to_metric")]
            Transforms::LogToMetric(config) => config.nestable(parents),
            #[cfg(feature = "transforms-lua")]
            Transforms::Lua(config) => config.nestable(parents),
            #[cfg(feature = "transforms-metric_to_log")]
            Transforms::MetricToLog(config) => config.nestable(parents),
            #[cfg(feature = "transforms-pipelines")]
            Transforms::Pipeline(config) => config.nestable(parents),
            #[cfg(feature = "transforms-pipelines")]
            Transforms::Pipelines(config) => config.nestable(parents),
            #[cfg(feature = "transforms-reduce")]
            Transforms::Reduce(config) => config.nestable(parents),
            #[cfg(feature = "transforms-remap")]
            Transforms::Remap(config) => config.nestable(parents),
            #[cfg(feature = "transforms-route")]
            Transforms::Route(config) => config.nestable(parents),
            #[cfg(feature = "transforms-sample")]
            Transforms::Sample(config) => config.nestable(parents),
            #[cfg(feature = "transforms-tag_cardinality_limit")]
            Transforms::TagCardinalityLimit(config) => config.nestable(parents),
            #[cfg(test)]
            Transforms::TestBasic(config) => config.nestable(parents),
            #[cfg(test)]
            Transforms::TestNoop(config) => config.nestable(parents),
            #[cfg(feature = "transforms-throttle")]
            Transforms::Throttle(config) => config.nestable(parents),
            #[allow(unreachable_patterns)]
            _ => unimplemented!(),
        }
    }

    #[allow(unused_variables)]
    fn expand(
        &mut self,
        name: &ComponentKey,
        inputs: &[String],
    ) -> crate::Result<Option<InnerTopology>> {
        match self {
            #[cfg(feature = "transforms-aggregate")]
            Transforms::Aggregate(config) => config.expand(name, inputs),
            #[cfg(feature = "transforms-aws_ec2_metadata")]
            Transforms::AwsEc2Metadata(config) => config.expand(name, inputs),
            #[cfg(feature = "transforms-dedupe")]
            Transforms::Dedupe(config) => config.expand(name, inputs),
            #[cfg(feature = "transforms-filter")]
            Transforms::Filter(config) => config.expand(name, inputs),
            #[cfg(feature = "transforms-geoip")]
            Transforms::Geoip(config) => config.expand(name, inputs),
            #[cfg(feature = "transforms-log_to_metric")]
            Transforms::LogToMetric(config) => config.expand(name, inputs),
            #[cfg(feature = "transforms-lua")]
            Transforms::Lua(config) => config.expand(name, inputs),
            #[cfg(feature = "transforms-metric_to_log")]
            Transforms::MetricToLog(config) => config.expand(name, inputs),
            #[cfg(feature = "transforms-pipelines")]
            Transforms::Pipeline(config) => config.expand(name, inputs),
            #[cfg(feature = "transforms-pipelines")]
            Transforms::Pipelines(config) => config.expand(name, inputs),
            #[cfg(feature = "transforms-reduce")]
            Transforms::Reduce(config) => config.expand(name, inputs),
            #[cfg(feature = "transforms-remap")]
            Transforms::Remap(config) => config.expand(name, inputs),
            #[cfg(feature = "transforms-route")]
            Transforms::Route(config) => config.expand(name, inputs),
            #[cfg(feature = "transforms-sample")]
            Transforms::Sample(config) => config.expand(name, inputs),
            #[cfg(feature = "transforms-tag_cardinality_limit")]
            Transforms::TagCardinalityLimit(config) => config.expand(name, inputs),
            #[cfg(test)]
            Transforms::TestBasic(config) => config.expand(name, inputs),
            #[cfg(test)]
            Transforms::TestNoop(config) => config.expand(name, inputs),
            #[cfg(feature = "transforms-throttle")]
            Transforms::Throttle(config) => config.expand(name, inputs),
            #[allow(unreachable_patterns)]
            _ => unimplemented!(),
        }
    }
}

impl NamedComponent for Transforms {
    const NAME: &'static str = "_invalid_usage";

    fn get_component_name(&self) -> &'static str {
        match self {
            #[cfg(feature = "transforms-aggregate")]
            Transforms::Aggregate(config) => config.get_component_name(),
            #[cfg(feature = "transforms-aws_ec2_metadata")]
            Transforms::AwsEc2Metadata(config) => config.get_component_name(),
            #[cfg(feature = "transforms-dedupe")]
            Transforms::Dedupe(config) => config.get_component_name(),
            #[cfg(feature = "transforms-filter")]
            Transforms::Filter(config) => config.get_component_name(),
            #[cfg(feature = "transforms-geoip")]
            Transforms::Geoip(config) => config.get_component_name(),
            #[cfg(feature = "transforms-log_to_metric")]
            Transforms::LogToMetric(config) => config.get_component_name(),
            #[cfg(feature = "transforms-lua")]
            Transforms::Lua(config) => config.get_component_name(),
            #[cfg(feature = "transforms-metric_to_log")]
            Transforms::MetricToLog(config) => config.get_component_name(),
            #[cfg(feature = "transforms-pipelines")]
            Transforms::Pipeline(config) => config.get_component_name(),
            #[cfg(feature = "transforms-pipelines")]
            Transforms::Pipelines(config) => config.get_component_name(),
            #[cfg(feature = "transforms-reduce")]
            Transforms::Reduce(config) => config.get_component_name(),
            #[cfg(feature = "transforms-remap")]
            Transforms::Remap(config) => config.get_component_name(),
            #[cfg(feature = "transforms-route")]
            Transforms::Route(config) => config.get_component_name(),
            #[cfg(feature = "transforms-sample")]
            Transforms::Sample(config) => config.get_component_name(),
            #[cfg(feature = "transforms-tag_cardinality_limit")]
            Transforms::TagCardinalityLimit(config) => config.get_component_name(),
            #[cfg(test)]
            Transforms::TestBasic(config) => config.get_component_name(),
            #[cfg(test)]
            Transforms::TestNoop(config) => config.get_component_name(),
            #[cfg(feature = "transforms-throttle")]
            Transforms::Throttle(config) => config.get_component_name(),
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
