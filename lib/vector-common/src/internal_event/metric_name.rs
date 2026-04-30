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
        }
    }
}
