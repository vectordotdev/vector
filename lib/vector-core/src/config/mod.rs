use serde::{Deserialize, Serialize};

mod global_options;
mod id;
mod log_schema;
pub mod proxy;

pub use global_options::GlobalOptions;
pub use id::ComponentKey;
pub use log_schema::{init_log_schema, log_schema, LogSchema};

pub const MEMORY_BUFFER_DEFAULT_MAX_EVENTS: usize =
    vector_buffers::config::memory_buffer_default_max_events();

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum DataType {
    Any,
    Log,
    Metric,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub port: Option<String>,
    pub ty: DataType,
}

impl Output {
    /// Create a default `Output` of the given data type.
    ///
    /// A default output is one without a port identifier (i.e. not a named output) and the default
    /// output consumers will receive if they declare the component itself as an input.
    pub fn default(ty: DataType) -> Self {
        Self { port: None, ty }
    }

    /// Check if the `Output` is a default output
    pub fn is_default(&self) -> bool {
        self.port.is_none()
    }
}

impl<T: Into<String>> From<(T, DataType)> for Output {
    fn from((name, ty): (T, DataType)) -> Self {
        Self {
            port: Some(name.into()),
            ty,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AcknowledgementsConfig {
    enabled: Option<bool>,
}

impl AcknowledgementsConfig {
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            enabled: other.enabled.or(self.enabled),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled.unwrap_or(false)
    }
}

impl From<Option<bool>> for AcknowledgementsConfig {
    fn from(enabled: Option<bool>) -> Self {
        Self { enabled }
    }
}

impl From<bool> for AcknowledgementsConfig {
    fn from(enabled: bool) -> Self {
        Some(enabled).into()
    }
}
