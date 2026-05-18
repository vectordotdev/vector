use serde_json::{Value, json};
use std::sync::LazyLock;

/// Clones `base` and inserts all fields from `extra`, returning the merged object.
///
/// Intended for inline use in `#[configurable(metadata(docs::tags = ...))]` to avoid
/// naming single-use tag sets:
///
/// ```ignore
/// #[configurable(metadata(docs::tags = merge(&*COMPONENT_TAGS, json!({
///     "bucket": {"description": "The S3 bucket.", "required": true}
/// }))))]
/// ```
#[must_use]
pub fn merge(base: &Value, extra: Value) -> Value {
    let mut result = base.clone();
    if let (Some(obj), Value::Object(extra_obj)) = (result.as_object_mut(), extra) {
        obj.extend(extra_obj);
    }
    result
}

// ─── Base tag groups ───────────────────────────────────────────────────────────

pub static INTERNAL_METRICS_TAGS: LazyLock<Value> = LazyLock::new(|| {
    json!({
        "pid":  {"description": "The process ID of the Vector instance.", "required": false},
        "host": {"description": "The hostname of the system Vector is running on.", "required": false}
    })
});

pub static COMPONENT_TAGS: LazyLock<Value> = LazyLock::new(|| {
    let mut tags = INTERNAL_METRICS_TAGS.clone();
    let obj = tags.as_object_mut().unwrap();
    obj.insert(
        "component_kind".to_owned(),
        json!({
            "description": "The Vector component kind.",
            "required": true,
            "enum": {
                "sink": "Vector sink components",
                "source": "Vector source components",
                "transform": "Vector transform components"
            }
        }),
    );
    obj.insert(
        "component_id".to_owned(),
        json!({"description": "The Vector component ID.", "required": true}),
    );
    obj.insert(
        "component_type".to_owned(),
        json!({"description": "The Vector component type.", "required": true}),
    );
    tags
});

// ─── Extensions of COMPONENT_TAGS ─────────────────────────────────────────────

pub static COMPONENT_TAGS_OUTPUT: LazyLock<Value> = LazyLock::new(|| {
    let mut tags = COMPONENT_TAGS.clone();
    tags.as_object_mut().unwrap().insert(
        "output".to_owned(),
        json!({"description": "The specific output of the component.", "required": false}),
    );
    tags
});

pub static COMPONENT_TAGS_GRPC_METHOD_SERVICE: LazyLock<Value> = LazyLock::new(|| {
    let mut tags = COMPONENT_TAGS.clone();
    let obj = tags.as_object_mut().unwrap();
    obj.insert("grpc_method".to_owned(),
        json!({"description": "The name of the method called on the gRPC service.", "required": true}));
    obj.insert(
        "grpc_service".to_owned(),
        json!({"description": "The gRPC service name.", "required": true}),
    );
    tags
});

pub static COMPONENT_TAGS_GRPC_ALL: LazyLock<Value> = LazyLock::new(|| {
    let mut tags = COMPONENT_TAGS_GRPC_METHOD_SERVICE.clone();
    tags.as_object_mut().unwrap().insert(
        "grpc_status".to_owned(),
        json!({"description": "The human-readable gRPC status code.", "required": true}),
    );
    tags
});

pub static COMPONENT_TAGS_HTTP_METHOD: LazyLock<Value> = LazyLock::new(|| {
    let mut tags = COMPONENT_TAGS.clone();
    tags.as_object_mut().unwrap().insert(
        "method".to_owned(),
        json!({"description": "The HTTP method of the request.", "required": false}),
    );
    tags
});

pub static COMPONENT_TAGS_HTTP_STATUS: LazyLock<Value> = LazyLock::new(|| {
    let mut tags = COMPONENT_TAGS.clone();
    tags.as_object_mut().unwrap().insert(
        "status".to_owned(),
        json!({"description": "The HTTP status code of the request.", "required": false}),
    );
    tags
});

pub static COMPONENT_TAGS_HTTP_METHOD_PATH: LazyLock<Value> = LazyLock::new(|| {
    let mut tags = COMPONENT_TAGS_HTTP_METHOD.clone();
    tags.as_object_mut().unwrap().insert(
        "path".to_owned(),
        json!({"description": "The path that produced the error.", "required": true}),
    );
    tags
});

pub static COMPONENT_TAGS_HTTP_ALL: LazyLock<Value> = LazyLock::new(|| {
    let mut tags = COMPONENT_TAGS_HTTP_METHOD_PATH.clone();
    tags.as_object_mut().unwrap().insert(
        "status".to_owned(),
        json!({"description": "The HTTP status code of the request.", "required": false}),
    );
    tags
});

