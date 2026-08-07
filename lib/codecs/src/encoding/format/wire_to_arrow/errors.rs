//! Error types for the streaming wire-to-Arrow encoder.

use snafu::Snafu;

/// Errors that can occur when building an encoding plan or encoding a batch.
#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
pub enum WireToArrowError {
    /// Required serializer-config field (`schema`) was not populated before
    /// `build()`. Sinks inject this at config build time.
    #[snafu(display("wire-to-Arrow serializer requires a {field}"))]
    ConfigurationMissing {
        /// Which config field was missing.
        field: &'static str,
    },

    /// Failed to load the proto descriptor from `desc_file` / `message_type`.
    #[snafu(display("failed to load proto descriptor: {message}"))]
    DescriptorLoad {
        /// The underlying error message from `get_message_descriptor`.
        message: String,
    },

    /// The batch had no events to encode.
    #[snafu(display("cannot encode an empty batch"))]
    NoEvents,

    /// An event in the batch had no `message` field.
    #[snafu(display("event is missing a `message` field"))]
    MessageBytesMissing,

    /// An event's `message` field was not a `Value::Bytes`.
    #[snafu(display("event `message` is not bytes-typed"))]
    MessageBytesWrongType,

    /// Proto descriptor is missing a field named in the Arrow schema.
    #[snafu(display("proto field '{name}' not found in descriptor"))]
    MissingProtoField {
        /// The Arrow field name that wasn't found in the proto descriptor.
        name: String,
    },

    /// The proto field's Kind cannot be represented by any supported scalar.
    #[snafu(display("unsupported proto kind for field '{name}': {kind}"))]
    UnsupportedKind {
        /// The proto field name.
        name: String,
        /// The proto kind that isn't supported.
        kind: String,
    },

