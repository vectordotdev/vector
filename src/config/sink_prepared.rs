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
//!
//! The erased prepared state is stored on `SinkOuter` (see `SinkOuter::prepared`) and
//! consumed at build time by `SinkOuter::build`, which dispatches through
//! `PreparedSinkErased::build_prepared_erased`.
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
