//! Datadog-native event and span context.

use std::collections::BTreeMap;

use bytes::Bytes;
use vector_common::byte_size_of::ByteSizeOf;
use vrl::value::KeyString;

use super::{Attributes, SamplingPriority};

/// Event-level Datadog-native state.
///
/// The default value means no Datadog-native state is present. `agent` and `chunk`
/// use `Option` so their wire envelopes can be absent independently of the always-present
/// tracer projection.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DatadogEventContext {
    /// Agent-to-intake envelope. Boxed so OTLP-originated events do not pay the
    /// inline size of an unused envelope.
    agent: Option<Box<DatadogAgentEnvelope>>,
    /// Tracer-payload tags. Always present; default is empty.
    pub tracer: DatadogTracerContext,
    /// Chunk-scoped sampling state. Boxed for the same reason as [`Self::agent`].
    chunk: Option<Box<DatadogChunkContext>>,
}

impl DatadogEventContext {
    /// Returns the agent envelope if present.
    #[must_use]
    pub fn agent(&self) -> Option<&DatadogAgentEnvelope> {
        self.agent.as_deref()
    }

    /// Returns a mutable agent envelope if present.
    pub fn agent_mut(&mut self) -> Option<&mut DatadogAgentEnvelope> {
        self.agent.as_deref_mut()
    }

    /// Replaces the agent envelope.
    pub fn set_agent(&mut self, agent: Option<DatadogAgentEnvelope>) {
        self.agent = agent.map(Box::new);
    }

    /// Returns the chunk context if present.
    #[must_use]
    pub fn chunk(&self) -> Option<&DatadogChunkContext> {
        self.chunk.as_deref()
    }

    /// Returns a mutable chunk context if present.
    pub fn chunk_mut(&mut self) -> Option<&mut DatadogChunkContext> {
        self.chunk.as_deref_mut()
    }

    /// Replaces the chunk context.
    pub fn set_chunk(&mut self, chunk: Option<DatadogChunkContext>) {
        self.chunk = chunk.map(Box::new);
    }
}

impl ByteSizeOf for DatadogEventContext {
    fn allocated_bytes(&self) -> usize {
        boxed_allocated(self.agent.as_deref())
            + self.tracer.allocated_bytes()
            + boxed_allocated(self.chunk.as_deref())
    }
}

fn boxed_allocated<T: ByteSizeOf>(value: Option<&T>) -> usize {
    value.map_or(0, |inner| {
        std::mem::size_of_val(inner) + inner.allocated_bytes()
    })
}

/// Datadog `AgentPayload` fields that have no format-independent home.
#[derive(Clone, Debug, Default)]
pub struct DatadogAgentEnvelope {
    /// Agent hostname.
    pub host_name: String,
    /// Agent `env`.
    pub env: String,
    /// Agent version string.
    pub agent_version: String,
    /// Agent `TargetTPS`.
    pub target_tps: f64,
    /// Agent `ErrorTPS`.
    pub error_tps: f64,
    /// Agent `RareSamplerEnabled`.
    pub rare_sampler_enabled: bool,
    /// Tags common to every tracer payload in the agent payload.
    pub tags: Attributes,
}

impl PartialEq for DatadogAgentEnvelope {
    fn eq(&self, other: &Self) -> bool {
        self.host_name == other.host_name
            && self.env == other.env
            && self.agent_version == other.agent_version
            && self.target_tps.to_bits() == other.target_tps.to_bits()
            && self.error_tps.to_bits() == other.error_tps.to_bits()
            && self.rare_sampler_enabled == other.rare_sampler_enabled
            && self.tags == other.tags
    }
}

impl Eq for DatadogAgentEnvelope {}

impl ByteSizeOf for DatadogAgentEnvelope {
    fn allocated_bytes(&self) -> usize {
        self.host_name.len()
            + self.env.len()
            + self.agent_version.len()
            + self.tags.allocated_bytes()
    }
}

/// Tracer-payload tags copied onto every event derived from that payload.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DatadogTracerContext {
    /// Tracer-payload tags.
    pub tags: Attributes,
}

impl ByteSizeOf for DatadogTracerContext {
    fn allocated_bytes(&self) -> usize {
        self.tags.allocated_bytes()
    }
}

/// Chunk-scoped Datadog sampling state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DatadogChunkContext {
    /// Sampling priority. `None` means the wire field was absent.
    pub priority: Option<SamplingPriority>,
    /// Chunk origin (`lambda`, `rum`, …). `None` means absent; `Some("")` is preserved.
    pub origin: Option<String>,
    /// Whether the chunk was dropped by a sampler.
    pub dropped: bool,
    /// Chunk tags.
    pub tags: Attributes,
}

impl ByteSizeOf for DatadogChunkContext {
    fn allocated_bytes(&self) -> usize {
        self.origin.as_ref().map_or(0, String::len) + self.tags.allocated_bytes()
    }
}

/// Per-span Datadog-native fields.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DatadogSpanContext {
    /// Datadog `Span.resource`.
    pub resource_name: Option<String>,
    /// Datadog `Span.type`.
    pub span_type: Option<String>,
    /// Datadog `Span.meta_struct`.
    pub meta_struct: BTreeMap<KeyString, Bytes>,
}

impl ByteSizeOf for DatadogSpanContext {
    fn allocated_bytes(&self) -> usize {
        self.resource_name.as_ref().map_or(0, String::len)
            + self.span_type.as_ref().map_or(0, String::len)
            + self.meta_struct.allocated_bytes()
    }
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use similar_asserts::assert_eq;

    use super::{DatadogAgentEnvelope, DatadogChunkContext, DatadogEventContext};

    #[test]
    fn optional_envelopes_are_boxed() {
        // Unboxed `Option<DatadogAgentEnvelope>` cannot use a niche and is therefore
        // much larger than a pointer. OTLP-originated events leave agent/chunk absent,
        // so boxing those optional envelopes is the cheaper layout. Always-present
        // `DatadogEventContext` / `DatadogSpanContext` stay unboxed.
        assert!(
            size_of::<Option<Box<DatadogAgentEnvelope>>>()
                < size_of::<Option<DatadogAgentEnvelope>>()
        );
        assert!(
            size_of::<Option<Box<DatadogChunkContext>>>()
                < size_of::<Option<DatadogChunkContext>>()
        );
        assert_eq!(
            size_of::<Option<Box<DatadogAgentEnvelope>>>(),
            size_of::<usize>()
        );

        let mut ctx = DatadogEventContext::default();
        assert!(ctx.agent().is_none());
        ctx.set_agent(Some(DatadogAgentEnvelope {
            host_name: "h".into(),
            ..DatadogAgentEnvelope::default()
        }));
        assert_eq!(ctx.agent().unwrap().host_name, "h");
    }

    #[test]
    fn defaults_mean_absent_state() {
        let ctx = DatadogEventContext::default();
        assert!(ctx.agent.is_none());
        assert!(ctx.chunk.is_none());
        assert!(ctx.tracer.tags.is_empty());
    }
}
