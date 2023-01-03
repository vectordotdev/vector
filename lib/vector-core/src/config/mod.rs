use std::{fmt, num::NonZeroUsize};

use bitmask_enum::bitmask;
use bytes::Bytes;
use chrono::{DateTime, Utc};

mod global_options;
mod log_schema;
pub mod proxy;

use crate::event::LogEvent;
pub use global_options::GlobalOptions;
pub use log_schema::{init_log_schema, log_schema, LogSchema};
use lookup::{lookup_v2::ValuePath, path, PathPrefix};
use serde::{Deserialize, Serialize};
use value::Value;
pub use vector_common::config::ComponentKey;
use vector_config::configurable_component;

use crate::schema;

pub const MEMORY_BUFFER_DEFAULT_MAX_EVENTS: NonZeroUsize =
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

    /// Set the schema requirement for this output.
    #[must_use]
    pub fn with_schema_requirement(mut self, schema_requirement: schema::Requirement) -> Self {
        self.log_schema_requirement = schema_requirement;
        self
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
    /// For *sources*, a `None` schema is identical to a `Some(Definition::source_default())`.
    ///
    /// For a *transform*, a schema [`Definition`] is required if `Datatype` is Log.
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

    /// Set the schema definition for this `Output`.
    #[must_use]
    pub fn with_schema_definition(mut self, schema_definition: schema::Definition) -> Self {
        self.log_schema_definition = Some(schema_definition);
        self
    }

    /// Set the port name for this `Output`.
    #[must_use]
    pub fn with_port(mut self, name: impl Into<String>) -> Self {
        self.port = Some(name.into());
        self
    }
}

/// Source-specific end-to-end acknowledgements configuration.
///
/// This type exists solely to provide a source-specific description of the `acknowledgements`
/// setting, as it is deprecated, and we still need to maintain a way to expose it in the
/// documentation before it's removed while also making sure people know it shouldn't be used.
#[configurable_component]
#[configurable(title = "Controls how acknowledgements are handled by this source.")]
#[configurable(
    description = "This setting is **deprecated** in favor of enabling `acknowledgements` at the [global][global_acks] or sink level. \
Enabling or disabling acknowledgements at the source level has **no effect** on acknowledgement behavior.

See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
[e2e_acks]: https://vector.dev/docs/about/under-the-hood/architecture/end-to-end-acknowledgements/"
)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SourceAcknowledgementsConfig {
    /// Whether or not end-to-end acknowledgements are enabled for this source.
    enabled: Option<bool>,
}

impl SourceAcknowledgementsConfig {
    pub const DEFAULT: Self = Self { enabled: None };

    #[must_use]
    pub fn merge_default(&self, other: &Self) -> Self {
        let enabled = self.enabled.or(other.enabled);
        Self { enabled }
    }

    pub fn enabled(&self) -> bool {
        self.enabled.unwrap_or(false)
    }
}

impl From<Option<bool>> for SourceAcknowledgementsConfig {
    fn from(enabled: Option<bool>) -> Self {
        Self { enabled }
    }
}

impl From<bool> for SourceAcknowledgementsConfig {
    fn from(enabled: bool) -> Self {
        Some(enabled).into()
    }
}

impl From<SourceAcknowledgementsConfig> for AcknowledgementsConfig {
    fn from(config: SourceAcknowledgementsConfig) -> Self {
        Self {
            enabled: config.enabled,
        }
    }
}

/// End-to-end acknowledgements configuration.
#[configurable_component]
#[configurable(title = "Controls how acknowledgements are handled for this sink.")]
#[configurable(
    description = "See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

[e2e_acks]: https://vector.dev/docs/about/under-the-hood/architecture/end-to-end-acknowledgements/"
)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AcknowledgementsConfig {
    /// Whether or not end-to-end acknowledgements are enabled.
    ///
    /// When enabled for a sink, any source connected to that sink, where the source supports
    /// end-to-end acknowledgements as well, will wait for events to be acknowledged by the sink
    /// before acknowledging them at the source.
    ///
    /// Enabling or disabling acknowledgements at the sink level takes precedence over any global
    /// [`acknowledgements`][global_acks] configuration.
    ///
    /// [global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
    enabled: Option<bool>,
}

impl AcknowledgementsConfig {
    pub const DEFAULT: Self = Self { enabled: None };

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

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize, PartialOrd, Ord, Eq)]
pub enum LogNamespace {
    /// Vector native namespacing
    ///
    /// Deserialized data is placed in the root of the event.
    /// Extra data is placed in "event metadata"
    Vector,

    /// This is the legacy namespacing.
    ///
    /// All data is set in the root of the event. Since this can lead
    /// to collisions, deserialized data has priority over metadata
    Legacy,
}

/// The user-facing config for log namespace is a bool (enabling or disabling the "Log Namespacing" feature).
/// Internally, this is converted to a enum.
impl From<bool> for LogNamespace {
    fn from(x: bool) -> Self {
        if x {
            LogNamespace::Vector
        } else {
            LogNamespace::Legacy
        }
    }
}

