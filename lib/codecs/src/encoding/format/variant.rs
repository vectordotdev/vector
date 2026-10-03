//! Helpers for encoding Parquet Variant columns into the Arrow JSON decoder's
//! input.
//!
//! A Parquet Variant column is an Arrow `Struct<metadata: LargeBinary, value:
//! LargeBinary>` carrying the canonical `arrow.parquet.variant` extension marker
//! (Arrow's `VariantType::NAME`). The Arrow JSON decoder cannot build that binary
//! form from a plain JSON object, so before decoding we rewrite each
//! variant-marked field's value into `{"metadata": "<hex>", "value": "<hex>"}` —
//! the Parquet Variant binary encoded as hex, which the decoder ingests natively
//! as `LargeBinary`. The rewrite recurses through struct/list/map, so nested
//! variants work too.
//!
//! A value is encoded by the shape it already has: a JSON object/array/scalar
//! becomes the matching Variant, and a string stays a Variant string — a string
//! that happens to hold JSON text is not re-parsed into structure. Parse such a
//! field upstream so it arrives structured if a structured Variant is wanted.
//!
//! The variant-bearing paths are resolved once per batch into a [`VariantPlan`]
//! (built from the schema), then applied to every row — so per-row work is
//! proportional to the variant columns, not the whole schema, and no marker
//! lookup happens per row.

use arrow::datatypes::{DataType, Field, Schema};
use arrow::error::ArrowError;
use parquet_variant::VariantBuilder;
use serde_json::Value;

/// Arrow extension name marking a Parquet Variant field (Arrow's
/// `VariantType::NAME`).
const VARIANT_EXTENSION_NAME: &str = "arrow.parquet.variant";

/// Arrow's `EXTENSION_TYPE_NAME_KEY`. Spelled out to avoid depending on the
/// exact re-export path across arrow versions.
const EXTENSION_TYPE_NAME_KEY: &str = "ARROW:extension:name";

/// True iff `field` carries the `arrow.parquet.variant` extension marker.
fn is_variant_field(field: &Field) -> bool {
    field
        .metadata()
        .get(EXTENSION_TYPE_NAME_KEY)
        .map(String::as_str)
        == Some(VARIANT_EXTENSION_NAME)
}

/// Encode a JSON value into the Parquet Variant `(metadata, value)` binary pair.
///
/// Source values arrive as `serde_json::Value`, so temporal/binary values take
/// their JSON forms rather than native Variant temporal/binary types.
fn value_to_variant(json: &Value) -> Result<(Vec<u8>, Vec<u8>), ArrowError> {
    let mut builder = VariantBuilder::new();
    parquet_variant_json::append_json(json, &mut builder)?;
    Ok(builder.finish())
}

/// Precomputed description of every variant-bearing path in a schema. Built once
/// per batch (see [`VariantPlan::build`]) and applied to each row, so a field
/// that carries no variant at any depth is never revisited per row and no
/// [`is_variant_field`] lookup happens on the hot path. Built once when the
/// schema is known and reused across batches; only a schema change requires
/// rebuilding it.
#[derive(Debug)]
pub(crate) struct VariantPlan(Vec<(String, FieldPlan)>);

/// How to rewrite one field's value to expose its variant(s). Only built for
/// fields that actually carry a variant somewhere.
#[derive(Debug)]
enum FieldPlan {
    /// The field is variant-marked: encode its value into `{metadata, value}`.
    Encode,
    /// Recurse into the named struct children that bear a variant.
    Struct(Vec<(String, FieldPlan)>),
    /// Recurse into every list element.
    List(Box<FieldPlan>),
    /// Recurse into every map value.
    Map(Box<FieldPlan>),
}

impl VariantPlan {
    /// Walk `schema` once, keeping only the fields that carry a variant at some
    /// depth. Empty when the schema has no variant column, which lets the caller
    /// skip the row rewrite entirely.
    pub(crate) fn build(schema: &Schema) -> Self {
        let fields = schema
            .fields()
            .iter()
            .filter_map(|f| field_plan(f).map(|p| (f.name().clone(), p)))
            .collect();
        Self(fields)
    }

    /// True when no field carries a variant — nothing to rewrite.
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Rewrite the variant-bearing fields of every row in place.
    pub(crate) fn apply(&self, values: &mut [Value]) -> Result<(), ArrowError> {
        for row in values {
            if let Value::Object(map) = row {
                for (name, plan) in &self.0 {
                    if let Some(v) = map.get_mut(name) {
                        plan.apply(v)?;
                    }
                }
            }
        }
        Ok(())
    }
}

