// ## skip check-events ##

use metrics::counter;
use vector_core::internal_event::InternalEvent;

use crate::sources::aws_kinesis_firehose::Compression;

#[derive(Debug)]
pub struct AwsKinesisFirehoseBytesReceived {
    pub byte_size: usize,
}

impl InternalEvent for AwsKinesisFirehoseBytesReceived {
    fn emit_logs(&self) {
        trace!(
            message = "Bytes received.",
            byte_size = %self.byte_size,
            protocol = "http",
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_received_bytes_total", self.byte_size as u64,
            "protocol" => "http",
        );
    }
}

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
        // deprecated
        counter!("events_in_total", self.count as u64);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct AwsKinesisFirehoseEventsSent {
    pub count: usize,
    pub byte_size: usize,
}

impl InternalEvent for AwsKinesisFirehoseEventsSent {
    fn emit_logs(&self) {
        trace!(message = "Events sent.", count = %self.count, byte_size = %self.byte_size);
    }

    fn emit_metrics(&self) {
        counter!("component_sent_events_total", self.count as u64);
        counter!("component_sent_event_bytes_total", self.byte_size as u64);
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
        counter!(
            "component_errors_total", 1,
            "stage" => "receiving",
            "error_type" => "http_error",
            "code" => self.code.to_string(),
        );
        // deprecated
        counter!("request_read_errors_total", 1);
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
        counter!(
            "component_errors_total", 1,
            "stage" => "processing",
            "error_type" => "decoding_error",
        );
        // deprecated
        counter!("request_automatic_decode_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct AwsKinesisFirehoseStreamError {
    pub error: String,
    pub request_id: String,
    pub count: usize,
}

impl InternalEvent for AwsKinesisFirehoseStreamError {
    fn emit_logs(&self) {
        error!(
            message = "Failed to forward events, downstream is closed",
            error = %self.error,
            error_type = "stream",
            stage = "sending",
            request_id = %self.request_id,
            count = self.count,
        )
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", self.count as u64,
            "stage" => "sending",
            "error" => self.error.clone(),
            "errot_type" => "stream",
            "request_id" => self.request_id.clone(),
        );
        counter!(
            "component_discarded_events_total", self.count as u64,
            "stage" => "sending",
            "error" => self.error.clone(),
            "errot_type" => "stream",
            "request_id" => self.request_id.clone(),
        );
    }
}
