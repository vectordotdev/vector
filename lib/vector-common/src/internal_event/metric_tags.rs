use serde::Serialize;

/// A complete tag set for a Vector internal metric, represented as structured
/// data that is serialized to a CUE expression at schema-generation time.
///
/// Use the pre-defined constants (`COMPONENT_TAGS`, `COMPONENT_TAGS_OUTPUT`,
/// etc.) or construct one inline for metric-specific tag shapes.
#[derive(Clone, Copy)]
pub struct TagSet {
    /// Base CUE tag group (e.g. `"_component_tags"`).  `None` means an empty
    /// base (`{}`).
    pub base: Option<&'static str>,
    /// Extra fields merged into the base with `&`.
    pub extra: &'static [ExtraTag],
}

/// A single extra tag field appended to a [`TagSet`].
#[derive(Clone, Copy)]
pub struct ExtraTag {
    pub name: &'static str,
    pub field: TagField,
}

/// Definition of one tag field.
#[derive(Clone, Copy)]
pub enum TagField {
    /// Reference to an existing CUE tag helper variable (e.g. `_output`).
    Ref(&'static str),
    /// Inline field definition with an optional fixed-value enum.
    Inline {
        description: &'static str,
        required: bool,
        enum_values: Option<&'static [(&'static str, &'static str)]>,
    },
}

impl TagSet {
    fn format_extras(self) -> Option<String> {
        if self.extra.is_empty() {
            return None;
        }
        let parts: Vec<String> = self
            .extra
            .iter()
            .map(|et| {
                let field = match et.field {
                    TagField::Ref(r) => r.to_owned(),
                    TagField::Inline {
                        description,
                        required,
                        enum_values: None,
                    } => format!(r#"{{description: "{description}", required: {required}}}"#),
                    TagField::Inline {
                        description,
                        required,
                        enum_values: Some(enums),
                    } => {
                        let pairs: Vec<String> =
                            enums.iter().map(|(k, v)| format!("{k}: \"{v}\"")).collect();
                        format!(
                            r#"{{description: "{description}", required: {required}, enum: {{{}}}}}"#,
                            pairs.join(", ")
                        )
                    }
                };
                format!("{}: {field}", et.name)
            })
            .collect();
        Some(parts.join(", "))
    }

    /// Renders the tag set as a CUE expression (emitted verbatim by vdev).
    #[must_use]
    pub fn to_cue(self) -> String {
        match (self.base, self.format_extras().as_deref()) {
            (None, None) => "{}".to_owned(),
            (Some(base), None) => base.to_owned(),
            (None, Some(ex)) => format!("{{{ex}}}"),
            (Some(base), Some(ex)) => format!("{base} & {{{ex}}}"),
        }
    }
}

impl Serialize for TagSet {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_cue())
    }
}

// ─── Common base tag sets ──────────────────────────────────────────────────────

pub const COMPONENT_TAGS: TagSet = TagSet {
    base: Some("_component_tags"),
    extra: &[],
};

pub const INTERNAL_METRICS_TAGS: TagSet = TagSet {
    base: Some("_internal_metrics_tags"),
    extra: &[],
};

pub const EMPTY_TAGS: TagSet = TagSet {
    base: None,
    extra: &[],
};

// ─── Cross-reference ──────────────────────────────────────────────────────────

/// Inherits the tag set defined on `component_received_events_total`.
pub const COMPONENT_RECEIVED_EVENTS_TAGS: TagSet = TagSet {
    base: Some("component_received_events_total.tags"),
    extra: &[],
};

// ─── Component tags with a single CUE-helper extra field ──────────────────────

pub const COMPONENT_TAGS_OUTPUT: TagSet = TagSet {
    base: Some("_component_tags"),
    extra: &[ExtraTag { name: "output", field: TagField::Ref("_output") }],
};

pub const INTERNAL_METRICS_TAGS_FILE: TagSet = TagSet {
    base: Some("_internal_metrics_tags"),
    extra: &[ExtraTag { name: "file", field: TagField::Ref("_file") }],
};

pub const INTERNAL_METRICS_TAGS_REASON: TagSet = TagSet {
    base: Some("_internal_metrics_tags"),
    extra: &[ExtraTag { name: "reason", field: TagField::Ref("_reason") }],
};

