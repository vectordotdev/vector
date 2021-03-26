use super::{
    log::Log,
    notification::{EventNotification, EventNotificationType},
};
use crate::api::tap::{TapNotification, TapPayload};

use async_graphql::Union;

#[derive(Union, Debug)]
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
            TapPayload::Log(name, ev) => Self::Log(Log::new(&name, ev)),
            TapPayload::Notification(name, n) => match n {
                TapNotification::Matched => Self::Notification(EventNotification::new(
                    &name,
                    EventNotificationType::Matched,
                )),
                TapNotification::NotMatched => Self::Notification(EventNotification::new(
                    &name,
                    EventNotificationType::NotMatched,
                )),
            },
            _ => unreachable!("TODO: implement metrics"),
        }
    }
}
