use std::time::Duration;

use metrics::Histogram;
use vector_lib::histogram;
use vector_lib::internal_event::MetricName;

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
        limit: Histogram = histogram!(MetricName::AdaptiveConcurrencyLimit),
        reached_limit: Histogram = histogram!(MetricName::AdaptiveConcurrencyReachedLimit),
        back_pressure: Histogram = histogram!(MetricName::AdaptiveConcurrencyBackPressure),
        past_rtt_mean: Histogram = histogram!(MetricName::AdaptiveConcurrencyPastRttMean),
    }

    fn emit(&self, data: AdaptiveConcurrencyLimitData) {
        self.limit.record(data.concurrency as f64);
        let reached_limit = if data.reached_limit { 1.0 } else { Default::default() };
        self.reached_limit.record(reached_limit);
        let back_pressure = if data.had_back_pressure { 1.0 } else { Default::default() };
        self.back_pressure.record(back_pressure);
        self.past_rtt_mean.record(data.past_rtt);
        // past_rtt_deviation is unrecorded
    }
}

registered_event! {
    AdaptiveConcurrencyInFlight => {
        in_flight: Histogram = histogram!(MetricName::AdaptiveConcurrencyInFlight),
    }

    fn emit(&self, in_flight: u64) {
        self.in_flight.record(in_flight as f64);
    }
}

registered_event! {
    AdaptiveConcurrencyObservedRtt => {
        observed_rtt: Histogram = histogram!(MetricName::AdaptiveConcurrencyObservedRtt),
    }

    fn emit(&self, rtt: Duration) {
        self.observed_rtt.record(rtt);
    }
}

registered_event! {
    AdaptiveConcurrencyAveragedRtt => {
        averaged_rtt: Histogram = histogram!(MetricName::AdaptiveConcurrencyAveragedRtt),
    }

    fn emit(&self, rtt: Duration) {
        self.averaged_rtt.record(rtt);
    }
}
