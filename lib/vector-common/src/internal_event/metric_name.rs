use strum::{AsRefStr, Display, EnumIter};

/// Canonical list of all per-component internal metric names emitted by Vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, AsRefStr, EnumIter)]
#[strum(serialize_all = "snake_case")]
pub enum MetricName {
    ComponentReceivedEventsTotal,
    ComponentReceivedEventBytesTotal,
    ComponentReceivedEventsCount,
    ComponentReceivedBytesTotal,
    ComponentSentEventsTotal,
    ComponentSentEventBytesTotal,
    ComponentSentBytesTotal,
    ComponentDiscardedEventsTotal,
    ComponentErrorsTotal,
    ComponentTimedOutEventsTotal,
    ComponentTimedOutRequestsTotal,
    BufferMaxSizeEvents,
    BufferMaxEventSize,
    BufferMaxSizeBytes,
    BufferMaxByteSize,
    BufferReceivedEventsTotal,
    BufferReceivedBytesTotal,
    BufferSentEventsTotal,
    BufferSentBytesTotal,
    BufferDiscardedEventsTotal,
    BufferDiscardedBytesTotal,
    BufferErrorsTotal,
    BufferSendDurationSeconds,
    BufferEvents,
    BufferSizeEvents,
    BufferSizeBytes,
    BufferByteSize,
    ComponentLatencySeconds,
    ComponentLatencyMeanSeconds,
    SourceLagTimeSeconds,
    SourceSendLatencySeconds,
    SourceSendBatchLatencySeconds,
}

impl MetricName {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ComponentReceivedEventsTotal => "component_received_events_total",
            Self::ComponentReceivedEventBytesTotal => "component_received_event_bytes_total",
            Self::ComponentReceivedEventsCount => "component_received_events_count",
            Self::ComponentReceivedBytesTotal => "component_received_bytes_total",
            Self::ComponentSentEventsTotal => "component_sent_events_total",
            Self::ComponentSentEventBytesTotal => "component_sent_event_bytes_total",
            Self::ComponentSentBytesTotal => "component_sent_bytes_total",
            Self::ComponentDiscardedEventsTotal => "component_discarded_events_total",
            Self::ComponentErrorsTotal => "component_errors_total",
            Self::ComponentTimedOutEventsTotal => "component_timed_out_events_total",
            Self::ComponentTimedOutRequestsTotal => "component_timed_out_requests_total",
            Self::BufferMaxSizeEvents => "buffer_max_size_events",
            Self::BufferMaxEventSize => "buffer_max_event_size",
            Self::BufferMaxSizeBytes => "buffer_max_size_bytes",
            Self::BufferMaxByteSize => "buffer_max_byte_size",
            Self::BufferReceivedEventsTotal => "buffer_received_events_total",
            Self::BufferReceivedBytesTotal => "buffer_received_bytes_total",
            Self::BufferSentEventsTotal => "buffer_sent_events_total",
            Self::BufferSentBytesTotal => "buffer_sent_bytes_total",
            Self::BufferDiscardedEventsTotal => "buffer_discarded_events_total",
            Self::BufferDiscardedBytesTotal => "buffer_discarded_bytes_total",
            Self::BufferErrorsTotal => "buffer_errors_total",
            Self::BufferSendDurationSeconds => "buffer_send_duration_seconds",
            Self::BufferEvents => "buffer_events",
            Self::BufferSizeEvents => "buffer_size_events",
            Self::BufferSizeBytes => "buffer_size_bytes",
            Self::BufferByteSize => "buffer_byte_size",
            Self::ComponentLatencySeconds => "component_latency_seconds",
            Self::ComponentLatencyMeanSeconds => "component_latency_mean_seconds",
            Self::SourceLagTimeSeconds => "source_lag_time_seconds",
            Self::SourceSendLatencySeconds => "source_send_latency_seconds",
            Self::SourceSendBatchLatencySeconds => "source_send_batch_latency_seconds",
        }
    }
}
