//! Loki sink
//!
//! This sink provides downstream support for `Loki` via
//! the v1 http json endpoint.
//!
//! <https://github.com/grafana/loki/tree/v1.6.1/docs>
//!
//! This sink uses `PartitionBatching` to partition events
//! by streams. There must be at least one valid set of labels.
//!
//! If an event produces no labels, this can happen if the template
//! does not match, we will add a default label `{agent="vector"}`.
mod config;
mod event;
mod healthcheck;
#[cfg(feature = "loki-integration-tests")]
#[cfg(test)]
mod integration_tests;
mod service;
mod sink;
#[cfg(test)]
mod tests;

#[cfg(feature = "loki-benches")]
pub use self::config::valid_label_name;
pub use self::config::{LokiConfig, OutOfOrderAction};
