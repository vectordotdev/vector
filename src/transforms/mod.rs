#[allow(unused_imports)]
use std::collections::HashSet;

use enum_dispatch::enum_dispatch;
use snafu::Snafu;

#[cfg(feature = "transforms-aggregate")]
pub mod aggregate;
#[cfg(feature = "transforms-aws_ec2_metadata")]
pub mod aws_ec2_metadata;
#[cfg(feature = "transforms-dedupe")]
pub mod dedupe;
#[cfg(feature = "transforms-filter")]
pub mod filter;
pub mod log_to_metric;
#[cfg(feature = "transforms-lua")]
pub mod lua;
#[cfg(feature = "transforms-metric_to_log")]
pub mod metric_to_log;
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

use vector_config::{configurable_component, NamedComponent};
pub use vector_core::transform::{
    FunctionTransform, OutputBuffer, SyncTransform, TaskTransform, Transform, TransformOutputs,
    TransformOutputsBuf,
};
use vector_core::{
    config::{Input, LogNamespace, Output},
    schema,
};

use crate::config::{TransformConfig, TransformContext};

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
#[enum_dispatch(TransformConfig)]
pub enum Transforms {
    /// Aggregate metrics passing through a topology.
    #[cfg(feature = "transforms-aggregate")]
    Aggregate(aggregate::AggregateConfig),

    /// Parse metadata emitted by AWS EC2 instances.
    #[cfg(feature = "transforms-aws_ec2_metadata")]
    AwsEc2Metadata(aws_ec2_metadata::Ec2Metadata),

    /// Deduplicate logs passing through a topology.
    #[cfg(feature = "transforms-dedupe")]
    Dedupe(dedupe::DedupeConfig),

    /// Filter events based on a set of conditions.
    #[cfg(feature = "transforms-filter")]
    Filter(filter::FilterConfig),

    /// Convert log events to metric events.
    LogToMetric(log_to_metric::LogToMetricConfig),

    /// Modify event data using the Lua programming language.
    #[cfg(feature = "transforms-lua")]
    Lua(lua::LuaConfig),

    /// Convert metric events to log events.
    #[cfg(feature = "transforms-metric_to_log")]
    MetricToLog(metric_to_log::MetricToLogConfig),

    /// Collapse multiple log events into a single event based on a set of conditions and merge strategies.
    #[cfg(feature = "transforms-reduce")]
    Reduce(reduce::ReduceConfig),

    /// Modify your observability data as it passes through your topology using Vector Remap Language (VRL).
    #[cfg(feature = "transforms-remap")]
    Remap(remap::RemapConfig),

    /// Split a stream of events into multiple sub-streams based on user-supplied conditions.
    #[cfg(feature = "transforms-route")]
    Route(route::RouteConfig),

    /// Sample events from an event stream based on supplied criteria and at a configurable rate.
    #[cfg(feature = "transforms-sample")]
    Sample(sample::SampleConfig),

    /// Limit the cardinality of tags on metrics events as a safeguard against cardinality explosion.
    #[cfg(feature = "transforms-tag_cardinality_limit")]
    TagCardinalityLimit(tag_cardinality_limit::TagCardinalityLimitConfig),

    /// Test (basic).
    #[cfg(test)]
    TestBasic(crate::test_util::mock::transforms::BasicTransformConfig),

    /// Test (noop).
    #[cfg(test)]
    TestNoop(crate::test_util::mock::transforms::NoopTransformConfig),

    /// Rate limit logs passing through a topology.
    #[cfg(feature = "transforms-throttle")]
    Throttle(throttle::ThrottleConfig),
}

// We can't use `enum_dispatch` here because it doesn't support associated constants.
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
            Transforms::LogToMetric(config) => config.get_component_name(),
            #[cfg(feature = "transforms-lua")]
            Transforms::Lua(config) => config.get_component_name(),
            #[cfg(feature = "transforms-metric_to_log")]
            Transforms::MetricToLog(config) => config.get_component_name(),
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
    use futures::Stream;
    use futures_util::SinkExt;
    use tokio::sync::mpsc;
    use tokio_util::sync::PollSender;
    use vector_core::transform::FunctionTransform;

    use super::Transforms;
    use crate::{
        config::{
            unit_test::{UnitTestStreamSinkConfig, UnitTestStreamSourceConfig},
            ConfigBuilder,
        },
        event::Event,
        test_util::start_topology,
        topology::RunningTopology,
        transforms::OutputBuffer,
    };

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

    #[allow(dead_code)]
    pub async fn create_topology<T: Into<Transforms>>(
        events: impl Stream<Item = Event> + Send + 'static,
        transform_config: T,
    ) -> (RunningTopology, mpsc::Receiver<Event>) {
        let mut builder = ConfigBuilder::default();

        let (tx, rx) = mpsc::channel(1);

        builder.add_source("in", UnitTestStreamSourceConfig::new(events));
        builder.add_transform("transform", &["in"], transform_config);
        builder.add_sink(
            "out",
            &["transform"],
            UnitTestStreamSinkConfig::new(
                PollSender::new(tx).sink_map_err(|error| panic!("{}", error)),
            ),
        );

        let config = builder.build().expect("building config should not fail");
        let (topology, _) = start_topology(config, false).await;

        (topology, rx)
    }
}
