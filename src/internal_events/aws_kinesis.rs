/// Used in both `aws_kinesis_streams` and `aws_kinesis_firehose` sinks
use metrics::counter;
use vector_lib::internal_event::InternalEvent;
use vector_lib::internal_event::{error_stage, error_type, ComponentEventsDropped, UNINTENTIONAL};

#[derive(Debug)]
pub struct AwsKinesisStreamNoPartitionKeyError<'a> {
    pub partition_key_field: &'a str,
}

impl InternalEvent for AwsKinesisStreamNoPartitionKeyError<'_> {
    fn emit(self) {
        let reason = "Partition key does not exist.";

        error!(
            message = reason,
            partition_key_field = %self.partition_key_field,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );

        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        );

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason });
    }
}
