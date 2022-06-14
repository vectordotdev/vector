// Set of `stage` tags to use when emiting error events.
pub mod error_stage {
    pub const RECEIVING: &str = "receiving";
    pub const PROCESSING: &str = "processing";
    pub const SENDING: &str = "sending";
}

// Set of `error_type` tags to use when emiting error events.
pub mod error_type {
    // When the event acknowledgment failed.
    pub const ACKNOWLEDGMENT_FAILED: &str = "acknowledgment_failed";
    // When the external command called by the component failed.
    pub const COMMAND_FAILED: &str = "command_failed";
    // When a condition for the event to be valid failed.
    // This is used for example when a field is missing or should be a string.
    pub const CONDITION_FAILED: &str = "condition_failed";
    // When the component or the service on which it depends is not configured properly.
    pub const CONFIGURATION_FAILED: &str = "configuration_failed";
    // When the component failed to connect to an external service.
    pub const CONNECTION_FAILED: &str = "connection_failed";
    // When the component failed to convert a value.
    // For example, when converting from string to float.
    pub const CONVERSION_FAILED: &str = "conversion_failed";
    // When the component failed to convert an event to a structure required
    // by the external service the event should be sent to.
    pub const ENCODER_FAILED: &str = "encoder_failed";
    // When the received event has an unexpected metric.
    pub const INVALID_METRIC: &str = "invalid_metric";
    // When the component was unable to perform an IO.
    pub const IO_FAILED: &str = "io_failed";
    // When the component is unable to parse a message to build an event.
    pub const PARSER_FAILED: &str = "parser_failed";
    // When the component was unable to read from the source.
    pub const READER_FAILED: &str = "reader_failed";
    // When the component was unable to perform a request or the request failed.
    pub const REQUEST_FAILED: &str = "request_failed";
    // When the component depends on a script that failed
    pub const SCRIPT_FAILED: &str = "script_failed";
    // When the component was unable to build a template or interpolate it.
    pub const TEMPLATE_FAILED: &str = "template_failed";
    // When an execution took longer than expected and failed.
    pub const TIMED_OUT: &str = "timed_out";
    // When the component was unable to write some data.
    pub const WRITER_FAILED: &str = "writer_failed";
}

#[cfg(any(
    feature = "sinks-azure_blob",
    feature = "sinks-elasticsearch",
    feature = "sinks-prometheus",
    feature = "sources-apache_metrics",
    feature = "sources-aws_ecs_metrics",
    feature = "sources-aws_kinesis_firehose",
    feature = "sources-prometheus",
    feature = "sources-utils-http",
))]
pub(crate) fn http_error_code(code: u16) -> String {
    format!("http_response_{}", code)
}

pub(crate) fn io_error_code(error: &std::io::Error) -> &'static str {
    use std::io::ErrorKind::*;

    // there are many more gated behind https://github.com/rust-lang/rust/issues/86442
    match error.kind() {
        AddrInUse => "address_in_use",
        AddrNotAvailable => "address_not_available",
        AlreadyExists => "entity_already_exists",
        BrokenPipe => "broken_pipe",
        ConnectionAborted => "connection_aborted",
        ConnectionRefused => "connection_refused",
        ConnectionReset => "connection_reset",
        Interrupted => "operation_interrupted",
        InvalidData => "invalid_data",
        InvalidInput => "invalid_input_parameter",
        NotConnected => "not_connected",
        NotFound => "entity_not_found",
        Other => "other_error",
        OutOfMemory => "out_of_memory",
        PermissionDenied => "permission_denied",
        TimedOut => "timed_out",
        UnexpectedEof => "unexpected_end_of_file",
        Unsupported => "unsupported",
        WouldBlock => "operation_would_block",
        WriteZero => "write_zero",
        _ => "unknown",
    }
}

#[cfg(feature = "sources-aws_ecs_metrics")]
pub(crate) fn hyper_error_code(error: &hyper::Error) -> &'static str {
    if error.is_body_write_aborted() {
        "body_write_aborted"
    } else if error.is_canceled() {
        "cancelled"
    } else if error.is_closed() {
        "sender_closed"
    } else if error.is_connect() {
        "connect_error"
    } else if error.is_incomplete_message() {
        "incomplete_message"
    } else if error.is_parse() {
        "parse_error"
    } else if error.is_parse_status() {
        "parse_status_error"
    } else if error.is_parse_too_large() {
        "parse_too_large"
    } else if error.is_timeout() {
        "timeout"
    } else if error.is_user() {
        "user"
    } else {
        "unknown"
    }
}
