use async_graphql::{Object, Union};

use crate::api::schema::events::log::Log;
use crate::api::schema::events::metric::Metric;
use crate::api::schema::events::trace::Trace;
use vector_lib::tap::controller::TapPayload;
use vector_lib::tap::notification::Notification;

/// This wrapper struct hoists `message` up from [`Notification`] for a more
/// natural querying experience. While ideally [`Notification`] would be a
/// GraphQL interface with a common `message` field, an interface cannot be
/// directly nested into the union of [`super::OutputEventsPayload`].
///
/// The GraphQL specification forbids such a nesting:
/// <http://spec.graphql.org/October2021/#sel-HAHdfFDABABkG3_I>
#[derive(Debug, Clone)]
pub struct EventNotification {
    pub notification: Notification,
}

#[Object]
/// A notification regarding events observation
impl EventNotification {
    /// Notification details
    async fn notification(&self) -> &Notification {
        &self.notification
    }

    /// The human-readable message associated with the notification
    async fn message(&self) -> &str {
        self.notification.as_str()
    }
}

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
pub(crate) fn from_tap_payload_to_output_events(t: TapPayload) -> Vec<OutputEventsPayload> {
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
