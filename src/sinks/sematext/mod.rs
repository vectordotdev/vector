pub mod logs;
pub mod metrics;

use vector_config::configurable_component;

pub use self::{logs::SematextLogsConfig, metrics::SematextMetricsConfig};

/// Sematext region.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    /// US region.
    Us,

    /// EU region.
    Eu,
}
