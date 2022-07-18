pub mod logs;
pub mod metrics;

pub use self::{logs::SematextLogsConfig, metrics::SematextMetricsConfig};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(self) enum Region {
    Us,
    Eu,
}
