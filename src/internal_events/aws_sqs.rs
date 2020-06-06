use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct AwsSqsEventSent {
    pub byte_size: usize,
}

impl InternalEvent for AwsSqsEventSent {
    fn emit_metrics(&self) {
        counter!(
            "events_processed", 1,
            "component_kind" => "sink",
            "component_type" => "aws_sqs",
        );
        counter!(
            "bytes_processed", self.byte_size as u64,
            "component_kind" => "sink",
            "component_type" => "aws_sqs",
        );
    }
}