pub const COMPONENT_TAGS_ERROR_TYPE_STAGE: TagSet = TagSet {
    base: Some("_component_tags"),
    extra: &[
        ExtraTag { name: "error_type", field: TagField::Ref("_error_type") },
        ExtraTag { name: "stage", field: TagField::Ref("_stage") },
    ],
};

// ─── gRPC tag sets ────────────────────────────────────────────────────────────

pub const COMPONENT_TAGS_GRPC_METHOD_SERVICE: TagSet = TagSet {
    base: Some("_component_tags"),
    extra: &[
        ExtraTag { name: "grpc_method", field: TagField::Ref("_grpc_method") },
        ExtraTag { name: "grpc_service", field: TagField::Ref("_grpc_service") },
    ],
};

pub const COMPONENT_TAGS_GRPC_ALL: TagSet = TagSet {
    base: Some("_component_tags"),
    extra: &[
        ExtraTag { name: "grpc_method", field: TagField::Ref("_grpc_method") },
        ExtraTag { name: "grpc_service", field: TagField::Ref("_grpc_service") },
        ExtraTag { name: "grpc_status", field: TagField::Ref("_grpc_status") },
    ],
};

// ─── HTTP tag sets ────────────────────────────────────────────────────────────

pub const COMPONENT_TAGS_HTTP_METHOD: TagSet = TagSet {
    base: Some("_component_tags"),
    extra: &[ExtraTag { name: "method", field: TagField::Ref("_method") }],
};

pub const COMPONENT_TAGS_HTTP_STATUS: TagSet = TagSet {
    base: Some("_component_tags"),
    extra: &[ExtraTag { name: "status", field: TagField::Ref("_status") }],
};

pub const COMPONENT_TAGS_HTTP_METHOD_PATH: TagSet = TagSet {
    base: Some("_component_tags"),
    extra: &[
        ExtraTag { name: "method", field: TagField::Ref("_method") },
        ExtraTag { name: "path", field: TagField::Ref("_path") },
    ],
};

pub const COMPONENT_TAGS_HTTP_ALL: TagSet = TagSet {
    base: Some("_component_tags"),
    extra: &[
        ExtraTag { name: "method", field: TagField::Ref("_method") },
        ExtraTag { name: "path", field: TagField::Ref("_path") },
        ExtraTag { name: "status", field: TagField::Ref("_status") },
    ],
};

// ─── Metric-specific tag sets ─────────────────────────────────────────────────

pub const COMPONENT_RECEIVED_EVENTS_TOTAL_TAGS: TagSet = TagSet {
    base: Some("_component_tags"),
    extra: &[
        ExtraTag {
            name: "file",
            field: TagField::Inline {
                description: "The file from which the data originated.",
                required: false,
                enum_values: None,
            },
        },
        ExtraTag {
            name: "uri",
            field: TagField::Inline {
                description: "The sanitized URI from which the data originated.",
                required: false,
                enum_values: None,
            },
        },
        ExtraTag {
            name: "container_name",
            field: TagField::Inline {
                description: "The name of the container from which the data originated.",
                required: false,
                enum_values: None,
            },
        },
        ExtraTag {
            name: "pod_name",
            field: TagField::Inline {
                description: "The name of the pod from which the data originated.",
                required: false,
                enum_values: None,
            },
        },
        ExtraTag {
            name: "peer_addr",
            field: TagField::Inline {
                description: "The IP from which the data originated.",
                required: false,
                enum_values: None,
            },
        },
        ExtraTag {
            name: "peer_path",
            field: TagField::Inline {
                description: "The pathname from which the data originated.",
                required: false,
                enum_values: None,
            },
        },
        ExtraTag { name: "mode", field: TagField::Ref("_mode") },
    ],
};

pub const COMPONENT_DISCARDED_EVENTS_TOTAL_TAGS: TagSet = TagSet {
    base: Some("_component_tags"),
    extra: &[ExtraTag {
        name: "intentional",
        field: TagField::Inline {
            description: "True if the events were discarded intentionally, like a `filter` transform, or false if due to an error.",
            required: true,
            enum_values: None,
        },
    }],
};

