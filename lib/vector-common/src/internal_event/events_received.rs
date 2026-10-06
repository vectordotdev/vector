use metrics::{Counter, Histogram};

use crate::{counter, histogram};
use tracing::trace;

use super::{CountByteSize, CounterName, HistogramName};

crate::registered_event!(
    EventsReceived => {
        events_count: Histogram = histogram!(HistogramName::ComponentReceivedEventsCount),
        events: Counter = counter!(CounterName::ComponentReceivedEventsTotal),
        event_bytes: Counter = counter!(CounterName::ComponentReceivedEventBytesTotal),
    }

    fn emit(&self, data: CountByteSize) {
        let CountByteSize(count, byte_size) = data;

        trace!(message = "Events received.", count = %count, byte_size = %byte_size);

        #[allow(clippy::cast_precision_loss)]
        self.events_count.record(count as f64);
        self.events.increment(count as u64);
        self.event_bytes.increment(byte_size.get() as u64);
    }
);
