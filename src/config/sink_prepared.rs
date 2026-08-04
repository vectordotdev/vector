//! Prepared sink infrastructure for the sinks-only lifecycle architecture.
//!
//! This module implements a split between the serde/typetag-facing `SinkConfig` role
//! and the topology-facing runtime role. It introduces:
//!
//! - `PreparedSink`: a generic, implementor-facing trait. A migrated sink implements
//!   this and works entirely with its own concrete validated type `T` — it returns `T`
//!   from `validate_structure` and receives `&T` back in `build_prepared`. No `Any` in sight.
//! - `PreparedSinkErased`: the object-safe boundary the framework actually crosses.
//!   A blanket impl derives it from any `PreparedSink`, performing the `Box<dyn Any>`
//!   erasure and downcast automatically.
//! - `PreparedSinkEntry`: pairs a sink's raw config with its erased prepared state.
//!
//! `PreparedSink::validate_structure` is the same phase as the RFC's
//! `SinkConfig::validate_structure` (pure structural validation owned by config
//! compilation), but it retains the validated state so `build_prepared` does not
//! redo it.
//!
//! Design goals:
//! - Validation returns retained values passed to build
//! - Validation is pure: no filesystem/network/credentials/spawn/await
//! - Build may do environment-dependent construction but must not redo pure validation
//! - Preserve raw config serialization, reload diffing, and metadata access
//! - Sink implementors never touch `Any`; the framework owns the erasure

use std::any::Any;

use async_trait::async_trait;
use vector_lib::sink::VectorSink;

use super::SinkContext;
use crate::sinks::Healthcheck;

/// Generic prepared-sink trait, implemented by migrated sinks.
///
/// The implementor works entirely with their concrete validated type `Self::Prepared`:
///
/// ```ignore
/// #[async_trait]
/// impl PreparedSink for MySinkConfig {
///     type Prepared = ValidatedMySink;
///
///     fn validate_structure(&self) -> crate::Result<Self::Prepared> {
///         ValidatedMySink::from_config(self)
///     }
///
///     async fn build_prepared(
///         &self,
///         prepared: &Self::Prepared,
///         cx: SinkContext,
///     ) -> crate::Result<(VectorSink, Healthcheck)> {
///         prepared.build(cx).await
///     }
/// }
/// ```
///
/// The framework automatically erases `Self::Prepared` to `Box<dyn Any>` and restores
/// it at build time via `PreparedSinkErased`, so no `Any` appears in implementor code.
#[async_trait]
pub trait PreparedSink {
    /// The concrete validated state produced by `validate_structure` and consumed by `build_prepared`.
    type Prepared: Send + Sync + 'static;

    /// Performs pure structural validation, returning the validated state.
    ///
    /// This is the same phase as the RFC's `SinkConfig::validate_structure`, but it
    /// retains the validated state so `build_prepared` does not redo it.
    ///
    /// # Purity Guarantees
    ///
    /// This method must be pure: no filesystem access, no network operations, no
    /// credential resolution, no spawning, and no async/await. All such environment-
    /// dependent operations belong in `build_prepared`.
    fn validate_structure(&self) -> crate::Result<Self::Prepared>;

    /// Builds the sink from the validated state, without redoing pure validation.
    ///
    /// May perform environment-dependent construction (HTTP clients, schema fetching, etc.).
    async fn build_prepared(
        &self,
        prepared: &Self::Prepared,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)>;
}

/// Object-safe erased boundary used by the framework.
///
/// This is what actually crosses the `Box<dyn SinkConfig>` boundary. It is derived
/// automatically from any `PreparedSink` by the blanket impl below, which owns the
/// `Box<dyn Any>` erasure and the downcast back to the concrete validated type.
#[async_trait]
pub trait PreparedSinkErased {
    /// Erases the validated state into a `Box<dyn Any>`.
    fn validate_structure_erased(&self) -> crate::Result<Box<dyn Any + Send + Sync>>;

    /// Restores the validated state from `&dyn Any` and builds the sink.
    async fn build_prepared_erased(
        &self,
        prepared: &(dyn Any + Send + Sync),
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)>;
}

#[async_trait]
impl<T> PreparedSinkErased for T
where
    T: PreparedSink + Send + Sync + 'static,
{
    fn validate_structure_erased(&self) -> crate::Result<Box<dyn Any + Send + Sync>> {
        Ok(Box::new(self.validate_structure()?))
    }

    async fn build_prepared_erased(
        &self,
        prepared: &(dyn Any + Send + Sync),
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let prepared = prepared
            .downcast_ref::<T::Prepared>()
            .expect("prepared state type mismatch");
        self.build_prepared(prepared, cx).await
    }
}

/// A sink that has been validated/prepared at config-compile time.
///
/// Pairs the original raw config (preserved for serialization, reload diffing, and
/// metadata access) with the erased prepared state produced by `SinkConfig::prepare`.
/// The prepared state is downcast by the concrete sink's `SinkConfig::build_prepared`.
pub struct PreparedSinkEntry {
    /// The raw sink config, preserved for serialization and diffing.
    raw: super::BoxedSink,
    /// The erased prepared state from `SinkConfig::prepare`.
    prepared: Box<dyn Any + Send + Sync>,
}

impl core::fmt::Debug for PreparedSinkEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PreparedSinkEntry")
            .field("raw", &self.raw)
            .field(
                "prepared_type",
                &std::any::type_name_of_val(self.prepared.as_ref()),
            )
            .finish()
    }
}

impl PreparedSinkEntry {
    /// Creates a new prepared sink entry from a prepared state and its raw config.
    pub fn new(raw: super::BoxedSink, prepared: Box<dyn Any + Send + Sync>) -> Self {
        Self { raw, prepared }
    }

    /// Creates a legacy entry that wraps the raw config without native preparation.
    ///
    /// The prepared state is empty; `SinkConfig::build_prepared`'s default ignores it
    /// and builds directly from the raw config.
    pub fn legacy(raw: super::BoxedSink) -> Self {
        Self {
            raw,
            prepared: Box::new(()),
        }
    }

    /// Returns a reference to the raw sink config.
    pub fn raw(&self) -> &super::BoxedSink {
        &self.raw
    }

    /// Returns the erased prepared state.
    pub fn prepared(&self) -> &dyn Any {
        self.prepared.as_ref()
    }

    /// Builds the sink, dispatching through the erased prepared boundary when the
    /// sink has been migrated, or falling back to the raw config for legacy sinks.
    pub async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        match self.raw.as_prepared() {
            Some(p) => p.build_prepared_erased(self.prepared.as_ref(), cx).await,
            None => self.raw.build(cx).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinks::clickhouse::config::{ClickhouseConfig, Format};

    #[test]
    fn legacy_entry_builds_from_raw() {
        let config = ClickhouseConfig {
            endpoint: "http://localhost:8123".parse::<http::Uri>().unwrap().into(),
            table: "test_table".try_into().unwrap(),
            database: Some("test_db".try_into().unwrap()),
            format: Format::JsonEachRow,
            ..Default::default()
        };

        let entry = PreparedSinkEntry::legacy(Box::new(config));
        assert_eq!(entry.raw().get_component_name(), "clickhouse");
        // The prepared state for a legacy entry is the unit type.
        assert!(entry.prepared().is::<()>());
    }
}