pub const COMPONENT_SENT_BYTES_TOTAL_TAGS: TagSet = TagSet {
    base: Some("_component_tags"),
    extra: &[
        ExtraTag {
            name: "endpoint",
            field: TagField::Inline {
                description: "The endpoint to which the bytes were sent. For HTTP, this will be the host and path only, excluding the query string.",
                required: false,
                enum_values: None,
            },
        },
        ExtraTag {
            name: "file",
            field: TagField::Inline {
                description: "The absolute path of the destination file.",
                required: false,
                enum_values: None,
            },
        },
        ExtraTag {
            name: "protocol",
            field: TagField::Inline {
                description: "The protocol used to send the bytes.",
                required: true,
                enum_values: None,
            },
        },
        ExtraTag {
            name: "region",
            field: TagField::Inline {
                description: "The AWS region name to which the bytes were sent. In some configurations, this may be a literal hostname.",
                required: false,
                enum_values: None,
            },
        },
    ],
};

pub const S3_OBJECT_PROCESSING_TAGS: TagSet = TagSet {
    base: Some("_component_tags"),
    extra: &[ExtraTag {
        name: "bucket",
        field: TagField::Inline {
            description: "The name of the S3 bucket.",
            required: true,
            enum_values: None,
        },
    }],
};

pub const KAFKA_CONSUMER_LAG_TAGS: TagSet = TagSet {
    base: Some("_component_tags"),
    extra: &[
        ExtraTag {
            name: "topic_id",
            field: TagField::Inline {
                description: "The Kafka topic id.",
                required: true,
                enum_values: None,
            },
        },
        ExtraTag {
            name: "partition_id",
            field: TagField::Inline {
                description: "The Kafka partition id.",
                required: true,
                enum_values: None,
            },
        },
    ],
};

pub const TAG_VALUE_LIMIT_EXCEEDED_TOTAL_TAGS: TagSet = TagSet {
    base: Some("_component_tags"),
    extra: &[
        ExtraTag {
            name: "metric_name",
            field: TagField::Inline {
                description: "The name of the metric whose tag value limit was exceeded. Only present when `internal_metrics.include_extended_tags` is enabled.",
                required: false,
                enum_values: None,
            },
        },
        ExtraTag {
            name: "tag_key",
            field: TagField::Inline {
                description: "The key of the tag whose value limit was exceeded. Only present when `internal_metrics.include_extended_tags` is enabled.",
                required: false,
                enum_values: None,
            },
        },
    ],
};

pub const SQS_S3_IGNORED_TOTAL_TAGS: TagSet = TagSet {
    base: Some("_component_tags"),
    extra: &[ExtraTag {
        name: "ignore_type",
        field: TagField::Inline {
            description: "The reason for ignoring the S3 record",
            required: true,
            enum_values: Some(&[("invalid_event_kind", "The kind of invalid event.")]),
        },
    }],
};

pub const BUILD_INFO_TAGS: TagSet = TagSet {
    base: Some("_internal_metrics_tags"),
    extra: &[
        ExtraTag {
            name: "debug",
            field: TagField::Inline {
                description: "Whether this is a debug build of Vector",
                required: true,
                enum_values: None,
            },
        },
        ExtraTag {
            name: "version",
            field: TagField::Inline {
                description: "Vector version.",
                required: true,
                enum_values: None,
            },
        },
        ExtraTag {
            name: "rust_version",
            field: TagField::Inline {
                description: "The Rust version from the package manifest.",
                required: true,
                enum_values: None,
            },
        },
        ExtraTag {
            name: "arch",
            field: TagField::Inline {
                description: "The target architecture being compiled for. (e.g. x86_64)",
                required: true,
                enum_values: None,
            },
        },
        ExtraTag {
            name: "revision",
            field: TagField::Inline {
                description: "Revision identifer, related to versioned releases.",
                required: true,
                enum_values: None,
            },
        },
    ],
};

pub const CONNECTION_READ_ERRORS_TOTAL_TAGS: TagSet = TagSet {
    base: Some("_component_tags"),
    extra: &[ExtraTag {
        name: "mode",
        field: TagField::Inline {
            description: "",
            required: true,
            enum_values: Some(&[("udp", "User Datagram Protocol")]),
        },
    }],
};

pub const UTF8_CONVERT_ERRORS_TOTAL_TAGS: TagSet = TagSet {
    base: Some("_component_tags"),
    extra: &[ExtraTag {
        name: "mode",
        field: TagField::Inline {
            description: "The connection mode used by the component.",
            required: true,
            enum_values: Some(&[("udp", "User Datagram Protocol")]),
        },
    }],
};
