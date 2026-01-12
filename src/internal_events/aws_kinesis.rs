/// Used in both `aws_kinesis_streams` and `aws_kinesis_firehose` sinks
use metrics::counter;
use vector_lib::NamedInternalEvent;
use vector_lib::internal_event::{
    ComponentEventsDropped, InternalEvent, UNINTENTIONAL, error_stage, error_type,
};

#[derive(Debug, NamedInternalEvent)]
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
        );

        counter!(
            "component_errors_total",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason });
    }
}
