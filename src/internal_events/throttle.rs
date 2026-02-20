use metrics::{counter, gauge};
use vector_lib::NamedInternalEvent;
use vector_lib::internal_event::{ComponentEventsDropped, INTENTIONAL, InternalEvent};

#[derive(Debug, NamedInternalEvent)]
pub(crate) struct ThrottleEventDiscarded {
    pub key: String,
    pub threshold_type: &'static str,
    pub emit_events_discarded_per_key: bool,
    pub emit_detailed_metrics: bool,
}

impl InternalEvent for ThrottleEventDiscarded {
    fn emit(self) {
        let message = "Rate limit exceeded.";

        debug!(message, key = %self.key, threshold_type = %self.threshold_type);

        // Backward compat: existing deprecated per-key counter
        if self.emit_events_discarded_per_key {
            counter!("events_discarded_total", "key" => self.key.clone()).increment(1); // Deprecated.
        }

        // New: detailed per-key per-threshold-type counter (opt-in)
        if self.emit_detailed_metrics {
            counter!(
                "throttle_events_discarded_total",
                "key" => self.key.clone(),
                "threshold_type" => self.threshold_type,
            )
            .increment(1);
        }

        // Always: bounded cardinality per-threshold-type counter (max 3 values)
        counter!(
            "throttle_threshold_discarded_total",
            "threshold_type" => self.threshold_type,
        )
        .increment(1);

        // Always: standard component metric
        emit!(ComponentEventsDropped::<INTENTIONAL> {
            count: 1,
            reason: message
        })
    }
}

/// Emitted for every event processed (passed or dropped) to track per-key volume.
#[derive(Debug, NamedInternalEvent)]
pub(crate) struct ThrottleEventProcessed {
    pub key: String,
    pub json_bytes: u64,
    pub token_cost: u64,
    pub emit_detailed_metrics: bool,
}

impl InternalEvent for ThrottleEventProcessed {
    fn emit(self) {
        if self.emit_detailed_metrics {
            counter!("throttle_events_processed_total", "key" => self.key.clone()).increment(1);
            if self.json_bytes > 0 {
                counter!("throttle_bytes_processed_total", "key" => self.key.clone())
                    .increment(self.json_bytes);
            }
            if self.token_cost > 0 {
                counter!("throttle_tokens_processed_total", "key" => self.key).increment(self.token_cost);
            }
        }
    }
}

/// Emits a gauge update for the utilization ratio of a key/threshold-type combination.
#[derive(Debug, NamedInternalEvent)]
pub(crate) struct ThrottleUtilizationUpdate {
    pub key: String,
    pub threshold_type: &'static str,
    pub ratio: f64,
}

impl InternalEvent for ThrottleUtilizationUpdate {
    fn emit(self) {
        gauge!(
            "throttle_utilization_ratio",
            "key" => self.key,
            "threshold_type" => self.threshold_type,
        )
        .set(self.ratio);
    }
}
