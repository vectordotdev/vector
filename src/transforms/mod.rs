#![allow(missing_docs)]
#[allow(unused_imports)]
use std::collections::HashSet;

use snafu::Snafu;

pub mod sample;

#[cfg(feature = "transforms-aggregate")]
pub mod aggregate;
#[cfg(feature = "transforms-aws_ec2_metadata")]
pub mod aws_ec2_metadata;
#[cfg(feature = "transforms-dedupe")]
pub mod dedupe;
#[cfg(feature = "transforms-filter")]
pub mod filter;
#[cfg(feature = "transforms-log_to_metric")]
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
#[cfg(feature = "transforms-tag_cardinality_limit")]
pub mod tag_cardinality_limit;
#[cfg(feature = "transforms-throttle")]
pub mod throttle;

pub use vector_lib::transform::{
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

#[cfg(test)]
mod test {
    use futures::Stream;
    use futures_util::SinkExt;
    use tokio::sync::mpsc;
    use tokio_util::sync::PollSender;
    use vector_lib::transform::FunctionTransform;

    use crate::{
        config::{
            unit_test::{UnitTestStreamSinkConfig, UnitTestStreamSourceConfig},
            ConfigBuilder, TransformConfig,
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
    pub async fn create_topology<T: TransformConfig + 'static>(
        events: impl Stream<Item = Event> + Send + 'static,
        transform_config: T,
    ) -> (RunningTopology, mpsc::Receiver<Event>) {
        let mut builder = ConfigBuilder::default();

        let (tx, rx) = mpsc::channel(1);

        // TODO: Use non-hard-coded names to improve tests.
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
