//! Prepared sink infrastructure for the sinks-only lifecycle architecture.
//!
//! This module implements a split between the serde/typetag-facing `SinkConfig` role
//! and the topology-facing runtime role. It introduces:
//!
//! - `ValidateSink`: A typed trait for pure validation/preparation
//! - `PreparedSink`: An object-safe erased validated/prepared sink trait
//! - `BoxedPreparedSink`: A boxed type for storing prepared sinks
//! - `LegacyPreparedSink`: An adapter for unmigrated sinks
//!
//! Design goals:
//! - Validation/preparation returns retained values passed to build
//! - Validation is pure: no filesystem/network/credentials/spawn/await
//! - Build may do environment-dependent construction but must not redo pure validation
//! - Preserve raw config serialization, reload diffing, and metadata access

use std::path::PathBuf;

use async_trait::async_trait;
use dyn_clone::DynClone;
use vector_lib::{
    config::{AcknowledgementsConfig, Input},
    sink::VectorSink,
};

use super::{Resource, SinkContext};
use crate::{sinks::Healthcheck, template::ConfinementConfig};

/// Object-safe trait for prepared/validated sinks.
///
/// This is the topology-facing runtime interface that prepared sinks implement.
/// It provides build and metadata access without exposing heterogeneous associated types.
#[async_trait]
pub trait PreparedSink: DynClone + Send + Sync + core::fmt::Debug {
    /// Returns the component type name (e.g., "clickhouse").
    fn get_type_name(&self) -> &'static str;

    /// Builds the sink with the given context.
    ///
    /// This method may perform environment-dependent construction (HTTP clients, etc.)
    /// but must not redo pure validation that was already done during preparation.
    ///
    /// # Errors
    ///
    /// If an error occurs while building the sink, an error variant explaining the issue is
    /// returned.
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)>;

    /// Gets the input configuration for this sink.
    fn input(&self) -> Input;

    /// Gets the files to watch to trigger reload.
    fn files_to_watch(&self) -> Vec<&PathBuf> {
        Vec::new()
    }

    /// Gets the list of resources, if any, used by this sink.
    fn resources(&self) -> Vec<Resource> {
        Vec::new()
    }

    /// Returns this sink's template-confinement config, if it supports confinement.
    fn confinement_config(&self) -> Option<&ConfinementConfig> {
        None
    }

    /// Gets the acknowledgements configuration for this sink.
    fn acknowledgements(&self) -> AcknowledgementsConfig;
}

dyn_clone::clone_trait_object!(PreparedSink);

/// Boxed prepared sink type.
pub type BoxedPreparedSink = Box<dyn PreparedSink>;

/// Typed helper trait for sink validation/preparation.
///
/// Concrete sinks implement this trait to express their validated type and pure
/// validation/preparation conversion. The associated type `Validated` captures
/// all pure validation results that build can consume without recomputing.
///
/// # Purity Guarantees
///
/// The `prepare` method must be pure:
/// - No filesystem access
/// - No network operations
/// - No credential resolution
/// - No spawning
/// - No async/await
///
/// All such environment-dependent operations belong in `build` on `Validated`.
pub trait ValidateSink {
    /// The validated/prepared state type.
    ///
    /// This type captures all pure validation results and should implement
    /// `PreparedSink` to provide the build method.
    type Validated: PreparedSink;

    /// Performs pure validation and preparation.
    ///
    /// This method must be pure (no side effects, no environment access).
    /// All validation results are captured in the returned `Validated` type.
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails. The error should be descriptive
    /// and preserve existing error messages.
    fn prepare(&self) -> crate::Result<Self::Validated>;
}

/// Legacy adapter for unmigrated sinks.
///
/// This adapter wraps a raw `SinkConfig` and delegates all operations to it,
/// allowing unmigrated sinks to continue working without modification.
/// Validation is performed lazily during build.
#[derive(Clone, Debug)]
pub struct LegacyPreparedSink {
    /// The raw sink config, preserved for serialization and metadata access.
    inner: super::BoxedSink,
}

impl LegacyPreparedSink {
    /// Creates a new legacy adapter wrapping a raw sink config.
    pub fn new(inner: super::BoxedSink) -> Self {
        Self { inner }
    }

    /// Returns a reference to the inner raw sink config.
    pub fn inner(&self) -> &super::BoxedSink {
        &self.inner
    }
}

#[async_trait]
impl PreparedSink for LegacyPreparedSink {
    fn get_type_name(&self) -> &'static str {
        self.inner.get_component_name()
    }

    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        self.inner.build(cx).await
    }

    fn input(&self) -> Input {
        self.inner.input()
    }

    fn files_to_watch(&self) -> Vec<&PathBuf> {
        self.inner.files_to_watch()
    }

    fn resources(&self) -> Vec<Resource> {
        self.inner.resources()
    }

    fn confinement_config(&self) -> Option<&ConfinementConfig> {
        self.inner.confinement_config()
    }

    fn acknowledgements(&self) -> AcknowledgementsConfig {
        *self.inner.acknowledgements()
    }
}

/// Prepared sink with retained raw config.
///
/// This type pairs a prepared sink with its original raw configuration,
/// preserving serialization and reload diffing capabilities.
#[derive(Clone, Debug)]
pub struct PreparedSinkEntry {
    /// The raw sink config, preserved for serialization and diffing.
    raw: super::BoxedSink,
    /// The prepared sink for topology building.
    prepared: BoxedPreparedSink,
}

impl PreparedSinkEntry {
    /// Creates a new prepared sink entry from a prepared sink and its raw config.
    pub fn new(raw: super::BoxedSink, prepared: BoxedPreparedSink) -> Self {
        Self { raw, prepared }
    }

    /// Creates a legacy entry that wraps the raw config without native preparation.
    pub fn legacy(raw: super::BoxedSink) -> Self {
        let prepared = Box::new(LegacyPreparedSink::new(raw.clone()));
        Self { raw, prepared }
    }

    /// Returns a reference to the raw sink config.
    pub fn raw(&self) -> &super::BoxedSink {
        &self.raw
    }

    /// Returns a reference to the prepared sink.
    pub fn prepared(&self) -> &dyn PreparedSink {
        self.prepared.as_ref()
    }

    /// Returns the prepared sink for topology building.
    pub fn into_prepared(self) -> BoxedPreparedSink {
        self.prepared
    }

    /// Returns the raw config for serialization.
    pub fn into_raw(self) -> super::BoxedSink {
        self.raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinks::clickhouse::config::{ClickhouseConfig, Format};

    #[test]
    fn trait_object_safety() {
        // Ensure PreparedSink is object-safe by attempting to create a trait object.
        fn _assert_object_safe(_: Box<dyn PreparedSink>) {}
    }

    #[test]
    fn legacy_adapter_creation() {
        let config = ClickhouseConfig {
            endpoint: "http://localhost:8123".parse::<http::Uri>().unwrap().into(),
            table: "test_table".try_into().unwrap(),
            database: Some("test_db".try_into().unwrap()),
            format: Format::JsonEachRow,
            ..Default::default()
        };

        let legacy = LegacyPreparedSink::new(Box::new(config));
        assert_eq!(legacy.get_type_name(), "clickhouse");
    }
}
