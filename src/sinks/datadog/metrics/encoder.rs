use std::io;

use vector_core::event::Metric;

use crate::sinks::util::encoding::Encoder;

pub struct DatadogMetricsEncoder;

impl Encoder<Vec<Metric>> for DatadogMetricsEncoder {
    fn encode_input(&self, input: Vec<Metric>, writer: &mut dyn io::Write) -> io::Result<usize> {
        todo!()
    }
}
