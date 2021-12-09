use serde::{Deserialize, Serialize};

mod global_options;
mod id;
mod log_schema;
pub mod proxy;

pub use global_options::GlobalOptions;
pub use id::ComponentKey;
pub use log_schema::{init_log_schema, log_schema, LogSchema};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AcknowledgementsConfig {
    pub enabled: bool,
}

impl AcknowledgementsConfig {
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            enabled: self.enabled || other.enabled,
        }
    }
}

impl From<bool> for AcknowledgementsConfig {
    fn from(enabled: bool) -> Self {
        Self { enabled }
    }
}
