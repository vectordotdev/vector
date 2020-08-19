use super::InternalEvent;
use metrics::value;
use std::time::Duration;

#[derive(Debug)]
pub struct AutoConcurrencyLimit {
    pub concurrency: u64,
    pub reached_limit: bool,
    pub had_back_pressure: bool,
    pub current_rtt: Duration,
    pub past_rtt: Duration,
}

impl InternalEvent for AutoConcurrencyLimit {
    fn emit_logs(&self) {
        trace!(
            message = "Changed concurrency.",
            concurrency = %self.concurrency,
            reached_limit = %self.reached_limit,
            had_back_pressure = %self.had_back_pressure,
            current_rtt = ?self.current_rtt,
            past_rtt = ?self.past_rtt,
        );
    }

    fn emit_metrics(&self) {
        value!("auto_concurrency_limit", self.concurrency);
    }
}

#[derive(Debug)]
pub struct AutoConcurrencyInFlight {
    pub in_flight: u64,
}

impl InternalEvent for AutoConcurrencyInFlight {
    fn emit_metrics(&self) {
        value!("auto_concurrency_in_flight", self.in_flight);
    }
}

#[derive(Debug)]
pub struct AutoConcurrencyObservedRtt {
    pub rtt: Duration,
}

impl InternalEvent for AutoConcurrencyObservedRtt {
    fn emit_metrics(&self) {
        value!("auto_concurrency_observed_rtt", self.rtt);
    }
}

#[derive(Debug)]
pub struct AutoConcurrencyAveragedRtt {
    pub rtt: Duration,
}

impl InternalEvent for AutoConcurrencyAveragedRtt {
    fn emit_metrics(&self) {
        value!("auto_concurrency_averaged_rtt", self.rtt);
    }
}