    /// The combination of proto kind, Arrow type, and cardinality isn't supported.
    #[snafu(display(
        "field '{name}': unsupported combination \
         proto kind {kind} / arrow type {arrow_type} / repeated {repeated}"
    ))]
    UnsupportedCombination {
        /// The proto field name.
        name: String,
        /// The proto kind involved.
        kind: String,
        /// The Arrow data type involved.
        arrow_type: String,
        /// Whether the proto field was repeated.
        repeated: bool,
    },

    /// A repeated message field's Arrow element type isn't Struct.
    #[snafu(display(
        "field '{name}': repeated message requires List<Struct>, got List<{element}>"
    ))]
    RepeatedNonStructList {
        /// The proto field name.
        name: String,
        /// The Arrow list-element type that was found (expected Struct).
        element: String,
    },

    /// Ran out of wire bytes before finishing a tag/field.
    #[snafu(display("unexpected end of wire input"))]
    UnexpectedEof,

    /// Varint exceeded the max 10-byte encoding.
    #[snafu(display("varint exceeds 10 bytes"))]
    VarintOverflow,

    /// Unknown proto wire type (should be 0, 1, 2, or 5).
    #[snafu(display("invalid proto wire type {wire_type}"))]
    InvalidWireType {
        /// The unrecognized wire-type value from the tag.
        wire_type: u8,
    },

    /// Wire type for a field doesn't match the plan's expectation.
    #[snafu(display(
        "wire type mismatch: plan expected {expected}, wire bytes had {actual}"
    ))]
    WireTypeMismatch {
        /// The wire type the plan expected for this field.
        expected: u8,
        /// The wire type actually present in the bytes.
        actual: u8,
    },

    /// String field contained non-UTF-8 bytes.
    #[snafu(display("invalid UTF-8 in proto string field"))]
    InvalidUtf8,

    /// Plan and builder trees diverged during scan / finish, or a code
    /// path the encoder considers structurally impossible was reached.
    /// Always a code bug — never user input. `site` is a short label
    /// identifying which emit site fired so a bug report points at the
    /// right path without needing a backtrace.
    #[snafu(display("internal: plan/builder tree mismatch at {site}"))]
    PlanBuilderMismatch {
        /// Short label naming the emit site (e.g. `"scan_message"`,
        /// `"finish:absent_struct_non_struct_arrow"`). Free-form but
        /// expected to be a `&'static str` literal at the call site.
        site: &'static str,
    },

    /// `arrow::record_batch::RecordBatch::try_new` rejected the assembled arrays.
    #[snafu(display("failed to assemble RecordBatch: {source}"))]
    RecordBatchAssembly {
        /// The underlying Arrow error from `RecordBatch::try_new`.
        source: arrow::error::ArrowError,
    },

    /// `arrow::array::StructArray::try_new` / `ListArray::try_new` rejected
    /// the assembled arrays.
    #[snafu(display("failed to assemble {kind} array: {source}"))]
    ArrayAssembly {
        /// Which kind of array failed to assemble (e.g. "struct", "list").
        kind: &'static str,
        /// The underlying Arrow error from the array constructor.
        source: arrow::error::ArrowError,
    },

    /// A wire-parse error the encoder doesn't model as one of the
    /// variants above.
    #[snafu(display("wire parse error: {source}"))]
    ProtoParser {
        /// The underlying [`super::wire::ParseError`].
        source: super::wire::ParseError,
    },

    /// Plan-build recursion exceeded [`MAX_NESTING_DEPTH`]. Caps both the
    /// build-time walk over the Arrow schema and the scan-time walk over
    /// wire bytes (which can't recurse deeper than the plan).
    ///
    /// [`MAX_NESTING_DEPTH`]: super::plan::MAX_NESTING_DEPTH
    #[snafu(display(
        "wire-to-Arrow plan exceeds max nesting depth of {limit}"
    ))]
    SchemaTooDeep {
        /// The configured maximum depth.
        limit: usize,
    },

    /// The Arrow schema declares a primitive leaf type the encoder can't
    /// build a column for (e.g. `Date32`, `Time64`, decimal). Caught at
    /// plan-build so the failure surfaces at serializer init rather than
    /// panicking inside `TypedBuilder::new` on the first batch.
    #[snafu(display(
        "Arrow field '{name}' has unsupported leaf data type {arrow_type}"
    ))]
    UnsupportedArrowLeafType {
        /// The Arrow field name carrying the unsupported leaf type.
        name: String,
        /// The unsupported Arrow data type (debug form).
        arrow_type: String,
    },

    /// A repeated-list slot's running offset would overflow `i32`.
    ///
    /// Arrow's `ListArray` / `MapArray` use `i32` offsets, so the cumulative
    /// element count across all rows in a batch is capped at `i32::MAX`
    /// (~2.1B). The encoder normally bumps the counter via `+= 1` inside
    /// `scan_message` / `append_repeated_scalar`; without bounds checking,
    /// release-mode wrap-around silently produces a non-monotonic offsets
    /// buffer and `OffsetBuffer::new` asserts at batch finish, taking the
    /// whole process down.
    ///
    /// Two places guard against this:
    /// - `validate_message`'s packed-scalar count drops a single row whose
    ///   own delta would already exceed `i32::MAX` (per-row isolation).
    /// - The runtime appenders use `checked_add` and surface this variant if
    ///   the cumulative count (across rows + sub-rows in the batch) would
    ///   wrap. That path fails the batch cleanly with a structured error
    ///   rather than panicking the process — adversarial wire bytes can no
    ///   longer crash the encoder regardless of how many rows they span.
    #[snafu(display("repeated-list offset would overflow i32 at {site}"))]
    OffsetOverflow {
        /// Short label naming the site that detected the overflow (e.g.
        /// `"scan_message:repeated_message"`, `"append_repeated_scalar:packed"`).
        site: &'static str,
    },

    /// A singular (non-repeated) proto field appeared more than once in a
    /// single message. Proto3 parsers must accept this (last-wins for
    /// scalars, merge for sub-messages), but the encoder appends to
    /// Arrow column builders on every occurrence, which would diverge
    /// column lengths and fail batch assembly. Surfaced from
    /// `validate_message` so the offending row is dropped via the normal
    /// per-row isolation path instead of poisoning the whole batch.
    #[snafu(display(
        "duplicate singular proto field {field_number} in one message"
    ))]
    DuplicateSingularField {
        /// The proto field number whose tag appeared more than once.
        field_number: u32,
    },

    /// Arrow Map's entry struct declares a field name that the proto
    /// MapEntry descriptor doesn't carry.
    ///
    /// Proto `map<K, V>` is generated as a `MapEntry` message with fields
    /// named `key` (1) and `value` (2). Arrow's Map type doesn't enforce
    /// these names on its inner Struct, so a user-supplied schema can
    /// declare `Map<Struct(k, v)>` and have it pass type checking — but
    /// there is no proto field for the encoder to read into the slot, the
    /// `MapEntry`'s key non-null contract still applies, and the
    /// absent-padding path can't honor both. Reject the mismatch at
    /// plan-build so it surfaces clearly at sink init rather than as a
    /// runtime panic when the proto3-default helper meets a kind/builder
    /// pair it can't pad.
    #[snafu(display(
        "Arrow Map entry has field '{name}' which is not declared in the proto MapEntry; \
         entry struct field names must match proto MapEntry's 'key' and 'value'"
    ))]
    MapEntryFieldNotInProto {
        /// The Arrow Map entry field name that doesn't match proto MapEntry.
        name: String,
    },

    /// The Arrow schema declares a singular column as non-nullable, but the
    /// encoder cannot guarantee a value will be present on every row.
    ///
    /// Proto3 omits default-valued singular fields on the wire, so the
    /// encoder writes a null whenever a tag is absent. A non-nullable
    /// declaration would then trip a generic `RecordBatch::try_new`
    /// "non-nullable contains nulls" failure deep in `encode_batch`,
    /// dropping the entire batch with no row context. We reject the
    /// mismatch at plan-build time so it surfaces clearly at serializer
    /// init.
    ///
    /// `List<…>` and `Map<…>` outer columns are exempt: the encoder always
    /// emits at least an empty list / empty map per row, so the outer
    /// column never contains a null.
    #[snafu(display(
        "Arrow field '{name}' is declared non-nullable but {reason}; \
         declare the column nullable in the schema or change the proto"
    ))]
    NonNullableNotGuaranteed {
        /// The Arrow field name.
        name: String,
        /// Short explanation of why the encoder can't guarantee non-null
        /// (e.g. "proto3 singular fields are omitted at default value").
        reason: &'static str,
    },
}

