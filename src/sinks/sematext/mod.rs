pub mod logs;
pub mod metrics;

pub use self::{logs::SematextLogsConfig, metrics::SematextMetricsConfig};

use vector_lib::configurable::configurable_component;

/// The Sematext region to send data to.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    /// United States
    Us,

    /// Europe
    Eu,
}

const fn default_region() -> Region {
    Region::Us
}