impl Default for LogNamespace {
    fn default() -> Self {
        Self::Legacy
    }
}

/// A shortcut to specify no `LegacyKey` should be used (since otherwise a turbofish would be required)
pub const NO_LEGACY_KEY: Option<LegacyKey<&'static str>> = None;

pub enum LegacyKey<T> {
    /// Always insert the data, even if the field previously existed
    Overwrite(T),
    /// Only insert the data if the field is currently empty
    InsertIfEmpty(T),
}

impl LogNamespace {
    /// Vector: This is added to "event metadata", nested under the source name.
    ///
    /// Legacy: This is stored on the event root, only if a field with that name doesn't already exist.
    pub fn insert_source_metadata<'a>(
        &self,
        source_name: &'a str,
        log: &mut LogEvent,
        legacy_key: Option<LegacyKey<impl ValuePath<'a>>>,
        metadata_key: impl ValuePath<'a>,
        value: impl Into<Value>,
    ) {
        match self {
            LogNamespace::Vector => {
                log.metadata_mut()
                    .value_mut()
                    .insert(path!(source_name).concat(metadata_key), value);
            }
            LogNamespace::Legacy => match legacy_key {
                None => { /* don't insert */ }
                Some(LegacyKey::Overwrite(key)) => {
                    log.insert((PathPrefix::Event, key), value);
                }
                Some(LegacyKey::InsertIfEmpty(key)) => {
                    log.try_insert((PathPrefix::Event, key), value);
                }
            },
        }
    }

    /// Vector: This is retrieved from the "event metadata", nested under the source name.
    ///
    /// Legacy: This is retrieved from the event.
    pub fn get_source_metadata<'a, 'b>(
        &self,
        source_name: &'a str,
        log: &'b LogEvent,
        legacy_key: impl ValuePath<'a>,
        metadata_key: impl ValuePath<'a>,
    ) -> Option<&'b Value> {
        match self {
            LogNamespace::Vector => log
                .metadata()
                .value()
                .get(path!(source_name).concat(metadata_key)),
            LogNamespace::Legacy => log.get((PathPrefix::Event, legacy_key)),
        }
    }

    /// Vector: The `ingest_timestamp`, and `source_type` fields are added to "event metadata", nested
    /// under the name "vector". This data will be marked as read-only in VRL.
    ///
    /// Legacy: The values of `source_type_key`, and `timestamp_key` are stored as keys on the event root,
    /// only if a field with that name doesn't already exist.
    pub fn insert_standard_vector_source_metadata(
        &self,
        log: &mut LogEvent,
        source_name: &'static str,
        now: DateTime<Utc>,
    ) {
        self.insert_vector_metadata(
            log,
            path!(log_schema().source_type_key()),
            path!("source_type"),
            Bytes::from_static(source_name.as_bytes()),
        );
        self.insert_vector_metadata(
            log,
            path!(log_schema().timestamp_key()),
            path!("ingest_timestamp"),
            now,
        );
    }

    /// Vector: This is added to the "event metadata", nested under the name "vector". This data
    /// will be marked as read-only in VRL.
    ///
    /// Legacy: This is stored on the event root, only if a field with that name doesn't already exist.
    pub fn insert_vector_metadata<'a>(
        &self,
        log: &mut LogEvent,
        legacy_key: impl ValuePath<'a>,
        metadata_key: impl ValuePath<'a>,
        value: impl Into<Value>,
    ) {
        match self {
            LogNamespace::Vector => {
                log.metadata_mut()
                    .value_mut()
                    .insert(path!("vector").concat(metadata_key), value);
            }
            LogNamespace::Legacy => {
                log.try_insert((PathPrefix::Event, legacy_key), value);
            }
        }
    }

    /// Vector: This is retrieved from the "event metadata", nested under the name "vector".
    ///
    /// Legacy: This is retrieved from the event.
    pub fn get_vector_metadata<'a, 'b>(
        &self,
        log: &'b LogEvent,
        legacy_key: impl ValuePath<'a>,
        metadata_key: impl ValuePath<'a>,
    ) -> Option<&'b Value> {
        match self {
            LogNamespace::Vector => log
                .metadata()
                .value()
                .get(path!("vector").concat(metadata_key)),
            LogNamespace::Legacy => log.get((PathPrefix::Event, legacy_key)),
        }
    }

    pub fn new_log_from_data(&self, value: impl Into<Value>) -> LogEvent {
        match self {
            LogNamespace::Vector | LogNamespace::Legacy => LogEvent::from(value.into()),
        }
    }

    // combine a global (self) and local value to get the actual namespace
    #[must_use]
    pub fn merge(&self, override_value: Option<impl Into<LogNamespace>>) -> LogNamespace {
        override_value.map_or(*self, Into::into)
    }
}
