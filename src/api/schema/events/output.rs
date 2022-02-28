use async_graphql::Union;

use super::{
    log::Log,
    metric::Metric,
    notification::{EventNotification, EventNotificationType},
    trace::Trace,
};
use crate::api::tap::{TapNotification, TapPayload};

#[derive(Union, Debug, Clone)]
/// An event or a notification
pub enum OutputEventsPayload {
    /// Log event
    Log(Log),

    /// Metric event
    Metric(Metric),

    // Notification
    Notification(EventNotification),

    /// Trace event
    Trace(Trace),
}

/// Convert an `api::TapPayload` to the equivalent GraphQL type.
impl From<TapPayload> for OutputEventsPayload {
    fn from(t: TapPayload) -> Self {
        match t {
            TapPayload::Log(output_id, ev) => Self::Log(Log::new(output_id, ev)),
            TapPayload::Metric(output_id, ev) => Self::Metric(Metric::new(output_id, ev)),
            TapPayload::Notification(component_key, n) => match n {
                TapNotification::Matched => Self::Notification(EventNotification::new(
                    component_key,
                    EventNotificationType::Matched,
                )),
                TapNotification::NotMatched => Self::Notification(EventNotification::new(
                    component_key,
                    EventNotificationType::NotMatched,
                )),
            },
            TapPayload::Trace(output_id, ev) => Self::Trace(Trace::new(output_id, ev)),
        }
    }
}
