//! Attribute keys shared by Datadog egress grouping and span encoding.
//!
//! The tracer-payload keys match the legacy Datadog ingress shim so a
//! `Datadog -> Vector -> Datadog` round trip restores `TracerPayload` scalars.

/// Sink-owned high half of a 128-bit trace ID.
pub(super) const TID_TAG: &str = "_dd.p.tid";
/// Typed span status message, written into `meta` when non-empty.
pub(super) const ERROR_MESSAGE: &str = "error.message";

/// Promoted to [`crate::event::typed_trace::Resource::service`].
pub(super) const SERVICE_NAME: &str = "service.name";
/// Promoted to [`crate::event::typed_trace::Resource::environment`].
pub(super) const ENVIRONMENT_NAME: &str = "deployment.environment.name";
/// Promoted to [`crate::event::typed_trace::Resource::host`].
pub(super) const HOST_NAME: &str = "host.name";

/// `TracerPayload.containerID`.
pub(super) const CONTAINER_ID: &str = "container.id";
/// `TracerPayload.languageName`.
pub(super) const TELEMETRY_SDK_LANGUAGE: &str = "telemetry.sdk.language";
/// `TracerPayload.languageVersion`.
pub(super) const PROCESS_RUNTIME_VERSION: &str = "process.runtime.version";
/// `TracerPayload.tracerVersion`.
pub(super) const TELEMETRY_SDK_VERSION: &str = "telemetry.sdk.version";
/// `TracerPayload.runtimeID`.
pub(super) const RUNTIME_ID: &str = "runtime-id";
/// `TracerPayload.appVersion`.
pub(super) const SERVICE_VERSION: &str = "service.version";

/// Resource attributes that populate `TracerPayload` scalars, in wire-field order.
pub(super) const TRACER_ATTRIBUTE_KEYS: [&str; 6] = [
    CONTAINER_ID,
    TELEMETRY_SDK_LANGUAGE,
    PROCESS_RUNTIME_VERSION,
    TELEMETRY_SDK_VERSION,
    RUNTIME_ID,
    SERVICE_VERSION,
];

/// Resource keys that are not copied onto span `meta`.
///
/// Typed slots and tracer-payload scalars already have wire fields. Other
/// resource attributes are flattened per span.
pub(super) fn is_unflattened_resource_key(key: &str) -> bool {
    key == SERVICE_NAME
        || key == ENVIRONMENT_NAME
        || key == HOST_NAME
        || TRACER_ATTRIBUTE_KEYS.contains(&key)
}
