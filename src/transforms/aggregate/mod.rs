mod config;
mod event_time;
mod transform;

pub use config::{AggregateConfig, AggregationMode, EventTimeConfig, LateEvents, MissingTimestamp};
pub use transform::Aggregate;

#[cfg(test)]
mod tests;
