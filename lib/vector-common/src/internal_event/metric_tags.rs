use serde_json::{Value, json};
use std::sync::LazyLock;

/// A compile-time handle to a lazily-initialized JSON tag-set.
///
/// Holds a function pointer so it can be used as a `const` in
/// `#[configurable(metadata(docs::tags = MY_TAGS))]` attributes.
pub struct TagSet(pub fn() -> &'static Value);

impl serde::Serialize for TagSet {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        (self.0)().serialize(s)
    }
}

// ─── Base tag groups ───────────────────────────────────────────────────────────

fn internal_metrics_tags_value() -> &'static Value {
    static V: LazyLock<Value> = LazyLock::new(|| {
        json!({
            "pid":  {"description": "The process ID of the Vector instance.", "required": false},
            "host": {"description": "The hostname of the system Vector is running on.", "required": false}
        })
    });
    &V
}

fn component_tags_value() -> &'static Value {
    static V: LazyLock<Value> = LazyLock::new(|| {
        let mut tags = internal_metrics_tags_value().clone();
        let obj = tags.as_object_mut().unwrap();
        obj.insert(
            "component_kind".to_owned(),
            json!({
                "description": "The Vector component kind.",
                "required": true,
                "enum": {
                    "sink":      "Vector sink components",
                    "source":    "Vector source components",
                    "transform": "Vector transform components"
                }
            }),
        );
        obj.insert("component_id".to_owned(),   json!({"description": "The Vector component ID.", "required": true}));
        obj.insert("component_type".to_owned(), json!({"description": "The Vector component type.", "required": true}));
        tags
    });
    &V
}

fn empty_tags_value() -> &'static Value {
    static V: LazyLock<Value> = LazyLock::new(|| json!({}));
    &V
}

// ─── Extensions of COMPONENT_TAGS ─────────────────────────────────────────────

fn component_tags_output_value() -> &'static Value {
    static V: LazyLock<Value> = LazyLock::new(|| {
        let mut tags = component_tags_value().clone();
        tags.as_object_mut().unwrap()
            .insert("output".to_owned(), json!({"description": "The specific output of the component.", "required": false}));
        tags
    });
    &V
}

fn component_tags_grpc_method_service_value() -> &'static Value {
    static V: LazyLock<Value> = LazyLock::new(|| {
        let mut tags = component_tags_value().clone();
        let obj = tags.as_object_mut().unwrap();
        obj.insert("grpc_method".to_owned(),  json!({"description": "The name of the method called on the gRPC service.", "required": true}));
        obj.insert("grpc_service".to_owned(), json!({"description": "The gRPC service name.", "required": true}));
        tags
    });
    &V
}

fn component_tags_grpc_all_value() -> &'static Value {
    static V: LazyLock<Value> = LazyLock::new(|| {
        let mut tags = component_tags_grpc_method_service_value().clone();
        tags.as_object_mut().unwrap()
            .insert("grpc_status".to_owned(), json!({"description": "The human-readable gRPC status code.", "required": true}));
        tags
    });
    &V
}

fn component_tags_http_method_value() -> &'static Value {
    static V: LazyLock<Value> = LazyLock::new(|| {
        let mut tags = component_tags_value().clone();
        tags.as_object_mut().unwrap()
            .insert("method".to_owned(), json!({"description": "The HTTP method of the request.", "required": false}));
        tags
    });
    &V
}

fn component_tags_http_status_value() -> &'static Value {
    static V: LazyLock<Value> = LazyLock::new(|| {
        let mut tags = component_tags_value().clone();
        tags.as_object_mut().unwrap()
            .insert("status".to_owned(), json!({"description": "The HTTP status code of the request.", "required": false}));
        tags
    });
    &V
}

fn component_tags_http_method_path_value() -> &'static Value {
    static V: LazyLock<Value> = LazyLock::new(|| {
        let mut tags = component_tags_http_method_value().clone();
        tags.as_object_mut().unwrap()
            .insert("path".to_owned(), json!({"description": "The path that produced the error.", "required": true}));
        tags
    });
    &V
}

fn component_tags_http_all_value() -> &'static Value {
    static V: LazyLock<Value> = LazyLock::new(|| {
        let mut tags = component_tags_http_method_path_value().clone();
        tags.as_object_mut().unwrap()
            .insert("status".to_owned(), json!({"description": "The HTTP status code of the request.", "required": false}));
        tags
    });
    &V
}

fn component_tags_error_type_stage_value() -> &'static Value {
    static V: LazyLock<Value> = LazyLock::new(|| {
        let mut tags = component_tags_value().clone();
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
    &V
}

// ─── Extensions of INTERNAL_METRICS_TAGS ──────────────────────────────────────

fn internal_metrics_tags_file_value() -> &'static Value {
    static V: LazyLock<Value> = LazyLock::new(|| {
        let mut tags = internal_metrics_tags_value().clone();
        tags.as_object_mut().unwrap()
            .insert("file".to_owned(), json!({"description": "The file that produced the error.", "required": false}));
        tags
    });
    &V
}