pub static COMPONENT_TAGS_ERROR_TYPE_STAGE: LazyLock<Value> = LazyLock::new(|| {
    let mut tags = COMPONENT_TAGS.clone();
    let obj = tags.as_object_mut().unwrap();
    obj.insert("error_type".to_owned(), json!({"description": "The type of the error", "required": true, "enum": {
        "acknowledgements_failed":     "The acknowledgement operation failed.",
        "delete_failed":               "The file deletion failed.",
        "encode_failed":               "The encode operation failed.",
        "field_missing":               "The event field was missing.",
        "glob_failed":                 "The glob pattern match operation failed.",
        "http_error":                  "The HTTP request resulted in an error code.",
        "invalid_metric":              "The metric was invalid.",
        "kafka_offset_update":         "The consumer offset update failed.",
        "kafka_read":                  "The message from Kafka was invalid.",
        "mapping_failed":              "The mapping failed.",
        "match_failed":                "The match operation failed.",
        "out_of_order":                "The event was out of order.",
        "parse_failed":                "The parsing operation failed.",
        "read_failed":                 "The file read operation failed.",
        "render_error":                "The rendering operation failed.",
        "stream_closed":               "The downstream was closed, forwarding the event(s) failed.",
        "type_conversion_failed":      "The type conversion operating failed.",
        "type_field_does_not_exist":   "The type field does not exist.",
        "type_ip_address_parse_error": "The IP address did not parse.",
        "unlabeled_event":             "The event was not labeled.",
        "value_invalid":               "The value was invalid.",
        "watch_failed":                "The file watch operation failed.",
        "write_failed":                "The file write operation failed."
    }}));
    obj.insert("stage".to_owned(), json!({"description": "The stage within the component at which the error occurred.", "required": true, "enum": {
        "receiving":  "While receiving data.",
        "processing": "While processing data within the component.",
        "sending":    "While sending data."
    }}));
    tags
});

// ─── Extensions of INTERNAL_METRICS_TAGS ──────────────────────────────────────

pub static INTERNAL_METRICS_TAGS_FILE: LazyLock<Value> = LazyLock::new(|| {
    let mut tags = INTERNAL_METRICS_TAGS.clone();
    tags.as_object_mut().unwrap().insert(
        "file".to_owned(),
        json!({"description": "The file that produced the error.", "required": false}),
    );
    tags
});

pub static INTERNAL_METRICS_TAGS_REASON: LazyLock<Value> = LazyLock::new(|| {
    let mut tags = INTERNAL_METRICS_TAGS.clone();
    tags.as_object_mut().unwrap().insert("reason".to_owned(), json!({
        "description": "The type of the error",
        "required": true,
        "enum": {"out_of_order": "The event was out of order.", "oversized": "The event was too large."}
    }));
    tags
});

// ─── Metric-specific tag sets ─────────────────────────────────────────────────

pub static COMPONENT_RECEIVED_EVENTS_TOTAL_TAGS: LazyLock<Value> = LazyLock::new(|| {
    let mut tags = COMPONENT_TAGS.clone();
    let obj = tags.as_object_mut().unwrap();
    obj.insert(
        "file".to_owned(),
        json!({"description": "The file from which the data originated.", "required": false}),
    );
    obj.insert("uri".to_owned(),
        json!({"description": "The sanitized URI from which the data originated.", "required": false}));
    obj.insert("container_name".to_owned(),
        json!({"description": "The name of the container from which the data originated.", "required": false}));
    obj.insert("pod_name".to_owned(),
        json!({"description": "The name of the pod from which the data originated.", "required": false}));
    obj.insert(
        "peer_addr".to_owned(),
        json!({"description": "The IP from which the data originated.", "required": false}),
    );
    obj.insert(
        "peer_path".to_owned(),
        json!({"description": "The pathname from which the data originated.", "required": false}),
    );
    obj.insert("mode".to_owned(), json!({"description": "The connection mode used by the component.", "required": false, "enum": {
        "udp":  "User Datagram Protocol",
        "tcp":  "Transmission Control Protocol",
        "unix": "Unix domain socket"
    }}));
    tags
});

/// Same tag set as `component_received_events_total` (inherited by byte-count metrics).
pub static COMPONENT_RECEIVED_EVENTS_TAGS: LazyLock<Value> =
    LazyLock::new(|| COMPONENT_RECEIVED_EVENTS_TOTAL_TAGS.clone());

pub static S3_OBJECT_PROCESSING_TAGS: LazyLock<Value> = LazyLock::new(|| {
    let mut tags = COMPONENT_TAGS.clone();
    tags.as_object_mut().unwrap().insert(
        "bucket".to_owned(),
        json!({"description": "The name of the S3 bucket.", "required": true}),
    );
    tags
});
