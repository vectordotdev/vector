#[cfg(feature = "sinks-splunk_hec")]
pub use self::sink::*;
#[cfg(feature = "sources-splunk_hec")]
pub use self::source::*;

#[cfg(feature = "sinks-splunk_hec")]
mod sink {
    use metrics::{counter, decrement_gauge, increment_gauge};
    use serde_json::Error;
    use vector_core::internal_event::InternalEvent;

    use crate::{
        event::metric::{MetricKind, MetricValue},
        sinks::splunk_hec::common::acknowledgements::HecAckApiError,
    };

    #[derive(Debug)]
    pub struct SplunkEventEncodeError {
        pub error: Error,
    }

    impl InternalEvent for SplunkEventEncodeError {
        fn emit_logs(&self) {
            error!(
                message = "Error encoding Splunk HEC event to JSON.",
                error = ?self.error,
                error_type = "encode_failed",
                stage = "processing",
                internal_log_rate_secs = 30,
            );
        }

        fn emit_metrics(&self) {
            counter!("component_errors_total", 1, "error_type" => "encode_failed", "stage" => "processing");
        }
    }

    #[derive(Debug)]
    pub(crate) struct SplunkInvalidMetricReceivedError<'a> {
        pub value: &'a MetricValue,
        pub kind: &'a MetricKind,
        pub error: crate::Error,
    }

    impl<'a> InternalEvent for SplunkInvalidMetricReceivedError<'a> {
        fn emit_logs(&self) {
            error!(
                message = "Invalid metric received.",
                error = ?self.error,
                value = ?self.value,
                kind = ?self.kind,
                error_type = "invalid_metric",
                stage = "processing",
                internal_log_rate_secs = 30,
            )
        }

        fn emit_metrics(&self) {
            counter!("component_errors_total", 1, "stage" => "processing", "error_type" => "invalid_metric");
            counter!("component_discarded_events_total", 1);
        }
    }

    #[derive(Debug)]
    pub struct SplunkResponseParseError {
        pub error: Error,
    }

    impl InternalEvent for SplunkResponseParseError {
        fn emit_logs(&self) {
            warn!(
                message = "Unable to parse Splunk HEC response. Acknowledging based on initial 200 OK.",
                error = ?self.error,
                internal_log_rate_secs = 30,
            );
        }
    }

    #[derive(Debug)]
    pub struct SplunkIndexerAcknowledgementAPIError {
        pub message: &'static str,
        pub error: HecAckApiError,
    }

    impl InternalEvent for SplunkIndexerAcknowledgementAPIError {
        fn emit_logs(&self) {
            error!(
                message = self.message,
                error = ?self.error,
                error_type = "acknowledgements_error",
                stage = "sending",
                internal_log_rate_secs = 30,
            );
        }

        fn emit_metrics(&self) {
            counter!("component_errors_total", 1, "error_type" => "acknowledgements_error", "stage" => "sending");
        }
    }

    #[derive(Debug)]
    pub struct SplunkIndexerAcknowledgementUnavailableError;

    impl InternalEvent for SplunkIndexerAcknowledgementUnavailableError {
        fn emit_logs(&self) {
            warn!(
                message = "Internal indexer acknowledgement client unavailable. Acknowledging based on initial 200 OK.",
                internal_log_rate_secs = 30,
            );
        }
    }

    pub struct SplunkIndexerAcknowledgementAckAdded;

    impl InternalEvent for SplunkIndexerAcknowledgementAckAdded {
        fn emit_metrics(&self) {
            increment_gauge!("splunk_pending_acks", 1.0);
        }
    }

    pub struct SplunkIndexerAcknowledgementAcksRemoved {
        pub count: f64,
    }

    impl InternalEvent for SplunkIndexerAcknowledgementAcksRemoved {
        fn emit_metrics(&self) {
            decrement_gauge!("splunk_pending_acks", self.count);
        }
    }
}

#[cfg(feature = "sources-splunk_hec")]
mod source {
    use metrics::counter;
    use vector_core::internal_event::InternalEvent;

    use crate::sources::splunk_hec::ApiError;

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