fn internal_metrics_tags_reason_value() -> &'static Value {
    static V: LazyLock<Value> = LazyLock::new(|| {
        let mut tags = internal_metrics_tags_value().clone();
        tags.as_object_mut().unwrap()
            .insert("reason".to_owned(), json!({"description": "The type of the error", "required": true, "enum": {
                "out_of_order": "The event was out of order.",
                "oversized":    "The event was too large."
            }}));
        tags
    });
    &V
}

// ─── Metric-specific tag sets ─────────────────────────────────────────────────

fn component_received_events_total_tags_value() -> &'static Value {
    static V: LazyLock<Value> = LazyLock::new(|| {
        let mut tags = component_tags_value().clone();
        let obj = tags.as_object_mut().unwrap();
        obj.insert("file".to_owned(),           json!({"description": "The file from which the data originated.", "required": false}));
        obj.insert("uri".to_owned(),            json!({"description": "The sanitized URI from which the data originated.", "required": false}));
        obj.insert("container_name".to_owned(), json!({"description": "The name of the container from which the data originated.", "required": false}));
        obj.insert("pod_name".to_owned(),       json!({"description": "The name of the pod from which the data originated.", "required": false}));
        obj.insert("peer_addr".to_owned(),      json!({"description": "The IP from which the data originated.", "required": false}));
        obj.insert("peer_path".to_owned(),      json!({"description": "The pathname from which the data originated.", "required": false}));
        obj.insert("mode".to_owned(),           json!({"description": "The connection mode used by the component.", "required": false, "enum": {
            "udp":  "User Datagram Protocol",
            "tcp":  "Transmission Control Protocol",
            "unix": "Unix domain socket"
        }}));
        tags
    });
    &V
}

fn component_discarded_events_total_tags_value() -> &'static Value {
    static V: LazyLock<Value> = LazyLock::new(|| {
        let mut tags = component_tags_value().clone();
        tags.as_object_mut().unwrap().insert("intentional".to_owned(), json!({
            "description": "True if the events were discarded intentionally, like a `filter` transform, or false if due to an error.",
            "required": true
        }));
        tags
    });
    &V
}

fn component_sent_bytes_total_tags_value() -> &'static Value {
    static V: LazyLock<Value> = LazyLock::new(|| {
        let mut tags = component_tags_value().clone();
        let obj = tags.as_object_mut().unwrap();
        obj.insert("endpoint".to_owned(), json!({"description": "The endpoint to which the bytes were sent. For HTTP, this will be the host and path only, excluding the query string.", "required": false}));
        obj.insert("file".to_owned(),     json!({"description": "The absolute path of the destination file.", "required": false}));
        obj.insert("protocol".to_owned(), json!({"description": "The protocol used to send the bytes.", "required": true}));
        obj.insert("region".to_owned(),   json!({"description": "The AWS region name to which the bytes were sent. In some configurations, this may be a literal hostname.", "required": false}));
        tags
    });
    &V
}

fn s3_object_processing_tags_value() -> &'static Value {
    static V: LazyLock<Value> = LazyLock::new(|| {
        let mut tags = component_tags_value().clone();
        tags.as_object_mut().unwrap()
            .insert("bucket".to_owned(), json!({"description": "The name of the S3 bucket.", "required": true}));
        tags
    });
    &V
}

fn kafka_consumer_lag_tags_value() -> &'static Value {
    static V: LazyLock<Value> = LazyLock::new(|| {
        let mut tags = component_tags_value().clone();
        let obj = tags.as_object_mut().unwrap();
        obj.insert("topic_id".to_owned(),     json!({"description": "The Kafka topic id.", "required": true}));
        obj.insert("partition_id".to_owned(), json!({"description": "The Kafka partition id.", "required": true}));
        tags
    });
    &V
}

fn tag_value_limit_exceeded_total_tags_value() -> &'static Value {
    static V: LazyLock<Value> = LazyLock::new(|| {
        let mut tags = component_tags_value().clone();
        let obj = tags.as_object_mut().unwrap();
        obj.insert("metric_name".to_owned(), json!({"description": "The name of the metric whose tag value limit was exceeded. Only present when `internal_metrics.include_extended_tags` is enabled.", "required": false}));
        obj.insert("tag_key".to_owned(),     json!({"description": "The key of the tag whose value limit was exceeded. Only present when `internal_metrics.include_extended_tags` is enabled.", "required": false}));
        tags
    });
    &V
}

fn sqs_s3_ignored_total_tags_value() -> &'static Value {
    static V: LazyLock<Value> = LazyLock::new(|| {
        let mut tags = component_tags_value().clone();
        tags.as_object_mut().unwrap().insert("ignore_type".to_owned(), json!({
            "description": "The reason for ignoring the S3 record",
            "required": true,
            "enum": {"invalid_event_kind": "The kind of invalid event."}
        }));
        tags
    });
    &V
}

