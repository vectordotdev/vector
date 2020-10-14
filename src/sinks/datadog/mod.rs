use serde::{Deserialize, Serialize};

pub mod logs;
pub mod metrics;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    Us,
    Eu,
}
