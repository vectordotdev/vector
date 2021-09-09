use super::InternalEvent;
use metrics::counter;
use serde_json::Error;

#[cfg(feature = "sources-splunk_hec")]
pub use self::source::*;

#[derive(Debug)]
pub struct SplunkEventSent {
    pub byte_size: usize,
}

impl InternalEvent for SplunkEventSent {
    fn emit_metrics(&self) {
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct SplunkEventEncodeError {
    pub error: Error,
}

impl InternalEvent for SplunkEventEncodeError {
    fn emit_logs(&self) {
        error!(
            message = "Error encoding Splunk HEC event to JSON.",
            error = ?self.error,
            internal_log_rate_secs = 30,
        );
    }

    fn emit_metrics(&self) {
        counter!("encode_errors_total", 1);
    }
}

#[cfg(feature = "sources-splunk_hec")]
mod source {
    use super::InternalEvent;
    use crate::sources::splunk_hec::ApiError;
    use metrics::counter;

    #[derive(Debug)]
    pub struct SplunkHecEventReceived;

    impl InternalEvent for SplunkHecEventReceived {
        fn emit_logs(&self) {
            trace!(message = "Received one event.");
        }

        fn emit_metrics(&self) {
            counter!("events_in_total", 1);
        }
    }

    #[derive(Debug)]
    pub struct SplunkHecRequestReceived<'a> {
        pub path: &'a str,
    }

    impl<'a> InternalEvent for SplunkHecRequestReceived<'a> {
        fn emit_logs(&self) {
            debug!(
                message = "Received one request.",
                path = %self.path,
                internal_log_rate_secs = 10
            );
        }

        fn emit_metrics(&self) {
            counter!("requests_received_total", 1);
        }
    }

    #[derive(Debug)]
    pub struct SplunkHecRequestBodyInvalid {
        pub error: std::io::Error,
    }

    impl InternalEvent for SplunkHecRequestBodyInvalid {
        fn emit_logs(&self) {
            error!(
                message = "Invalid request body.",
                error = ?self.error,
                internal_log_rate_secs = 10
            );
        }

        fn emit_metrics(&self) {}
    }

    #[derive(Debug)]
    pub struct SplunkHecRequestError {
        pub(crate) error: ApiError,
    }

    impl InternalEvent for SplunkHecRequestError {
        fn emit_logs(&self) {
            error!(
                message = "Error processing request.",
                error = ?self.error,
                internal_log_rate_secs = 10
            );
        }

        fn emit_metrics(&self) {
            counter!("http_request_errors_total", 1);
        }
    }
}
