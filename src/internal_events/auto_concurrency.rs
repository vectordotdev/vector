use super::InternalEvent;
use metrics::value;
use std::time::Duration;

#[derive(Debug)]
pub struct AutoConcurrencyLimit {
    pub concurrency: u64,
}

impl InternalEvent for AutoConcurrencyLimit {
    fn emit_logs(&self) {
        trace!(message = "changed concurrency.", concurrency = %self.concurrency);
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
