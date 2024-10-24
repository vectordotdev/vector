#[cfg(any(
    feature = "sources-apache_metrics",
    feature = "sources-aws_ecs_metrics",
    feature = "sources-aws_kinesis_firehose",
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
