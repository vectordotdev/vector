mod config;
mod transform;

pub use config::{AggregateConfig, AggregationMode};
pub use transform::Aggregate;

#[cfg(test)]
mod tests;
