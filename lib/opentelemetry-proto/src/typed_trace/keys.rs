//! Semantic-convention and reserved Datadog OTLP bridge attribute keys.

pub const SERVICE_NAME: &str = "service.name";
pub const ENVIRONMENT_NAME: &str = "deployment.environment.name";
pub const ENVIRONMENT_LEGACY: &str = "deployment.environment";
pub const HOST_NAME: &str = "host.name";

pub const DATADOG_AGENT: &str = "datadog.agent";
pub const DATADOG_TRACER_TAGS: &str = "datadog.tracer.tags";
pub const DATADOG_CHUNK_PRIORITY: &str = "datadog.chunk.priority";
pub const DATADOG_CHUNK_ORIGIN: &str = "datadog.chunk.origin";
pub const DATADOG_CHUNK_DROPPED: &str = "datadog.chunk.dropped";
pub const DATADOG_CHUNK_TAGS: &str = "datadog.chunk.tags";
pub const DATADOG_SPAN_RESOURCE: &str = "datadog.span.resource";
pub const DATADOG_SPAN_TYPE: &str = "datadog.span.type";
pub const DATADOG_SPAN_META_STRUCT: &str = "datadog.span.meta_struct";

pub const AGENT_HOST_NAME: &str = "host_name";
pub const AGENT_ENV: &str = "env";
pub const AGENT_VERSION: &str = "agent_version";
pub const AGENT_TARGET_TPS: &str = "target_tps";
pub const AGENT_ERROR_TPS: &str = "error_tps";
pub const AGENT_RARE_SAMPLER: &str = "rare_sampler_enabled";
pub const AGENT_TAGS: &str = "tags";

/// Bridge keys reserved at both resource and span scope; a bridge key at the wrong scope is
/// malformed. Every other `datadog.*` key is an ordinary attribute.
pub fn is_reserved_datadog_key(key: &str) -> bool {
    matches!(
        key,
        DATADOG_AGENT
            | DATADOG_TRACER_TAGS
            | DATADOG_CHUNK_PRIORITY
            | DATADOG_CHUNK_ORIGIN
            | DATADOG_CHUNK_DROPPED
            | DATADOG_CHUNK_TAGS
            | DATADOG_SPAN_RESOURCE
            | DATADOG_SPAN_TYPE
            | DATADOG_SPAN_META_STRUCT
    )
}
