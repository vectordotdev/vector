mod generic;

use super::{ProcessedBytesTotal, ProcessedEventsTotal};
use async_graphql::Interface;

#[derive(Debug, Clone, Interface)]
#[graphql(
    field(name = "processed_events_total", type = "Option<ProcessedEventsTotal>"),
    field(name = "processed_bytes_total", type = "Option<ProcessedBytesTotal>")
)]
pub enum SourceMetrics {
    GenericSource(generic::GenericSource),
}
