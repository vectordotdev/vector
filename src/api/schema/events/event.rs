use super::log_event::LogEvent;
use crate::api::tap::{TapNotification, TapPayload};
use async_graphql::{Enum, SimpleObject, Union};

#[derive(Enum, Copy, Clone, PartialEq, Eq)]
pub enum EventEncodingType {
    Json,
    Yaml,
}

#[derive(Enum, Debug, Copy, Clone, PartialEq, Eq)]
/// Event notification type
pub enum EventNotificationType {
    /// A component was found that matched the provided pattern
    Matched,
    /// There isn't currently a component that matches this pattern
    NotMatched,
}

#[derive(Debug, SimpleObject)]
/// A notification regarding events observation
pub struct EventNotification {
    /// Name of the component associated with the notification
    component_name: String,

    /// Event notification type
    notification: EventNotificationType,
}

impl EventNotification {
    pub fn new(component_name: &str, notification: EventNotificationType) -> Self {
        Self {
            component_name: component_name.to_string(),
            notification,
        }
    }
}

#[derive(Union, Debug)]
pub enum Event {
    /// Log event
    LogEvent(LogEvent),

    // Notification
    Notification(EventNotification),
}

/// Convert an `api::TapPayload` to the equivalent GraphQL type.
impl From<TapPayload> for Event {
    fn from(t: TapPayload) -> Self {
        match t {
            TapPayload::LogEvent(name, ev) => Self::LogEvent(LogEvent::new(&name, ev)),
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
