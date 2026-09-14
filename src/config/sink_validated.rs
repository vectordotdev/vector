//! Validated sink infrastructure for the sinks-only lifecycle architecture.
//!
//! This module implements a split between the serde/typetag-facing `SinkConfig` role
//! and the topology-facing runtime role. It introduces:
//!
//! - `ValidatedSink`: a generic, implementor-facing trait. A migrated sink implements
//!   this and works entirely with its own concrete validated type `T` — it returns `T`
//!   from `validate` and receives `&T` back in `build`. No `Any` in sight.
//! - `DynValidatedSink`: the object-safe boundary the framework actually crosses.
//!   A blanket impl derives it from any `ValidatedSink`, performing the `Box<dyn Any>`
//!   erasure and downcast automatically.
//!
//! The erased validated state is stored on `SinkOuter` (see `SinkOuter::validated`) and
//! consumed at build time by `SinkOuter::build`, which dispatches through
//! `DynValidatedSink::build_dyn`.
//!
//! `ValidatedSink::validate` is the same phase as the RFC's
//! `SinkConfig::validate_structure` (pure structural validation owned by config
//! compilation), but it retains the validated state so `build` does not
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

/// Generic validated-sink trait, implemented by migrated sinks.
///
/// The implementor works entirely with their concrete validated type `Self::Validated`:
///
/// ```ignore
/// #[async_trait]
/// impl ValidatedSink for MySinkConfig {
///     type Validated = ValidatedMySink;
///
///     fn validate(&self) -> crate::Result<Self::Validated> {
///         ValidatedMySink::from_config(self)
///     }
///
///     async fn build(
///         &self,
///         validated: &Self::Validated,
///         cx: SinkContext,
///     ) -> crate::Result<(VectorSink, Healthcheck)> {
///         validated.build(cx).await
///     }
/// }
/// ```
///
/// The framework automatically erases `Self::Validated` to `Box<dyn Any>` and restores
/// it at build time via `DynValidatedSink`, so no `Any` appears in implementor code.
#[async_trait]
pub trait ValidatedSink {
    /// The concrete validated state produced by `validate` and consumed by `build`.
    type Validated: Send + Sync + 'static;

    /// Performs pure structural validation, returning the validated state.
    ///
    /// This is the same phase as the RFC's `SinkConfig::validate_structure`, but it
    /// retains the validated state so `build` does not redo it.
    ///
    /// # Purity Guarantees
    ///
    /// This method must be pure: no filesystem access, no network operations, no
    /// credential resolution, no spawning, and no async/await. All such environment-
    /// dependent operations belong in `build`.
    fn validate(&self) -> crate::Result<Self::Validated>;

    /// Performs context-dependent validation that requires the enrichment tables.
    ///
    /// Defaults to a no-op; sinks that resolve enrichment tables at compile time
    /// (e.g. custom-auth VRL programs) override this to run against the configured
    /// tables, which are only available at the config-validation layer.
    fn validate_with_context(&self, _cx: &SinkContext) -> crate::Result<()> {
        Ok(())
    }

    /// Builds the sink from the validated state, without redoing pure validation.
    ///
    /// May perform environment-dependent construction (HTTP clients, schema fetching, etc.).
    async fn build(
        &self,
        validated: &Self::Validated,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)>;
}

/// Object-safe `dyn` boundary used by the framework.
///
/// This is what actually crosses the `Box<dyn SinkConfig>` boundary. It is derived
/// automatically from any `ValidatedSink` by the blanket impl below, which owns the
/// `Box<dyn Any>` erasure and the downcast back to the concrete validated type.
#[async_trait]
pub trait DynValidatedSink {
    /// Erases the validated state into a `Box<dyn Any>`.
    fn validate_dyn(&self) -> crate::Result<Box<dyn Any + Send + Sync>>;

    /// Validates context-dependent configuration (e.g. VRL programs that resolve
    /// enrichment tables) against the given context.
    fn validate_with_context_dyn(&self, cx: &SinkContext) -> crate::Result<()>;

    /// Restores the validated state from `&dyn Any` and builds the sink.
    async fn build_dyn(
        &self,
        validated: &(dyn Any + Send + Sync),
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)>;
}

#[async_trait]
impl<T> DynValidatedSink for T
where
    T: ValidatedSink + Send + Sync + 'static,
{
    fn validate_dyn(&self) -> crate::Result<Box<dyn Any + Send + Sync>> {
        Ok(Box::new(self.validate()?))
    }

    fn validate_with_context_dyn(&self, cx: &SinkContext) -> crate::Result<()> {
        self.validate_with_context(cx)
    }

    async fn build_dyn(
        &self,
        validated: &(dyn Any + Send + Sync),
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let validated = validated
            .downcast_ref::<T::Validated>()
            .expect("validated state type mismatch");
        self.build(validated, cx).await
    }
}
