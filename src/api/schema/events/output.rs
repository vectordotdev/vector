use async_graphql::Union;

use super::{log::Log, metric::Metric, notification::EventNotification, trace::Trace};
use crate::api::tap::TapPayload;

#[derive(Union, Debug, Clone)]
#[allow(clippy::large_enum_variant)]
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
impl From<TapPayload> for Vec<OutputEventsPayload> {
    fn from(t: TapPayload) -> Self {
        match t {
            TapPayload::Log(output, log_array) => log_array
                .into_iter()
                .map(|log| OutputEventsPayload::Log(Log::new(output.clone(), log)))
                .collect(),
            TapPayload::Metric(output, metric_array) => metric_array
                .into_iter()
                .map(|metric| OutputEventsPayload::Metric(Metric::new(output.clone(), metric)))
                .collect(),
            TapPayload::Notification(notification) => {
                vec![OutputEventsPayload::Notification(EventNotification {
                    notification,
                })]
            }
            TapPayload::Trace(output, trace_array) => trace_array
                .into_iter()
                .map(|trace| OutputEventsPayload::Trace(Trace::new(output.clone(), trace)))
                .collect(),
        }
    }
}
