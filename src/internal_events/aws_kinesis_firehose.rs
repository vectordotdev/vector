use super::prelude::{error_stage, error_type, http_error_code, io_error_code};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

use crate::sources::aws_kinesis_firehose::Compression;

#[derive(Debug)]
pub struct AwsKinesisFirehoseRequestReceived<'a> {
    pub request_id: Option<&'a str>,
    pub source_arn: Option<&'a str>,
}

impl<'a> InternalEvent for AwsKinesisFirehoseRequestReceived<'a> {
    fn emit(self) {
        debug!(
            message = "Handling AWS Kinesis Firehose request.",
            request_id = %self.request_id.unwrap_or_default(),
            source_arn = %self.source_arn.unwrap_or_default(),
            internal_log_rate_secs = 10
        );
        counter!("requests_received_total", 1);
    }
}

#[derive(Debug)]
pub struct AwsKinesisFirehoseRequestError<'a> {
    request_id: Option<&'a str>,
    error_code: String,
    error: &'a str,
}

impl<'a> AwsKinesisFirehoseRequestError<'a> {
    pub fn new(code: hyper::StatusCode, error: &'a str, request_id: Option<&'a str>) -> Self {
        Self {
            error_code: http_error_code(code.as_u16()),
            error,
            request_id,
        }
    }
}

impl<'a> InternalEvent for AwsKinesisFirehoseRequestError<'a> {
    fn emit(self) {
        error!(
            message = "Error occurred while handling request.",
            error = ?self.error,
            stage = error_stage::RECEIVING,
            error_type = error_type::REQUEST_FAILED,
            error_code = %self.error_code,
            internal_log_rate_secs = 10,
            request_id = %self.request_id.unwrap_or(""),
        );
        counter!(
            "component_errors_total", 1,
            "stage" => error_stage::RECEIVING,
            "error_type" => error_type::REQUEST_FAILED,
            "error_code" => self.error_code,
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
    fn emit(self) {
        error!(
            message = "Detected record failed to decode so passing along data as-is.",
            error = ?self.error,
            stage = error_stage::PROCESSING,
            error_type = error_type::PARSER_FAILED,
            error_code = %io_error_code(&self.error),
            internal_log_rate_secs = 10,
            compression = %self.compression,
        );
        counter!(
            "component_errors_total", 1,
            "stage" => error_stage::PROCESSING,
            "error_type" => error_type::PARSER_FAILED,
            "error_code" => io_error_code(&self.error),
        );
        // deprecated
        counter!("request_automatic_decode_errors_total", 1);
    }
}
