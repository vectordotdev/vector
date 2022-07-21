use serde::{Deserialize, Serialize};
use vector_core::config::LogNamespace;

pub(crate) use crate::schema::Definition;

#[derive(Debug, Deserialize, Serialize, PartialEq, Copy, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    #[serde(default = "default_sink_requirements_enabled")]
    pub sink_requirements: bool,

    pub log_namespace: Option<bool>,
}

impl Options {
    /// Gets the value of the globally configured log namespace, or the default if it wasn't set.
    pub fn log_namespace(self) -> LogNamespace {
        self.log_namespace
            .map_or(LogNamespace::Legacy, |use_vector_namespace| {
                use_vector_namespace.into()
            })
    }
}

impl Default for Options {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            sink_requirements: default_sink_requirements_enabled(),
            log_namespace: None,
        }
    }
}

const fn default_enabled() -> bool {
    false
}

const fn default_sink_requirements_enabled() -> bool {
    false
}
