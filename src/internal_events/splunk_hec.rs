use super::InternalEvent;
use metrics::counter;
use serde_json::Error;
use string_cache::DefaultAtom as Atom;

#[cfg(feature = "sources-splunk_hec")]
pub(crate) use self::source::*;

#[derive(Debug)]
pub(crate) struct SplunkEventSent {
    pub byte_size: usize,
}

impl InternalEvent for SplunkEventSent {
    fn emit_metrics(&self) {
        counter!(
            "events_processed", 1,
            "component_kind" => "sink",
            "component_type" => "splunk_hec",
        );
        counter!(
            "bytes_processed", self.byte_size as u64,
            "component_kind" => "sink",
            "component_type" => "splunk_hec",
        );
    }
}

#[derive(Debug)]
pub(crate) struct SplunkEventEncodeError {
    pub error: Error,
}

impl InternalEvent for SplunkEventEncodeError {
    fn emit_logs(&self) {
        error!(
            message = "error encoding Splunk HEC event to JSON.",
            error = ?self.error,
            rate_limit_secs = 30,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "encode_errors", 1,
            "component_kind" => "sink",
            "component_type" => "splunk_hec",
        );
    }
}

#[derive(Debug)]
pub struct SplunkSourceTypeMissingKeys {
    pub keys: Vec<Atom>,
}

impl InternalEvent for SplunkSourceTypeMissingKeys {
    fn emit_logs(&self) {
        warn!(
            message = "failed to render template for sourcetype, leaving empty",
            missing_keys = ?self.keys,
            rate_limit_secs = 30,
        )
    }

    fn emit_metrics(&self) {
        counter!(
            "sourcetype_missing_keys", 1,
            "component_kind" => "sink",
            "component_type" => "splunk_hec",
        );
    }
}

#[derive(Debug)]
pub struct SplunkSourceMissingKeys {
    pub keys: Vec<Atom>,
}

impl InternalEvent for SplunkSourceMissingKeys {
    fn emit_logs(&self) {
        warn!(
            message = "failed to render template for source, leaving empty",
            missing_keys = ?self.keys,
            rate_limit_secs = 30,
        )
    }

    fn emit_metrics(&self) {
        counter!(
            "source_missing_keys", 1,
            "component_kind" => "sink",
            "component_type" => "splunk_hec",
        );
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
            trace!(message = "received one event.");
        }

        fn emit_metrics(&self) {
            counter!(
                "events_processed", 1,
                "component_kind" => "source",
                "component_type" => "splunk_hec",
            );
        }
    }

    #[derive(Debug)]
    pub(crate) struct SplunkHECRequestReceived<'a> {
        pub path: &'a str,
    }

    impl<'a> InternalEvent for SplunkHECRequestReceived<'a> {
        fn emit_logs(&self) {
            debug!(
                message = "received one request.",
                path = %self.path,
                rate_limit_secs = 10
            );
        }

        fn emit_metrics(&self) {
            counter!(
                "request_received", 1,
                "component_kind" => "source",
                "component_type" => "splunk_hec",
            );
        }
    }

    #[derive(Debug)]
    pub(crate) struct SplunkHECRequestBodyInvalid {
        pub error: std::io::Error,
    }

    impl InternalEvent for SplunkHECRequestBodyInvalid {
        fn emit_logs(&self) {
            error!(
                message = "invalid request body.",
                error = %self.error,
                rate_limit_secs = 10
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
                message = "error processing request.",
                error = %self.error,
                rate_limit_secs = 10
            );
        }

        fn emit_metrics(&self) {
            counter!(
                "request_errors", 1,
                "component_kind" => "source",
                "component_type" => "splunk_hec",
            );
        }
    }
}
