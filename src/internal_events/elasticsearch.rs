use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct ElasticSearchEventEncoded {
    pub index: String,
}

impl InternalEvent for ElasticSearchEventEncoded {
    fn emit_logs(&self) {
        trace!(message = "Encoding event.", index = %self.index);
    }
}

#[derive(Debug)]
pub struct ElasticSearchEventSent {
    pub batch_size: usize,
    pub byte_size: usize,
}

impl InternalEvent for ElasticSearchEventSent {
    fn emit_metrics(&self) {
        counter!("processed_events_total", self.batch_size as u64);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct ElasticSearchMissingKeys<'a> {
    pub keys: &'a [String],
}

impl<'a> InternalEvent for ElasticSearchMissingKeys<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Keys do not exist on the event; dropping event.",
            missing_keys = ?self.keys,
            internal_log_rate_secs = 30,
        )
    }

    fn emit_metrics(&self) {
        counter!("missing_keys_total", 1);
    }
}
