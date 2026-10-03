//! [`WireToArrowEncoder`]: builds a [`MessagePlan`] once per
//! (proto descriptor, Arrow schema) pair and encodes batches of proto wire
//! bytes into Arrow `RecordBatch`es.

use std::sync::Arc;

use arrow::datatypes::{Fields, Schema};
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use prost_reflect::MessageDescriptor;

use super::builders::BuilderNodeList;
use super::errors::{Result, WireToArrowError};
use super::plan::MessagePlan;
use super::scan::{scan_message, validate_message};

/// Streaming wire-format encoder. Build once per (proto message type,
/// Arrow schema) pair, then call [`WireToArrowEncoder::encode_batch`]
/// repeatedly.
pub struct WireToArrowEncoder {
    plan: Arc<MessagePlan>,
    schema: Arc<Schema>,
}

impl std::fmt::Debug for WireToArrowEncoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WireToArrowEncoder")
            .field("schema_fields", &self.schema.fields().len())
            .finish()
    }
}

impl WireToArrowEncoder {
    /// Compile a plan for the given proto descriptor + Arrow schema.
    ///
    /// Every field in `schema` must exist (by name) in `descriptor`. Proto
    /// fields absent from `schema` are silently skipped at scan time.
    pub fn new(descriptor: &MessageDescriptor, schema: Schema) -> Result<Self> {
        let plan = MessagePlan::build(descriptor, &Fields::from(schema.fields().clone()))?;
        Ok(Self {
            plan: Arc::new(plan),
            schema: Arc::new(schema),
        })
    }

    /// Encode a batch of serialized proto messages into a single `RecordBatch`.
    ///
    /// Per-row isolation: each message is pre-validated via
    /// [`validate_message`] before any builder is touched. Rows that fail
    /// validation are dropped from the output batch and counted via the
    /// `wire_to_arrow_rows_dropped` metric (plus a rate-limit-friendly
    /// warn log carrying a sample error). Returning an empty `RecordBatch`
    /// is acceptable when every row was malformed.
    ///
    /// Errors out of this method are reserved for batch-level failures
    /// that aren't attributable to a single row: a code-bug surface
    /// (scan-vs-validate divergence surfaced as `PlanBuilderMismatch`) or
    /// a `RecordBatchAssembly` rejection from Arrow. Row-finalize is
    /// infallible, so it doesn't appear in this list.
    pub fn encode_batch(&self, messages: &[Bytes]) -> Result<RecordBatch> {
        let capacity = messages.len();
        let mut builders = BuilderNodeList::with_capacity(&self.plan, capacity)?;
        let mut dropped = 0u64;
        let mut sample_err: Option<WireToArrowError> = None;

        for msg_bytes in messages {
            // Pre-validate so a malformed row drops without poisoning any
            // builder. Arrow `*Builder` has no public rollback API, and
            // nested-struct `finalize_row` calls inside `scan_message` are
            // not reversible, so an upfront validation pass is how we
            // isolate per-row decode failures.
            if let Err(err) = validate_message(&self.plan, msg_bytes) {
                dropped += 1;
                if sample_err.is_none() {
                    sample_err = Some(err);
                }
                continue;
            }
            builders.reset_present();
            scan_message(&self.plan, msg_bytes, &mut builders)?;
            builders.finalize_row(&self.plan);
        }

        if dropped > 0 {
            metrics::counter!("wire_to_arrow_rows_dropped").increment(dropped);
            tracing::warn!(
                message = "wire-to-Arrow dropped malformed rows from batch",
                dropped,
                batch_size = messages.len(),
                sample_error = ?sample_err,
            );
        }

        let arrays = builders.finish(&self.plan)?;
        RecordBatch::try_new(Arc::clone(&self.schema), arrays)
            .map_err(|source| WireToArrowError::RecordBatchAssembly { source })
    }
}
