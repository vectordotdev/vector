use std::time::Duration;

use metrics::histogram;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct AdaptiveConcurrencyLimit {
    pub concurrency: u64,
    pub reached_limit: bool,
    pub had_back_pressure: bool,
    pub current_rtt: Option<Duration>,
    pub past_rtt: Duration,
    pub past_rtt_deviation: Duration,
}

impl InternalEvent for AdaptiveConcurrencyLimit {
    fn emit_logs(&self) {
        trace!(
            message = "Changed concurrency.",
            concurrency = %self.concurrency,
            reached_limit = %self.reached_limit,
            had_back_pressure = %self.had_back_pressure,
            current_rtt = ?self.current_rtt,
            past_rtt = ?self.past_rtt,
            past_rtt_deviation = ?self.past_rtt_deviation,
        );
    }

    fn emit_metrics(&self) {
        // These are histograms, as they may have a number of different
        // values over each reporting interval, and each of those values
        // is valuable for diagnosis.
        histogram!("adaptive_concurrency_limit", self.concurrency as f64);
        let reached_limit = self.reached_limit.then(|| 1.0).unwrap_or_default();
        histogram!("adaptive_concurrency_reached_limit", reached_limit);
        let back_pressure = self.had_back_pressure.then(|| 1.0).unwrap_or_default();
        histogram!("adaptive_concurrency_back_pressure", back_pressure);
        histogram!("adaptive_concurrency_past_rtt_mean", self.past_rtt);
    }
}

#[derive(Debug)]
pub struct AdaptiveConcurrencyInFlight {
    pub in_flight: u64,
}

impl InternalEvent for AdaptiveConcurrencyInFlight {
    fn emit_metrics(&self) {
        histogram!("adaptive_concurrency_in_flight", self.in_flight as f64);
    }
}

#[derive(Debug)]
pub struct AdaptiveConcurrencyObservedRtt {
    pub rtt: Duration,
}

impl InternalEvent for AdaptiveConcurrencyObservedRtt {
    fn emit_metrics(&self) {
        histogram!("adaptive_concurrency_observed_rtt", self.rtt);
    }
}

#[derive(Debug)]
pub struct AdaptiveConcurrencyAveragedRtt {
    pub rtt: Duration,
}

impl InternalEvent for AdaptiveConcurrencyAveragedRtt {
    fn emit_metrics(&self) {
        histogram!("adaptive_concurrency_averaged_rtt", self.rtt);
    }
}
