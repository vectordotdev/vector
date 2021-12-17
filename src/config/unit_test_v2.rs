use std::sync::Arc;

use futures_util::{
    future,
    stream::{self, BoxStream},
    FutureExt, SinkExt, StreamExt,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc::{Sender, Receiver}, Mutex, oneshot};
use vector_core::{
    event::{Event, LogEvent, Metric},
    sink::{StreamSink, VectorSink},
    transform::DataType,
};

use crate::{conditions, sinks::Healthcheck, sources};

use super::{SinkConfig, SinkContext, SourceConfig, SourceContext};

#[derive(Debug, Serialize, Deserialize)]
pub struct UnitTestSourceConfig {
    // Wrapped to satisfy trait bounds
    #[serde(skip)]
    pub receiver: Arc<Mutex<Option<Receiver<Event>>>>,
    // pub input_log_events: Vec<LogEvent>,
    // pub input_metric_events: Vec<Metric>,
}

#[async_trait::async_trait]
#[typetag::serde(name = "unit_test")]
impl SourceConfig for UnitTestSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<sources::Source> {
        let mut receiver = self.receiver.lock().await.take().unwrap();
        Ok(Box::pin(async move {
            let mut out = cx.out;
            let _shutdown = cx.shutdown;
            while let Some(event) = receiver.recv().await {
                println!("source received an event: {:?}", event);
                out.send(event)
                    .await
                    .map_err(|_| ())?;
            }
            println!("closing source...");
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
    #[serde(skip)]
    pub result_tx: Arc<Mutex<Option<oneshot::Sender<Vec<Event>>>>>,
    // need enrichment tables to build these conditions...current unit tests use Default::default
    // pub conditions: Vec<conditions::AnyCondition>,
}

#[async_trait::async_trait]
#[typetag::serde(name = "unit_test")]
impl SinkConfig for UnitTestSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let tx = self.result_tx.lock().await.take().unwrap();
        let sink = UnitTestSink::new(tx);
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
    result_tx: oneshot::Sender<Vec<Event>>,
}

impl UnitTestSink {
    fn new(result_tx: oneshot::Sender<Vec<Event>>) -> Self {
        Self {
            result_tx
        }
    }
}

#[async_trait::async_trait]
impl StreamSink for UnitTestSink {
    async fn run(mut self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        // Check the input using the conditions
        // Send the results back to the unit test framework
        // Include some wrapping to associate the results, which condition the results came from

        let mut results = Vec::new();
        while let Some(event) = input.next().await {
            println!("sink received event: {:?}", event);
            results.push(event);
        }
        if let Err(_) = self.result_tx.send(results) {
            error!(message = "Sending unit test results failed in unit test sink.");
        }
        println!("closing sink...");
        Ok(())
    }
}
