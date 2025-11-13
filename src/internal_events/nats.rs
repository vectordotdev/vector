use metrics::counter;
use tracing::info;
use vector_lib::internal_event::InternalEvent;

#[derive(Debug)]
pub struct NatsSlowConsumerEventReceived {
    pub subscription_id: u64,
    pub component_id: String,
}

impl InternalEvent for NatsSlowConsumerEventReceived {
    fn emit(self) {
        info!(
            message = "NATS slow consumer for subscription.",
            subscription_id = %self.subscription_id,
            component_id = %self.component_id,
            internal_log_rate_secs = 10,
        );
        counter!(
            "nats_slow_consumer_events_total",
            "component_id" => self.component_id,
            "subscription_id" => self.subscription_id.to_string(),
        )
        .increment(1);
    }
}
