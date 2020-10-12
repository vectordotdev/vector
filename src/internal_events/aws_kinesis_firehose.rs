use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct AwsKinesisFirehoseRequestReceived<'a> {
    pub request_id: Option<&'a str>,
    pub source_arn: Option<&'a str>,
}

impl<'a> InternalEvent for AwsKinesisFirehoseRequestReceived<'a> {
    fn emit_logs(&self) {
        info!(
            message = "Handling AWS Kinesis Firehose request.",
            request_id = %self.request_id.unwrap_or_default(),
            source_arn = %self.source_arn.unwrap_or_default(),
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
    pub error: &'a str,
}

impl<'a> InternalEvent for AwsKinesisFirehoseRequestError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Error occurred while handling request.",
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
