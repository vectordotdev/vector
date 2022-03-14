use bitmask_enum::bitmask;
use serde::{Deserialize, Serialize};
use std::fmt;

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

// This enum should be kept alphabetically sorted as the bitmask value is used when
// sorting sources by data type in the GraphQL API.
#[bitmask(u8)]
pub enum DataType {
    Log,
    Metric,
    Trace,
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut t = Vec::new();
        self.contains(DataType::Log).then(|| t.push("Log"));
        self.contains(DataType::Metric).then(|| t.push("Metric"));
        self.contains(DataType::Trace).then(|| t.push("Trace"));
        f.write_str(&t.join(","))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Input {
    ty: DataType,
    log_schema_requirement: schema::Requirement,
}

impl Input {
    pub fn data_type(&self) -> DataType {
        self.ty
    }

    pub fn schema_requirement(&self) -> &schema::Requirement {
        &self.log_schema_requirement
    }

    pub fn new(ty: DataType) -> Self {
        Self {
            ty,
            log_schema_requirement: schema::Requirement::empty(),
        }
    }

    pub fn log() -> Self {
        Self {
            ty: DataType::Log,
            log_schema_requirement: schema::Requirement::empty(),
        }
    }

    pub fn metric() -> Self {
        Self {
            ty: DataType::Metric,
            log_schema_requirement: schema::Requirement::empty(),
        }
    }

    pub fn trace() -> Self {
        Self {
            ty: DataType::Trace,
            log_schema_requirement: schema::Requirement::empty(),
        }
    }

    pub fn all() -> Self {
        Self {
            ty: DataType::all(),
            log_schema_requirement: schema::Requirement::empty(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub port: Option<String>,
    pub ty: DataType,

    // NOTE: schema definitions are only implemented/supported for log-type events. There is no
    // inherent blocker to support other types as well, but it'll require additional work to add
    // the relevant schemas, and store them separately in this type.
    ///
    /// The `None` variant of a schema definition has two distinct meanings for a source component
    /// versus a transform component:
    ///
    /// For *sources*, a `None` schema is identical to a `Some(Definition::undefined())` schema.
    ///
    /// For a *transform*, a `None` schema means the transform inherits the merged [`Definition`]
    /// of its inputs, without modifying the schema further.
    pub log_schema_definition: Option<schema::Definition>,
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
            log_schema_definition: None,
        }
    }

    /// Set the schema definition for this output.
    #[must_use]
    pub fn with_schema_definition(mut self, schema_definition: schema::Definition) -> Self {
        self.log_schema_definition = Some(schema_definition);
        self
    }
}

impl<T: Into<String>> From<(T, DataType)> for Output {
    fn from((name, ty): (T, DataType)) -> Self {
        Self {
            port: Some(name.into()),
            ty,
            log_schema_definition: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AcknowledgementsConfig {
    enabled: Option<bool>,
}

impl AcknowledgementsConfig {
    #[must_use]
    pub fn merge_default(&self, other: &Self) -> Self {
        let enabled = self.enabled.or(other.enabled);
        Self { enabled }
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
