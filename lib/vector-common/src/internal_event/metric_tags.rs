use serde_json::{Value, json};
use std::sync::LazyLock;

/// Compose a metric tag set, optionally extending a base.
///
/// # Forms
///
/// ```rust,ignore
/// // Extend a base tag set with additional tags (returns Value):
/// metric_tags! {
///     ..COMPONENT_TAGS,
///     "topic_id": {"description": "The Kafka topic id.", "required": true},
/// }
///
/// // Simple reference to a base tag set (returns &Value):
/// metric_tags!(..COMPONENT_TAGS)
///
/// // Empty tag set (returns Value):
/// metric_tags!()
/// ```
#[macro_export]
macro_rules! metric_tags {
    // Extend a base tag set with additional tags.
    (.. $base:expr, $($rest:tt)+) => {
        $crate::internal_event::metric_tags::merge_lazy(
            &$base,
            ::serde_json::json!({ $($rest)* })
        )
    };
    // Reference a base tag set (clones so the return type is Value in all forms).
    (.. $base:expr $(,)?) => {
        (*$base).clone()
    };
    // Empty tag set.
    () => {
        ::serde_json::json!({})
    };
}

/// Clones `base` (a `LazyLock<Value>`) and inserts all fields from `extra`.
///
/// Intended for static initializers: `LazyLock::new(|| merge_lazy(&BASE, json!({...})))`.
/// For inline annotations prefer the [`metric_tags!`] macro.
#[must_use]
pub fn merge_lazy(base: &LazyLock<Value>, extra: Value) -> Value {
    let mut result = (**base).clone();
    if let (Some(obj), Value::Object(extra_obj)) = (result.as_object_mut(), extra) {
        obj.extend(extra_obj);
    }
    result
}

// ─── Base tag groups ───────────────────────────────────────────────────────────

pub static INTERNAL_METRICS_TAGS: LazyLock<Value> = LazyLock::new(|| {
    json!({
        "pid": {"description": "The process ID of the Vector instance.", "required": false, "examples": ["4232"]},
        "host": {"description": "The hostname of the system Vector is running on.", "required": false, "examples": ["my-host.local"]}
    })
});

pub static COMPONENT_TAGS: LazyLock<Value> = LazyLock::new(|| {
    merge_lazy(
        &INTERNAL_METRICS_TAGS,
        json!({
            "component_kind": {
                "description": "The Vector component kind.",
                "required": true,
                "enum": {
                    "sink": "Vector sink components",
                    "source": "Vector source components",
                    "transform": "Vector transform components"
                }
            },
            "component_id": {"description": "The Vector component ID.", "required": true, "examples": ["my_source", "my_sink"]},
            "component_type": {"description": "The Vector component type.", "required": true, "examples": ["file", "http", "honeycomb", "splunk_hec"]}
        }),
    )
});

// ─── Extensions of COMPONENT_TAGS ─────────────────────────────────────────────

pub static COMPONENT_TAGS_OUTPUT: LazyLock<Value> = LazyLock::new(|| {
    merge_lazy(
        &COMPONENT_TAGS,
        json!({
            "output": {"description": "The specific output of the component.", "required": false}
        }),
    )
});

pub static COMPONENT_TAGS_GRPC_METHOD_SERVICE: LazyLock<Value> = LazyLock::new(|| {
    merge_lazy(
        &COMPONENT_TAGS,
        json!({
            "grpc_method": {"description": "The name of the method called on the gRPC service.", "required": true},
            "grpc_service": {"description": "The gRPC service name.", "required": true}
        }),
    )
});

pub static COMPONENT_TAGS_GRPC_ALL: LazyLock<Value> = LazyLock::new(|| {
    merge_lazy(
        &COMPONENT_TAGS_GRPC_METHOD_SERVICE,
        json!({
            "grpc_status": {"description": "The human-readable [gRPC status code](https://grpc.github.io/grpc/core/md_doc_statuscodes.html).", "required": true}
        }),
    )
});

pub static COMPONENT_TAGS_HTTP_METHOD: LazyLock<Value> = LazyLock::new(|| {
    merge_lazy(
        &COMPONENT_TAGS,
        json!({
            "method": {"description": "The HTTP method of the request.", "required": false}
        }),
    )
});

pub static COMPONENT_TAGS_HTTP_STATUS: LazyLock<Value> = LazyLock::new(|| {
    merge_lazy(
        &COMPONENT_TAGS,
        json!({
            "status": {"description": "The HTTP status code of the request.", "required": false}
        }),
    )
});

pub static COMPONENT_TAGS_HTTP_METHOD_PATH: LazyLock<Value> = LazyLock::new(|| {
    merge_lazy(
        &COMPONENT_TAGS_HTTP_METHOD,
        json!({
            "path": {"description": "The path that produced the error.", "required": true}
        }),
    )
});

