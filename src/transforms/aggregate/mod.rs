mod config;
mod event_time;
mod transform;

pub use config::{AggregateConfig, AggregationMode, TimeSource};
pub use transform::Aggregate;

#[cfg(test)]
mod tests;
