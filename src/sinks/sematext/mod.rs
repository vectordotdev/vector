mod logs;
mod metrics;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(self) enum Region {
    Us,
    Eu,
}