fn build_info_tags_value() -> &'static Value {
    static V: LazyLock<Value> = LazyLock::new(|| {
        let mut tags = internal_metrics_tags_value().clone();
        let obj = tags.as_object_mut().unwrap();
        obj.insert("debug".to_owned(),        json!({"description": "Whether this is a debug build of Vector", "required": true}));
        obj.insert("version".to_owned(),      json!({"description": "Vector version.", "required": true}));
        obj.insert("rust_version".to_owned(), json!({"description": "The Rust version from the package manifest.", "required": true}));
        obj.insert("arch".to_owned(),         json!({"description": "The target architecture being compiled for. (e.g. x86_64)", "required": true}));
        obj.insert("revision".to_owned(),     json!({"description": "Revision identifer, related to versioned releases.", "required": true}));
        tags
    });
    &V
}

fn connection_read_errors_total_tags_value() -> &'static Value {
    static V: LazyLock<Value> = LazyLock::new(|| {
        let mut tags = component_tags_value().clone();
        tags.as_object_mut().unwrap().insert("mode".to_owned(), json!({
            "description": "",
            "required": true,
            "enum": {"udp": "User Datagram Protocol"}
        }));
        tags
    });
    &V
}

fn utf8_convert_errors_total_tags_value() -> &'static Value {
    static V: LazyLock<Value> = LazyLock::new(|| {
        let mut tags = component_tags_value().clone();
        tags.as_object_mut().unwrap().insert("mode".to_owned(), json!({
            "description": "The connection mode used by the component.",
            "required": true,
            "enum": {"udp": "User Datagram Protocol"}
        }));
        tags
    });
    &V
}

// ─── Public constants ──────────────────────────────────────────────────────────

pub const INTERNAL_METRICS_TAGS: TagSet = TagSet(internal_metrics_tags_value);
pub const COMPONENT_TAGS: TagSet = TagSet(component_tags_value);
pub const EMPTY_TAGS: TagSet = TagSet(empty_tags_value);

/// Same tag set as `component_received_events_total` (inherited by byte-count metrics).
pub const COMPONENT_RECEIVED_EVENTS_TAGS: TagSet =
    TagSet(component_received_events_total_tags_value);

pub const COMPONENT_TAGS_OUTPUT: TagSet = TagSet(component_tags_output_value);
pub const COMPONENT_TAGS_GRPC_METHOD_SERVICE: TagSet =
    TagSet(component_tags_grpc_method_service_value);
pub const COMPONENT_TAGS_GRPC_ALL: TagSet = TagSet(component_tags_grpc_all_value);
pub const COMPONENT_TAGS_HTTP_METHOD: TagSet = TagSet(component_tags_http_method_value);
pub const COMPONENT_TAGS_HTTP_STATUS: TagSet = TagSet(component_tags_http_status_value);
pub const COMPONENT_TAGS_HTTP_METHOD_PATH: TagSet = TagSet(component_tags_http_method_path_value);
pub const COMPONENT_TAGS_HTTP_ALL: TagSet = TagSet(component_tags_http_all_value);
pub const COMPONENT_TAGS_ERROR_TYPE_STAGE: TagSet = TagSet(component_tags_error_type_stage_value);
pub const INTERNAL_METRICS_TAGS_FILE: TagSet = TagSet(internal_metrics_tags_file_value);
pub const INTERNAL_METRICS_TAGS_REASON: TagSet = TagSet(internal_metrics_tags_reason_value);
pub const COMPONENT_RECEIVED_EVENTS_TOTAL_TAGS: TagSet =
    TagSet(component_received_events_total_tags_value);
pub const COMPONENT_DISCARDED_EVENTS_TOTAL_TAGS: TagSet =
    TagSet(component_discarded_events_total_tags_value);
pub const COMPONENT_SENT_BYTES_TOTAL_TAGS: TagSet = TagSet(component_sent_bytes_total_tags_value);
pub const S3_OBJECT_PROCESSING_TAGS: TagSet = TagSet(s3_object_processing_tags_value);
pub const KAFKA_CONSUMER_LAG_TAGS: TagSet = TagSet(kafka_consumer_lag_tags_value);
pub const TAG_VALUE_LIMIT_EXCEEDED_TOTAL_TAGS: TagSet =
    TagSet(tag_value_limit_exceeded_total_tags_value);
pub const SQS_S3_IGNORED_TOTAL_TAGS: TagSet = TagSet(sqs_s3_ignored_total_tags_value);
pub const BUILD_INFO_TAGS: TagSet = TagSet(build_info_tags_value);
pub const CONNECTION_READ_ERRORS_TOTAL_TAGS: TagSet =
    TagSet(connection_read_errors_total_tags_value);
pub const UTF8_CONVERT_ERRORS_TOTAL_TAGS: TagSet = TagSet(utf8_convert_errors_total_tags_value);