/// Result alias for wire-to-Arrow encoder operations.
pub type Result<T> = std::result::Result<T, WireToArrowError>;

impl From<super::wire::ParseError> for WireToArrowError {
    /// Collapse most [`super::wire::ParseError`]s onto this crate's pre-existing
    /// variants; the long tail falls through into [`WireToArrowError::ProtoParser`].
    fn from(err: super::wire::ParseError) -> Self {
        use super::wire::ParseError;
        match err {
            ParseError::TruncatedVarint | ParseError::BufferTooShort { .. } => {
                WireToArrowError::UnexpectedEof
            }
            ParseError::VarintTooLong => WireToArrowError::VarintOverflow,
            ParseError::InvalidWireType(wt) => WireToArrowError::InvalidWireType { wire_type: wt },
            _ => WireToArrowError::ProtoParser { source: err },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::wire::ParseError;

    #[test]
    fn error_display_is_informative() {
        let e = WireToArrowError::MissingProtoField {
            name: "foo".to_string(),
        };
        let msg = format!("{e}");
        assert!(msg.contains("foo"), "display must contain field name: {msg}");
    }

    #[test]
    fn wire_type_mismatch_displays_both_numbers() {
        let e = WireToArrowError::WireTypeMismatch {
            expected: 2,
            actual: 0,
        };
        let msg = format!("{e}");
        assert!(msg.contains("2") && msg.contains("0"), "got: {msg}");
    }

    #[test]
    fn from_parse_error_truncated_varint() {
        assert!(matches!(
            WireToArrowError::from(ParseError::TruncatedVarint),
            WireToArrowError::UnexpectedEof
        ));
    }

    #[test]
    fn from_parse_error_invalid_wire_type() {
        assert!(matches!(
            WireToArrowError::from(ParseError::InvalidWireType(7)),
            WireToArrowError::InvalidWireType { wire_type: 7 }
        ));
    }
}
