use async_graphql::Union;

use super::{
    log::Log,
    metric::Metric,
    notification::{EventNotification, EventNotificationType},
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
}

/// Convert an `api::TapPayload` to the equivalent GraphQL type.
impl From<TapPayload> for OutputEventsPayload {
    fn from(t: TapPayload) -> Self {
        match t {
            TapPayload::Log(output_id, ev) => Self::Log(Log::new(output_id, ev)),
            TapPayload::Metric(output_id, ev) => Self::Metric(Metric::new(output_id, ev)),
            TapPayload::Notification(pattern, n) => match n {
                TapNotification::Matched => Self::Notification(EventNotification::new(
                    pattern,
                    EventNotificationType::Matched,
                )),
                TapNotification::NotMatched => Self::Notification(EventNotification::new(
                    pattern,
                    EventNotificationType::NotMatched,
                )),
                TapNotification::InvalidInputPatternMatch(invalid_matches) => {
                    Self::Notification(EventNotification::new_with_invalid_matches(
                        pattern,
                        EventNotificationType::InvalidInputPatternMatch,
                        invalid_matches,
                    ))
                }
                TapNotification::InvalidOutputPatternMatch(invalid_matches) => {
                    Self::Notification(EventNotification::new_with_invalid_matches(
                        pattern,
                        EventNotificationType::InvalidOutputPatternMatch,
                        invalid_matches,
                    ))
                }
            },
        }
    }
}
