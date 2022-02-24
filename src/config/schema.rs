pub(crate) use crate::schema::{Definition, Id};

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, PartialEq, Copy, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
        }
    }
}

const fn default_enabled() -> bool {
    false
}
