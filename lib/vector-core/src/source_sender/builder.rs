use std::{collections::HashMap, time::Duration};

use metrics::{Histogram, histogram};
use vector_buffers::topology::channel::LimitedReceiver;
use vector_common::internal_event::DEFAULT_OUTPUT;

use super::{
    CHUNK_SIZE, LAG_TIME_NAME, Output, OutputMetrics, SEND_BATCH_LATENCY_NAME, SEND_LATENCY_NAME,
    SourceSender, SourceSenderItem,
};
use crate::config::{ComponentKey, OutputId, SourceOutput};

pub struct Builder {
    buf_size: usize,
    default_output: Option<Output>,
    named_outputs: HashMap<String, Output>,
    lag_time: Option<Histogram>,
    send_latency: Option<Histogram>,
    send_batch_latency: Option<Histogram>,
    timeout: Option<Duration>,
    ewma_half_life_seconds: Option<f64>,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            buf_size: CHUNK_SIZE,
            default_output: None,
            named_outputs: Default::default(),
            lag_time: Some(histogram!(LAG_TIME_NAME)),
            send_latency: Some(histogram!(SEND_LATENCY_NAME)),
            send_batch_latency: Some(histogram!(SEND_BATCH_LATENCY_NAME)),
            timeout: None,
            ewma_half_life_seconds: None,
        }
    }
}

impl Builder {
    #[must_use]
    pub fn with_buffer(mut self, n: usize) -> Self {
        self.buf_size = n;
        self
    }

    #[must_use]
    pub fn with_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.timeout = timeout;
        self
    }

    #[must_use]
    pub fn with_ewma_half_life_seconds(mut self, half_life_seconds: Option<f64>) -> Self {
        self.ewma_half_life_seconds = half_life_seconds;
        self
    }

    pub fn add_source_output(
        &mut self,
        output: SourceOutput,
        component_key: ComponentKey,
    ) -> LimitedReceiver<SourceSenderItem> {
        let lag_time = self.lag_time.clone();
        let send_latency = self.send_latency.clone();
        let send_batch_latency = self.send_batch_latency.clone();
        let log_definition = output.schema_definition.clone();
        let output_id = OutputId {
            component: component_key,
            port: output.port.clone(),
        };
        match output.port {
            None => {
                let (output, rx) = Output::new_with_buffer(
                    self.buf_size,
                    DEFAULT_OUTPUT.to_owned(),
                    OutputMetrics::new(lag_time, send_latency, send_batch_latency),
                    log_definition,
                    output_id,
                    self.timeout,
                    self.ewma_half_life_seconds,
                );
                self.default_output = Some(output);
                rx
            }
            Some(name) => {
                let (output, rx) = Output::new_with_buffer(
                    self.buf_size,
                    name.clone(),
                    OutputMetrics::new(lag_time, send_latency, send_batch_latency),
                    log_definition,
                    output_id,
                    self.timeout,
                    self.ewma_half_life_seconds,
                );
                self.named_outputs.insert(name, output);
                rx
            }
        }
    }

    pub fn build(self) -> SourceSender {
        SourceSender {
            default_output: self.default_output,
            named_outputs: self.named_outputs,
        }
    }
}
