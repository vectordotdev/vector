use crate::internal_events::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct BufferEventDropped;

impl InternalEvent for BufferEventDropped {
    fn emit_logs(&self) {
        debug!(
            message = "Shedding load; dropping event.",
            rate_limit_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("events_dropped", 1,);
    }
}
