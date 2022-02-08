use async_graphql::Union;

use super::{
    log::Log,
    notification::{EventNotification, EventNotificationType},
};
use crate::api::tap::{TapNotification, TapPayload};

#[derive(Union, Debug, Clone)]
/// An event or a notification
pub enum OutputEventsPayload {
    /// Log event
    Log(Log),

    // Notification
    Notification(EventNotification),
}

/// Convert an `api::TapPayload` to the equivalent GraphQL type.
impl From<TapPayload> for OutputEventsPayload {
    fn from(t: TapPayload) -> Self {
        match t {
            TapPayload::Log(output_id, ev) => Self::Log(Log::new(output_id, ev)),
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
            _ => unreachable!("TODO: implement metrics"),
        }
    }
}
