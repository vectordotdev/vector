use std::sync::Arc;

use futures_util::{
    future,
    stream::{self, BoxStream},
    FutureExt, SinkExt, StreamExt,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{oneshot, Mutex};
use vector_core::{
    event::Event,
    sink::{StreamSink, VectorSink},
    transform::DataType,
};

use crate::{
    conditions::{self, Condition},
    sinks::Healthcheck,
    sources,
};

use super::{SinkConfig, SinkContext, SourceConfig, SourceContext};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct UnitTestSourceConfig {
    #[serde(skip)]
    pub events: Vec<Event>,
}

#[async_trait::async_trait]
#[typetag::serde(name = "unit_test")]
impl SourceConfig for UnitTestSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<sources::Source> {
        let mut events = self.events.clone().into_iter().map(Ok);

        Ok(Box::pin(async move {
            let mut out = cx.out;
            let _shutdown = cx.shutdown;
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

#[derive(Clone)]
pub enum UnitTestSinkCheck {
    // Check sets of conditions against received events
    Checks(Vec<Vec<Box<dyn Condition>>>),
    // Check that no events were received
    NoOutputs,
}

impl Default for UnitTestSinkCheck {
    fn default() -> Self {
        UnitTestSinkCheck::NoOutputs
    }
}

#[derive(Debug)]
pub struct UnitTestSinkResult {
    pub name: String,
    pub test_errors: Vec<String>,
    pub test_inspections: Vec<String>,
}

#[derive(Serialize, Deserialize, Default, Derivative)]
#[derivative(Debug)]
pub struct UnitTestSinkConfig {
    // Name of the test associated with this sink
    pub name: String,
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
            name: self.name.clone(),
            result_tx: tx,
            check: self.check.clone(),
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
    pub name: String,
    pub result_tx: oneshot::Sender<UnitTestSinkResult>,
    pub check: UnitTestSinkCheck,
}

#[async_trait::async_trait]
impl StreamSink for UnitTestSink {
    async fn run(mut self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        let mut output_events = Vec::new();
        let mut result = UnitTestSinkResult {
            name: self.name,
            test_errors: Vec::new(),
            test_inspections: Vec::new(),
        };

        // Receive all incoming events
        while let Some(event) = input.next().await {
            println!("sink received event: {:?}\n", event);
            output_events.push(event);
        }

        match self.check {
            UnitTestSinkCheck::Checks(checks) => {
                if output_events.is_empty() {
                    result
                        .test_errors
                        .push("check transform failed, no events received".to_string());
                }
                for check in checks {
                    if check.is_empty() {
                        // result.test_inspections.push(format!(
                        //     "check transform '{}' payloads (events encoded as JSON):\n{}\n{}",
                        //     self.name,
                        //     events_to_string("input", inputs),
                        //     events_to_string("output", outputs),
                        // ));
                        continue;
                    }

                    let mut check_errors = Vec::new();
                    for condition in check {
                        let mut condition_errors = Vec::new();
                        for event in output_events.iter() {
                            match condition.check_with_context(event) {
                                Ok(_) => {
                                    condition_errors.clear();
                                    break;
                                }
                                Err(error) => {
                                    condition_errors.push(error);
                                }
                            }
                        }
                        check_errors.extend(condition_errors);
                    }

                    result.test_errors.extend(check_errors);
                }
            }
            UnitTestSinkCheck::NoOutputs => {
                if !output_events.is_empty() {
                    result
                        .test_errors
                        .push("expected no outputs".to_string());
                }
            }
        }

        if let Err(_) = self.result_tx.send(result) {
            error!(message = "Sending unit test results failed in unit test sink.");
        }
        Ok(())
    }
}
