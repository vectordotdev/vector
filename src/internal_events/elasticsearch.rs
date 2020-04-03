use super::InternalEvent;
use metrics::counter;
use string_cache::DefaultAtom as Atom;

pub struct ElasticSearchEventReceived {
    pub byte_size: usize,
}

impl InternalEvent for ElasticSearchEventReceived {
    fn emit_metrics(&self) {
        counter!(
            "events_received", 1,
            "component_kind" => "sink",
            "component_type" => "elasticsearch",
        );
        counter!(
            "bytes_received", self.byte_size as u64,
            "component_kind" => "sink",
            "component_type" => "elasticsearch",
        );
    }
}

pub struct ElasticSearchMissingKeys {
    pub keys: Vec<Atom>,
}

impl InternalEvent for ElasticSearchMissingKeys {
    fn emit_logs(&self) {
        warn!(
            message = "Keys do not exist on the event; Dropping event.",
            missing_keys = ?self.keys,
            rate_limit_secs = 30,
        )
    }

    fn emit_metrics(&self) {
        counter!(
            "missing_keys", 1,
            "component_kind" => "sink",
            "component_type" => "elasticsearch",
        );
    }
}
