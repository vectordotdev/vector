use serde::{Deserialize, Serialize};

mod global_options;
mod id;
mod log_schema;
pub mod proxy;

pub use global_options::GlobalOptions;
pub use id::ComponentKey;
pub use log_schema::{init_log_schema, log_schema, LogSchema};

use crate::schema;

pub const MEMORY_BUFFER_DEFAULT_MAX_EVENTS: usize =
    vector_buffers::config::memory_buffer_default_max_events();

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum DataType {
    Any,
    Log,
    Metric,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Input {
    ty: DataType,
    schema_requirement: schema::Requirement,
}

impl Input {
    pub fn data_type(&self) -> DataType {
        self.ty
    }

    pub fn schema_requirement(&self) -> &schema::Requirement {
        &self.schema_requirement
    }

    pub fn log() -> Self {
        Self {
            ty: DataType::Log,
            schema_requirement: schema::Requirement,
        }
    }

    pub fn metric() -> Self {
        Self {
            ty: DataType::Metric,
            schema_requirement: schema::Requirement,
        }
    }

    pub fn any() -> Self {
        Self {
            ty: DataType::Any,
            schema_requirement: schema::Requirement,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub port: Option<String>,
    pub ty: DataType,

    /// NOTE: schema definitions are currently ignored for non-log events. In the future, we can
    /// change `DataType` to keep track of schema data internally (e.g. keep one for the `Log` or
    /// `Metric` variants, and two for the `Any` variant, one for log events and one for metrics).
    /// Alternatively, we could update this field to `log_schema_definition` and add a new
    /// `metric_schema_definition` as well.
    ///
    /// The `None` variant of a schema definition has two distinct meanings for a source component
    /// versus a transform component:
    ///
    /// For *sources*, a `None` schema is identical to a `Some(Definition::undefined())` schema.
    ///
    /// For a *transform*, a `None` schema means the transform inherits the merged [`Definition`]
    /// of its inputs, without modifying the schema further.
    pub schema_definition: Option<schema::Definition>,
}

impl Output {
    /// Create a default `Output` of the given data type.
    ///
    /// A default output is one without a port identifier (i.e. not a named output) and the default
    /// output consumers will receive if they declare the component itself as an input.
    pub fn default(ty: DataType) -> Self {
        Self {
            port: None,
            ty,
            schema_definition: None,
        }
    }

    /// Set the schema definition for this output.
    pub fn with_schema_definition(mut self, schema_definition: schema::Definition) -> Self {
        self.schema_definition = Some(schema_definition);
        self
    }
}

impl<T: Into<String>> From<(T, DataType)> for Output {
    fn from((name, ty): (T, DataType)) -> Self {
        Self {
            port: Some(name.into()),
            ty,
            schema_definition: None,
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
