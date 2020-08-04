use super::InternalEvent;
use metrics::counter;
use serde_json::Error;
use string_cache::DefaultAtom as Atom;

#[derive(Debug)]
pub struct SplunkEventSent {
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
pub struct SplunkEventEncodeError {
    pub error: Error,
}

impl InternalEvent for SplunkEventEncodeError {
    fn emit_logs(&self) {
        error!(
            message = "Error encoding Splunk HEC event to json",
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
