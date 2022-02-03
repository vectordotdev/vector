use std::sync::Arc;

use futures_util::{
    future,
    stream::{self, BoxStream},
    FutureExt, StreamExt,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{oneshot, Mutex};
use vector_core::{
    config::{DataType, Output},
    event::Event,
    sink::{StreamSink, VectorSink},
};

use crate::{
    conditions::Condition,
    config::{SinkConfig, SinkContext, SourceConfig, SourceContext},
    sinks::Healthcheck,
    sources,
};

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct UnitTestSourceConfig {
    #[serde(skip)]
    pub events: Vec<Event>,
}

#[async_trait::async_trait]
#[typetag::serde(name = "unit_test")]
impl SourceConfig for UnitTestSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<sources::Source> {
        let events = self.events.clone().into_iter();

        Ok(Box::pin(async move {
            let mut out = cx.out;
            // To appropriately shut down the topology after the source is done
            // sending events, we need to hold on to this shutdown trigger.
            let _shutdown = cx.shutdown;
            out.send_all(&mut stream::iter(events))
                .await
                .map_err(|_| ())?;
            Ok(())
        }))
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::Any)]
    }

    fn source_type(&self) -> &'static str {
        "unit_test"
    }
}

#[derive(Clone)]
pub enum UnitTestSinkCheck {
    // Check sets of conditions against received events
    Checks(Vec<Vec<Box<dyn Condition>>>),
    // Check that no events were received
    NoOutputs,
    // Do nothing
    NoOp,
}

impl Default for UnitTestSinkCheck {
    fn default() -> Self {
        UnitTestSinkCheck::NoOp
    }
}

#[derive(Debug)]
pub struct UnitTestSinkResult {
    pub test_name: String,
    pub test_errors: Vec<String>,
}

#[derive(Serialize, Deserialize, Default, Derivative)]
#[derivative(Debug)]
pub struct UnitTestSinkConfig {
    // Name of the test this sink is part of
    pub test_name: String,
    // Name of the transform/branch associated with this sink
    pub transform_id: String,
    #[serde(skip)]
    // Sender used to transmit the test result
    pub result_tx: Arc<Mutex<Option<oneshot::Sender<UnitTestSinkResult>>>>,
    #[serde(skip)]
    #[derivative(Debug = "ignore")]
    // Check applied to incoming events
    pub check: UnitTestSinkCheck,
}

#[async_trait::async_trait]
#[typetag::serde(name = "unit_test")]
impl SinkConfig for UnitTestSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let tx = self.result_tx.lock().await.take().unwrap();
        let sink = UnitTestSink {
            test_name: self.test_name.clone(),
            transform_id: self.transform_id.clone(),
            result_tx: tx,
            check: self.check.clone(),
        };
        let healthcheck = future::ok(()).boxed();

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn sink_type(&self) -> &'static str {
        "unit_test"
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }
}

pub struct UnitTestSink {
    pub test_name: String,
    pub transform_id: String,
    pub result_tx: oneshot::Sender<UnitTestSinkResult>,
    pub check: UnitTestSinkCheck,
}

#[async_trait::async_trait]
impl StreamSink<Event> for UnitTestSink {
    async fn run(mut self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        let mut output_events = Vec::new();
        let mut result = UnitTestSinkResult {
            test_name: self.test_name,
            test_errors: Vec::new(),
        };

        while let Some(event) = input.next().await {
            output_events.push(event);
        }

        match self.check {
            UnitTestSinkCheck::Checks(checks) => {
                if output_events.is_empty() {
                    result
                        .test_errors
                        .push(format!("checks for transform {:?} failed: no events received. Topology may be disconnected or transform is missing inputs.", self.transform_id));
                } else {
                    for (i, check) in checks.iter().enumerate() {
                        let mut check_errors = Vec::new();
                        for (j, condition) in check.iter().enumerate() {
                            let mut condition_errors = Vec::new();
                            for event in output_events.iter() {
                                match condition.check_with_context(event) {
                                    Ok(_) => {
                                        condition_errors.clear();
                                        break;
                                    }
                                    Err(error) => {
                                        condition_errors
                                            .push(format!("  condition[{}]: {}", j, error));
                                    }
                                }
                            }
                            check_errors.extend(condition_errors);
                        }
                        // If there are errors, add a preamble to the output
                        if !check_errors.is_empty() {
                            check_errors.insert(
                                0,
                                format!(
                                    "check[{}] for transform {:?} failed conditions:",
                                    i, self.transform_id
                                ),
                            );
                        }

                        result.test_errors.extend(check_errors);
                    }

                    // If there are errors, add a summary of events received
                    if !result.test_errors.is_empty() {
                        result.test_errors.push(format!(
                            "output payloads from {:?} (events encoded as JSON):\n  {}",
                            self.transform_id,
                            events_to_string(&output_events)
                        ));
                    }
                }
            }
            UnitTestSinkCheck::NoOutputs => {
                if !output_events.is_empty() {
                    result.test_errors.push(format!(
                        "check for transform {:?} failed: expected no outputs",
                        self.transform_id
                    ));
                }
            }
            UnitTestSinkCheck::NoOp => {}
        }

        if self.result_tx.send(result).is_err() {
            error!(message = "Sending unit test results failed in unit test sink.");
        }
        Ok(())
    }
}

fn events_to_string(events: &[Event]) -> String {
    events
        .iter()
        .map(|event| match event {
            Event::Log(log) => serde_json::to_string(log).unwrap_or_else(|_| "{}".to_string()),
            Event::Metric(metric) => {
                serde_json::to_string(metric).unwrap_or_else(|_| "{}".to_string())
            }
        })
        .collect::<Vec<_>>()
        .join("\n  ")
}
