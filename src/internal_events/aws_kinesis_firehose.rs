use super::InternalEvent;
use crate::sources::aws_kinesis_firehose;
use metrics::counter;

#[derive(Debug)]
pub struct AwsKinesisFirehoseRequestReceived<'a> {
    pub request_id: &'a str,
    pub source_arn: &'a str,
}

impl<'a> InternalEvent for AwsKinesisFirehoseRequestReceived<'a> {
    fn emit_logs(&self) {
        info!(
            message = "Handling AWS Kinesis Firehose request.",
            request_id = %self.request_id,
            source_arn = %self.source_arn,
            rate_limit_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("requests_received", 1,
            "component_kind" => "source",
            "component_type" => "aws_kinesis_firehose",
        );
    }
}

#[derive(Debug)]
pub struct AwsKinesisFirehoseRequestError<'a> {
    pub request_id: Option<&'a str>,
    pub error: &'a aws_kinesis_firehose::errors::RequestError,
}

impl<'a> InternalEvent for AwsKinesisFirehoseRequestError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "error handling request",
            error = ?self.error,
            rate_limit_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "request_read_errors", 1,
            "component_kind" => "source",
            "component_type" => "aws_kinesis_firehose",
        );
    }
}
