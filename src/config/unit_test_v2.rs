use futures_util::{
    future,
    stream::{self, BoxStream},
    FutureExt, SinkExt,
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;
use vector_core::{
    event::{Event, LogEvent, Metric},
    sink::{StreamSink, VectorSink},
    transform::DataType,
};

use crate::{conditions, sinks::Healthcheck, sources};

use super::{SinkConfig, SinkContext, SourceConfig, SourceContext};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct UnitTestSource {
    pub input_log_events: Vec<LogEvent>,
    pub input_metric_events: Vec<Metric>,
}

#[async_trait::async_trait]
#[typetag::serde(name = "unit_test")]
impl SourceConfig for UnitTestSource {
    async fn build(&self, cx: SourceContext) -> crate::Result<sources::Source> {
        let mut out = cx.out;

        let mut events = Vec::new();
        let log_events = self.input_log_events.clone();
        events.extend(log_events.into_iter().map(Event::Log).map(Ok));

        let metric_events = self.input_metric_events.clone();
        events.extend(metric_events.into_iter().map(Event::Metric).map(Ok));

        Ok(Box::pin(async move {
            out.send_all(&mut stream::iter(events))
                .await
                .map_err(|_| ())?;
            Ok(())
        }))
    }

    fn output_type(&self) -> DataType {
        DataType::Any
    }

    fn source_type(&self) -> &'static str {
        "unit_test"
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct UnitTestSinkConfig {
    // need enrichment tables to build these conditions...current unit tests use Default::default
    pub conditions: Vec<conditions::AnyCondition>,
}

#[async_trait::async_trait]
#[typetag::serde(name = "unit_test")]
impl SinkConfig for UnitTestSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let sink = UnitTestSink::new();
        let healthcheck = future::ok(()).boxed();

        Ok((VectorSink::Stream(Box::new(sink)), healthcheck))
    }

    fn sink_type(&self) -> &'static str {
        "unit_test"
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }
}

pub struct UnitTestSink;

impl UnitTestSink {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl StreamSink for UnitTestSink {
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        // Check the input using the conditions
        // Send the results back to the unit test framework
        // Include some wrapping to associate the results, which condition the results came from
        println!("I'm in the sink!");
        Ok(())
    }
}
