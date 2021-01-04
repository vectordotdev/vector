use super::InternalEvent;
use metrics::counter;
use serde_json::Error;

#[cfg(feature = "sources-splunk_hec")]
pub(crate) use self::source::*;

#[derive(Debug)]
pub(crate) struct SplunkEventSent {
    pub byte_size: usize,
}

impl InternalEvent for SplunkEventSent {
    fn emit_metrics(&self) {
        counter!("processed_events_total", 1);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub(crate) struct SplunkEventEncodeError {
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

#[derive(Debug)]
pub struct SplunkMissingKeys<'a> {
    pub field: &'static str,
    pub keys: &'a [String],
}

impl<'a> InternalEvent for SplunkMissingKeys<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Failed to render template for {}, leaving empty.",
            self.field,
            missing_keys = ?self.keys,
            internal_log_rate_secs = 30,
        )
    }

    fn emit_metrics(&self) {
        counter!("missing_keys_total", 1);
    }
}

#[cfg(feature = "sources-splunk_hec")]
mod source {
    use super::InternalEvent;
    use crate::sources::splunk_hec::ApiError;
    use metrics::counter;

    #[derive(Debug)]
    pub(crate) struct SplunkHECEventReceived;

    impl InternalEvent for SplunkHECEventReceived {
        fn emit_logs(&self) {
            trace!(message = "Received one event.");
        }

        fn emit_metrics(&self) {
            counter!("processed_events_total", 1);
        }
    }

    #[derive(Debug)]
    pub(crate) struct SplunkHECRequestReceived<'a> {
        pub path: &'a str,
    }

    impl<'a> InternalEvent for SplunkHECRequestReceived<'a> {
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
    pub(crate) struct SplunkHECRequestBodyInvalid {
        pub error: std::io::Error,
    }

    impl InternalEvent for SplunkHECRequestBodyInvalid {
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
    pub(crate) struct SplunkHECRequestError {
        pub(crate) error: ApiError,
    }

    impl InternalEvent for SplunkHECRequestError {
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
