use std::sync::Arc;

use futures_util::{
    future,
    stream::{self, BoxStream},
    FutureExt, SinkExt, StreamExt,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{
    mpsc::{Receiver, Sender},
    oneshot, Mutex,
};
use vector_core::{
    event::{Event, LogEvent, Metric},
    sink::{StreamSink, VectorSink},
    transform::DataType,
};

use crate::{conditions::{self, Condition}, sinks::Healthcheck, sources};

use super::{SinkConfig, SinkContext, SourceConfig, SourceContext};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct UnitTestSourceConfig {
    // Wrapped to satisfy trait bounds
    #[serde(skip)]
    pub receiver: Arc<Mutex<Option<Receiver<Event>>>>,
    #[serde(skip)]
    pub events: Vec<Event>,
    pub input_log_events: Vec<LogEvent>,
    pub input_metric_events: Vec<Metric>,
}

#[async_trait::async_trait]
#[typetag::serde(name = "unit_test")]
impl SourceConfig for UnitTestSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<sources::Source> {
        // let mut receiver = self.receiver.lock().await.take().unwrap();

        let mut events = self.events.clone().into_iter().map(Ok);

        // let mut events = Vec::new();
        // let log_events = self.input_log_events.clone();
        // events.extend(log_events.into_iter().map(Event::Log).map(Ok));

        // let metric_events = self.input_metric_events.clone();
        // events.extend(metric_events.into_iter().map(Event::Metric).map(Ok));

        Ok(Box::pin(async move {
            let mut out = cx.out;
            let _shutdown = cx.shutdown;
            out.send_all(&mut stream::iter(events))
                .await
                .map_err(|_| ())?;
            // while let Some(event) = receiver.recv().await {
            //     println!("source received an event: {:?}", event);
            //     out.send(event)
            //         .await
            //         .map_err(|_| ())?;
            // }
            // println!("closing source...");
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

#[derive(Serialize, Deserialize, Default, Derivative)]
#[derivative(Debug)]
pub struct UnitTestSinkConfig {
    #[serde(skip)]
    pub result_tx: Arc<Mutex<Option<oneshot::Sender<Vec<String>>>>>,
    // need enrichment tables to build these conditions...current unit tests use Default::default
    #[serde(skip)]
    #[derivative(Debug = "ignore")]
    pub checks: Vec<Vec<Box<dyn Condition>>>,
    pub no_outputs: bool,
}

#[async_trait::async_trait]
#[typetag::serde(name = "unit_test")]
impl SinkConfig for UnitTestSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let tx = self.result_tx.lock().await.take().unwrap();
        let sink = UnitTestSink {
            result_tx: tx,
            checks: self.checks.clone(),
        };
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

pub struct UnitTestSink {
    pub result_tx: oneshot::Sender<Vec<String>>,
    pub checks: Vec<Vec<Box<dyn Condition>>>,
}

#[async_trait::async_trait]
impl StreamSink for UnitTestSink {
    async fn run(mut self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        // Check the input using the conditions
        // Send the results back to the unit test framework
        // Include some wrapping to associate the results, which condition the results came from

        let mut output_events = Vec::new();
        let mut test_errors = Vec::new();

        while let Some(event) = input.next().await {
            println!("sink received event: {:?}", event);
            output_events.push(event);
        }

        for check in self.checks {
            let mut overall_check_errors = Vec::new();
            for event in output_events.iter() {
                // todo: add correct error message
                let mut per_event_errors = Vec::new();
                let check = check.clone();
                for condition in check {
                    match condition.check_with_context(event) {
                        Ok(_) => {}
                        Err(error) => {
                            per_event_errors.push(error);
                        }
                    }
                }
                if per_event_errors.is_empty() {
                    overall_check_errors.clear();
                    break;
                } else {
                    overall_check_errors.extend(per_event_errors);
                }
            }
            // either one or more events passed the check or the check failed for one or more events.
            // if failed, we need to update the test errors
            if !overall_check_errors.is_empty() {
                test_errors.extend(overall_check_errors);
            }
        }

        if let Err(_) = self.result_tx.send(test_errors) {
            error!(message = "Sending unit test results failed in unit test sink.");
        }
        Ok(())
    }
}
