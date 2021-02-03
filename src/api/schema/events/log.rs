use crate::event;
use async_graphql::Object;

#[derive(Debug)]
/// A log event
pub struct LogEvent(event::LogEvent);

#[Object]
impl LogEvent {
    /// Get the log event as a string
    async fn string(&self) -> &str {
        "test"
    }
}
