use crate::config::ComponentKey;
use async_graphql::{Enum, SimpleObject};

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
    /// Id of the component associated with the notification
    component_id: String,

    /// Id of the pipeline associated to the component
    pipeline_id: Option<String>,

    /// Event notification type
    notification: EventNotificationType,
}

impl EventNotification {
    pub fn new(component_key: ComponentKey, notification: EventNotificationType) -> Self {
        Self {
            component_id: component_key.id().to_string(),
            // the GraphQL SimpleObject forces to decompose at creation time
            pipeline_id: component_key.pipeline_str().map(Into::into),
            notification,
        }
    }
}