pub static COMPONENT_TAGS_HTTP_ALL: LazyLock<Value> = LazyLock::new(|| {
    merge_lazy(
        &COMPONENT_TAGS_HTTP_METHOD_PATH,
        json!({
            "status": {"description": "The HTTP status code of the request.", "required": false}
        }),
    )
});

pub static COMPONENT_TAGS_ERROR_TYPE_STAGE: LazyLock<Value> = LazyLock::new(|| {
    merge_lazy(
        &COMPONENT_TAGS,
        json!({
            "error_type": {
                "description": "The type of the error",
                "required": true,
                "enum": {
                    "acknowledgements_failed": "The acknowledgement operation failed.",
                    "delete_failed": "The file deletion failed.",
                    "encode_failed": "The encode operation failed.",
                    "field_missing": "The event field was missing.",
                    "glob_failed": "The glob pattern match operation failed.",
                    "http_error": "The HTTP request resulted in an error code.",
                    "invalid_metric": "The metric was invalid.",
                    "kafka_offset_update": "The consumer offset update failed.",
                    "kafka_read": "The message from Kafka was invalid.",
                    "mapping_failed": "The mapping failed.",
                    "match_failed": "The match operation failed.",
                    "out_of_order": "The event was out of order.",
                    "parse_failed": "The parsing operation failed.",
                    "read_failed": "The file read operation failed.",
                    "render_error": "The rendering operation failed.",
                    "stream_closed": "The downstream was closed, forwarding the event(s) failed.",
                    "type_conversion_failed": "The type conversion operating failed.",
                    "type_field_does_not_exist": "The type field does not exist.",
                    "type_ip_address_parse_error": "The IP address did not parse.",
                    "unlabeled_event": "The event was not labeled.",
                    "value_invalid": "The value was invalid.",
                    "watch_failed": "The file watch operation failed.",
                    "write_failed": "The file write operation failed."
                }
            },
            "stage": {
                "description": "The stage within the component at which the error occurred.",
                "required": true,
                "enum": {
                    "receiving": "While receiving data.",
                    "processing": "While processing data within the component.",
                    "sending": "While sending data."
                }
            }
        }),
    )
});

// ─── Extensions of INTERNAL_METRICS_TAGS ──────────────────────────────────────

pub static INTERNAL_METRICS_TAGS_FILE: LazyLock<Value> = LazyLock::new(|| {
    merge_lazy(
        &INTERNAL_METRICS_TAGS,
        json!({
            "file": {"description": "The file that produced the error.", "required": false}
        }),
    )
});

pub static INTERNAL_METRICS_TAGS_REASON: LazyLock<Value> = LazyLock::new(|| {
    merge_lazy(
        &INTERNAL_METRICS_TAGS,
        json!({
            "reason": {
                "description": "The type of the error",
                "required": true,
                "enum": {
                    "out_of_order": "The event was out of order.",
                    "oversized": "The event was too large."
                }
            }
        }),
    )
});

// ─── Metric-specific tag sets ─────────────────────────────────────────────────

pub static COMPONENT_RECEIVED_EVENTS_TOTAL_TAGS: LazyLock<Value> = LazyLock::new(|| {
    merge_lazy(
        &COMPONENT_TAGS,
        json!({
            "file": {"description": "The file from which the data originated.", "required": false},
            "uri": {"description": "The sanitized URI from which the data originated.", "required": false},
            "container_name": {"description": "The name of the container from which the data originated.", "required": false},
            "pod_name": {"description": "The name of the pod from which the data originated.", "required": false},
            "peer_addr": {"description": "The IP from which the data originated.", "required": false},
            "peer_path": {"description": "The pathname from which the data originated.", "required": false},
            "mode": {
                "description": "The connection mode used by the component.",
                "required": false,
                "enum": {
                    "udp": "User Datagram Protocol",
                    "tcp": "Transmission Control Protocol",
                    "unix": "Unix domain socket"
                }
            }
        }),
    )
});

/// Same tag set as `component_received_events_total` (inherited by byte-count metrics).
pub static COMPONENT_RECEIVED_EVENTS_TAGS: LazyLock<Value> =
    LazyLock::new(|| COMPONENT_RECEIVED_EVENTS_TOTAL_TAGS.clone());

pub static COMPONENT_TAGS_HTTP_ERROR_KIND: LazyLock<Value> = LazyLock::new(|| {
    merge_lazy(
        &COMPONENT_TAGS,
        json!({
            "error_kind": {"description": "The kind of HTTP error encountered (e.g. connection refused, connection reset).", "required": true}
        }),
    )
});

pub static S3_OBJECT_PROCESSING_TAGS: LazyLock<Value> = LazyLock::new(|| {
    merge_lazy(
        &COMPONENT_TAGS,
        json!({
            "bucket": {"description": "The name of the S3 bucket.", "required": true}
        }),
    )
});
