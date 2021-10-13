use crate::event::metric::{MetricKind, MetricValue};
use metrics::counter;
use serde_json::Error;
use vector_core::internal_event::InternalEvent;

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

#[derive(Debug)]
pub(crate) struct SplunkInvalidMetricReceived<'a> {
    pub value: &'a MetricValue,
    pub kind: &'a MetricKind,
}

impl<'a> InternalEvent for SplunkInvalidMetricReceived<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Invalid metric received kind; dropping event.",
            value = ?self.value,
            kind = ?self.kind,
            internal_log_rate_secs = 30,
        )
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1, "error_type" => "invalid_metric_kind");
    }
}

#[cfg(feature = "sources-splunk_hec")]
mod source {
    use crate::sources::splunk_hec::ApiError;
    use metrics::counter;
    use vector_core::internal_event::InternalEvent;

    pub struct SplunkHecBytesReceived<'a> {
        pub byte_size: usize,
        pub protocol: &'a str,
        pub http_path: &'a str,
    }

    impl InternalEvent for SplunkHecBytesReceived<'_> {
        fn emit_logs(&self) {
            trace!(message = "Bytes received.", byte_size = %self.byte_size, protocol = %self.protocol, http_path = %self.http_path);
        }

        fn emit_metrics(&self) {
            counter!("component_received_bytes_total", self.byte_size as u64,
                "http_path" => self.http_path.to_string(),
                "protocol" => self.protocol.to_string());
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
    pub struct SplunkHecRequestBodyInvalidError {
        pub error: std::io::Error,
    }

    impl InternalEvent for SplunkHecRequestBodyInvalidError {
        fn emit_logs(&self) {
            error!(
                message = "Invalid request body.",
                error = ?self.error,
                error_type = "parse_failed",
                stage = "processing",
                internal_log_rate_secs = 10
            );
        }

        fn emit_metrics(&self) {
            counter!("component_errors_total", 1, "error_type" => "parse_failed", "stage" => "processing")
        }
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
                error_type = "http_error",
                stage = "receiving",
                internal_log_rate_secs = 10
            );
        }

        fn emit_metrics(&self) {
            counter!("http_request_errors_total", 1);
            counter!("component_errors_total", 1, "error_type" => "http_error", "stage" => "receiving")
        }
    }
}