impl FieldPlan {
    /// Rewrite `value` in place per this plan. Struct/list/map recurse into
    /// their children; `Encode` turns a (non-null) value into the
    /// `{"metadata": hex, "value": hex}` shape the Arrow JSON decoder ingests.
    fn apply(&self, value: &mut Value) -> Result<(), ArrowError> {
        match self {
            FieldPlan::Encode => {
                // Null / absent stays null (the decoder yields a null struct
                // row); only a present value is encoded into the binary pair.
                if !value.is_null() {
                    let (metadata, val) = value_to_variant(value)?;
                    *value = serde_json::json!({
                        "metadata": hex::encode(&metadata),
                        "value": hex::encode(&val),
                    });
                }
            }
            FieldPlan::Struct(children) => {
                if let Value::Object(map) = value {
                    for (name, child) in children {
                        if let Some(v) = map.get_mut(name) {
                            child.apply(v)?;
                        }
                    }
                }
            }
            FieldPlan::List(item) => {
                if let Value::Array(items) = value {
                    for v in items {
                        item.apply(v)?;
                    }
                }
            }
            FieldPlan::Map(value_plan) => {
                // Arrow JSON encodes a map as an object; rewrite each map value.
                if let Value::Object(map) = value {
                    for (_k, v) in map.iter_mut() {
                        value_plan.apply(v)?;
                    }
                }
            }
        }
        Ok(())
    }
}

/// Build the [`FieldPlan`] for one field, or `None` if it carries no variant at
/// any depth. Recurses through struct/list/map; the exhaustive `DataType` match
/// makes a future Arrow container type a compile error here rather than a silent
/// skip.
fn field_plan(field: &Field) -> Option<FieldPlan> {
    if is_variant_field(field) {
        return Some(FieldPlan::Encode);
    }
    match field.data_type() {
        DataType::Struct(fields) => {
            let children: Vec<(String, FieldPlan)> = fields
                .iter()
                .filter_map(|f| field_plan(f).map(|p| (f.name().clone(), p)))
                .collect();
            (!children.is_empty()).then_some(FieldPlan::Struct(children))
        }
        DataType::List(item) | DataType::LargeList(item) | DataType::FixedSizeList(item, _) => {
            field_plan(item).map(|p| FieldPlan::List(Box::new(p)))
        }
        DataType::Map(entry, _) => match entry.data_type() {
            // A Map entry is always Struct(key, value) by Arrow's contract; only
            // the value field (index 1) can carry a variant (keys never do). Any
            // other shape is malformed — no variant.
            DataType::Struct(fields) => fields
                .get(1)
                .and_then(|f| field_plan(f))
                .map(|p| FieldPlan::Map(Box::new(p))),
            _ => None,
        },
        // Leaf/scalar types hold no nested field. The remaining container kinds
        // (ListView/LargeListView/Union/Dictionary/RunEndEncoded) are not
        // traversed for variant markers. Enumerated with no `_` arm so a future
        // Arrow DataType fails to compile here and forces a decision.
        DataType::Null
        | DataType::Boolean
        | DataType::Int8
        | DataType::Int16
        | DataType::Int32
        | DataType::Int64
        | DataType::UInt8
        | DataType::UInt16
        | DataType::UInt32
        | DataType::UInt64
        | DataType::Float16
        | DataType::Float32
        | DataType::Float64
        | DataType::Timestamp(_, _)
        | DataType::Date32
        | DataType::Date64
        | DataType::Time32(_)
        | DataType::Time64(_)
        | DataType::Duration(_)
        | DataType::Interval(_)
        | DataType::Binary
        | DataType::FixedSizeBinary(_)
        | DataType::LargeBinary
        | DataType::BinaryView
        | DataType::Utf8
        | DataType::LargeUtf8
        | DataType::Utf8View
        | DataType::ListView(_)
        | DataType::LargeListView(_)
        | DataType::Union(_, _)
        | DataType::Dictionary(_, _)
        | DataType::Decimal32(_, _)
        | DataType::Decimal64(_, _)
        | DataType::Decimal128(_, _)
        | DataType::Decimal256(_, _)
        | DataType::RunEndEncoded(_, _) => None,
    }
}
