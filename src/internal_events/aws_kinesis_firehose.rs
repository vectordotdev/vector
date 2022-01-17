// ## skip check-events ##

use metrics::counter;
use vector_core::internal_event::InternalEvent;

use crate::sources::aws_kinesis_firehose::Compression;

#[derive(Debug)]
pub struct AwsKinesisFirehoseEventsReceived {
    pub count: usize,
    pub byte_size: usize,
}

impl InternalEvent for AwsKinesisFirehoseEventsReceived {
    fn emit_logs(&self) {
        trace!(message = "Events received.", count = %self.count, byte_size = %self.byte_size);
    }

    fn emit_metrics(&self) {
        counter!("component_received_events_total", self.count as u64);
        counter!(
            "component_received_event_bytes_total",
            self.byte_size as u64
        );
        counter!("events_in_total", self.count as u64); // deprecated
        counter!("processed_bytes_total", self.byte_size as u64); // deprecated
    }
}

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
            internal_log_rate_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("requests_received_total", 1);
    }
}

#[derive(Debug)]
pub struct AwsKinesisFirehoseRequestError<'a> {
    pub request_id: Option<&'a str>,
    pub code: hyper::StatusCode,
    pub error: &'a str,
}

impl<'a> InternalEvent for AwsKinesisFirehoseRequestError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Error occurred while handling request.",
            error = ?self.error,
            error_type = "http_error",
            code = %self.code,
            stage = "receiving",
            internal_log_rate_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("request_read_errors_total", 1);
        counter!(
            "component_errors_total", 1,
            "stage" => "receiving",
            "error_type" => "http_error",
            "code" => self.code.to_string(),
        )
    }
}

#[derive(Debug)]
pub struct AwsKinesisFirehoseAutomaticRecordDecodeError {
    pub compression: Compression,
    pub error: std::io::Error,
}

impl InternalEvent for AwsKinesisFirehoseAutomaticRecordDecodeError {
    fn emit_logs(&self) {
        warn!(
            message = %format!("Detected record as {} but failed to decode so passing along data as-is.", self.compression),
            error = ?self.error,
            stage = "processing",
            error_type = "decoding_error",
            internal_log_rate_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("request_automatic_decode_errors_total", 1);
        counter!(
            "component_errors_total", 1,
            "stage" => "processing",
            "error_type" => "decoding_error",
        )
    }
}
