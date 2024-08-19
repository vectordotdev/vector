use std::time::Duration;

use metrics::{histogram, Histogram};

#[derive(Clone, Copy)]
pub struct AdaptiveConcurrencyLimitData {
    pub concurrency: u64,
    pub reached_limit: bool,
    pub had_back_pressure: bool,
    pub current_rtt: Option<Duration>,
    pub past_rtt: Duration,
    pub past_rtt_deviation: Duration,
}

registered_event! {
    AdaptiveConcurrencyLimit => {
        // These are histograms, as they may have a number of different
        // values over each reporting interval, and each of those values
        // is valuable for diagnosis.
        limit: Histogram = histogram!("adaptive_concurrency_limit"),
        reached_limit: Histogram = histogram!("adaptive_concurrency_reached_limit"),
        back_pressure: Histogram = histogram!("adaptive_concurrency_back_pressure"),
        past_rtt_mean: Histogram = histogram!("adaptive_concurrency_past_rtt_mean"),
    }

    fn emit(&self, data: AdaptiveConcurrencyLimitData) {
        self.limit.record(data.concurrency as f64);
        let reached_limit = data.reached_limit.then_some(1.0).unwrap_or_default();
        self.reached_limit.record(reached_limit);
        let back_pressure = data.had_back_pressure.then_some(1.0).unwrap_or_default();
        self.back_pressure.record(back_pressure);
        self.past_rtt_mean.record(data.past_rtt);
        // past_rtt_deviation is unrecorded
    }
}

registered_event! {
    AdaptiveConcurrencyInFlight => {
        in_flight: Histogram = histogram!("adaptive_concurrency_in_flight"),
    }

    fn emit(&self, in_flight: u64) {
        self.in_flight.record(in_flight as f64);
    }
}

registered_event! {
    AdaptiveConcurrencyObservedRtt => {
        observed_rtt: Histogram = histogram!("adaptive_concurrency_observed_rtt"),
    }

    fn emit(&self, rtt: Duration) {
        self.observed_rtt.record(rtt);
    }
}

registered_event! {
    AdaptiveConcurrencyAveragedRtt => {
        averaged_rtt: Histogram = histogram!("adaptive_concurrency_averaged_rtt"),
    }

    fn emit(&self, rtt: Duration) {
        self.averaged_rtt.record(rtt);
    }
}
