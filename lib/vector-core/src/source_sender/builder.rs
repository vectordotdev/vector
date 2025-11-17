use std::{collections::HashMap, time::Duration};

use metrics::{Histogram, histogram};
use vector_buffers::topology::channel::LimitedReceiver;
use vector_common::internal_event::DEFAULT_OUTPUT;

use super::{CHUNK_SIZE, LAG_TIME_NAME, Output, SourceSender, SourceSenderItem};
use crate::config::{ComponentKey, OutputId, SourceOutput};

pub struct Builder {
    buf_size: usize,
    default_output: Option<Output>,
    named_outputs: HashMap<String, Output>,
    lag_time: Option<Histogram>,
    timeout: Option<Duration>,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            buf_size: CHUNK_SIZE,
            default_output: None,
            named_outputs: Default::default(),
            lag_time: Some(histogram!(LAG_TIME_NAME)),
            timeout: None,
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

    pub fn add_source_output(
        &mut self,
        output: SourceOutput,
        component_key: ComponentKey,
    ) -> LimitedReceiver<SourceSenderItem> {
        let lag_time = self.lag_time.clone();
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
                    lag_time,
                    log_definition,
                    output_id,
                    self.timeout,
                );
                self.default_output = Some(output);
                rx
            }
            Some(name) => {
                let (output, rx) = Output::new_with_buffer(
                    self.buf_size,
                    name.clone(),
                    lag_time,
                    log_definition,
                    output_id,
                    self.timeout,
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
