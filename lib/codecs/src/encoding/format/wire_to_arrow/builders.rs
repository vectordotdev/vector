//! Column builders used by the wire-to-Arrow encoder.
//!
//! Leaves (`TypedBuilder`) wrap `arrow::array::*Builder` without trait-object
//! indirection. Branch nodes (`BuilderNode::Struct`, `BuilderNode::RepeatedMessage`)
//! own their children + per-row bookkeeping (validity, list offsets).

use std::sync::Arc;

use arrow::array::{
    ArrayRef, BooleanBuilder, Float32Builder, Float64Builder, Int32Builder, Int64Builder,
    LargeBinaryBuilder, LargeStringBuilder, ListArray, MapArray, StructArray,
    TimestampMicrosecondBuilder, UInt32Builder, UInt64Builder,
};
use arrow::buffer::{NullBuffer, OffsetBuffer, ScalarBuffer};
use arrow::datatypes::{DataType, Field, TimeUnit};

use super::append::append_proto3_default;
use super::errors::{Result, WireToArrowError};
use super::plan::{MessagePlan, PlanSlot, ScalarKind};

/// Leaf builder: one Arrow primitive column. Type-specific, no dyn dispatch.
pub enum TypedBuilder {
    Int32(Int32Builder),
    Int64(Int64Builder),
    UInt32(UInt32Builder),
    UInt64(UInt64Builder),
    Float32(Float32Builder),
    Float64(Float64Builder),
    Boolean(BooleanBuilder),
    LargeUtf8(LargeStringBuilder),
    LargeBinary(LargeBinaryBuilder),
    /// `TimestampMicrosecondBuilder` for the `_event_time` coercion and any
    /// other proto int64 field whose Arrow column is declared as
    /// `Timestamp(Microsecond, ...)`. The underlying i64 is written as
    /// microseconds since Unix epoch — we don't transform values, only the
    /// Arrow column type.
    TimestampMicros(TimestampMicrosecondBuilder),
}

/// Rough per-value byte-length hint used to pre-size the data buffer for
/// `LargeStringBuilder` / `LargeBinaryBuilder`. The builder grows on overflow,
/// so this only avoids the first few reallocations — picked to be in the right
/// order of magnitude for typical log-field values (short ids, short strings)
/// without over-allocating for columns that turn out to be mostly empty.
const AVG_VARLEN_BYTES_PER_VALUE: usize = 16;

impl TypedBuilder {
    /// True iff `dt` is one of the Arrow leaf data types this encoder can
    /// build. Used at plan-build to reject unsupported types up front
    /// instead of panicking in [`TypedBuilder::new`] on the first batch.
    pub fn supports(dt: &DataType) -> bool {
        matches!(
            dt,
            DataType::Int32
                | DataType::Int64
                | DataType::UInt32
                | DataType::UInt64
                | DataType::Float32
                | DataType::Float64
                | DataType::Boolean
                | DataType::LargeUtf8
                | DataType::LargeBinary
                | DataType::Timestamp(TimeUnit::Microsecond, _)
        )
    }

    /// Construct a typed builder matching the given Arrow DataType.
    ///
    /// # Panics
    /// Panics on unsupported primitive types. Plan-build validates the
    /// Arrow leaf types via [`supports`](Self::supports) before any
    /// builder is constructed, so reaching the panic here indicates a
    /// plan/builder mismatch (build bug, not user-controllable input).
    pub fn new(dt: &DataType, capacity: usize) -> Self {
        match dt {
            DataType::Int32 => TypedBuilder::Int32(Int32Builder::with_capacity(capacity)),
            DataType::Int64 => TypedBuilder::Int64(Int64Builder::with_capacity(capacity)),
            DataType::UInt32 => TypedBuilder::UInt32(UInt32Builder::with_capacity(capacity)),
            DataType::UInt64 => TypedBuilder::UInt64(UInt64Builder::with_capacity(capacity)),
            DataType::Float32 => TypedBuilder::Float32(Float32Builder::with_capacity(capacity)),
            DataType::Float64 => TypedBuilder::Float64(Float64Builder::with_capacity(capacity)),
            DataType::Boolean => TypedBuilder::Boolean(BooleanBuilder::with_capacity(capacity)),
            DataType::LargeUtf8 => TypedBuilder::LargeUtf8(LargeStringBuilder::with_capacity(
                capacity,
                capacity * AVG_VARLEN_BYTES_PER_VALUE,
            )),
            DataType::LargeBinary => TypedBuilder::LargeBinary(LargeBinaryBuilder::with_capacity(
                capacity,
                capacity * AVG_VARLEN_BYTES_PER_VALUE,
            )),
            DataType::Timestamp(TimeUnit::Microsecond, tz) => {
                let mut builder = TimestampMicrosecondBuilder::with_capacity(capacity);
                if let Some(tz) = tz {
                    builder = builder.with_timezone(tz.clone());
                }
                TypedBuilder::TimestampMicros(builder)
            }
            other => panic!("unsupported leaf DataType {other:?}"),
        }
    }

    pub fn append_null(&mut self) {
        match self {
            TypedBuilder::Int32(b) => b.append_null(),
            TypedBuilder::Int64(b) => b.append_null(),
            TypedBuilder::UInt32(b) => b.append_null(),
            TypedBuilder::UInt64(b) => b.append_null(),
            TypedBuilder::Float32(b) => b.append_null(),
            TypedBuilder::Float64(b) => b.append_null(),
            TypedBuilder::Boolean(b) => b.append_null(),
            TypedBuilder::LargeUtf8(b) => b.append_null(),
            TypedBuilder::LargeBinary(b) => b.append_null(),
            TypedBuilder::TimestampMicros(b) => b.append_null(),
        }
    }

    pub fn finish(&mut self) -> ArrayRef {
        match self {
            TypedBuilder::Int32(b) => Arc::new(b.finish()),
            TypedBuilder::Int64(b) => Arc::new(b.finish()),
            TypedBuilder::UInt32(b) => Arc::new(b.finish()),
            TypedBuilder::UInt64(b) => Arc::new(b.finish()),
            TypedBuilder::Float32(b) => Arc::new(b.finish()),
            TypedBuilder::Float64(b) => Arc::new(b.finish()),
            TypedBuilder::Boolean(b) => Arc::new(b.finish()),
            TypedBuilder::LargeUtf8(b) => Arc::new(b.finish()),
            TypedBuilder::LargeBinary(b) => Arc::new(b.finish()),
            TypedBuilder::TimestampMicros(b) => Arc::new(b.finish()),
        }
    }
}

/// A tree of builders mirroring a `MessagePlan`.
pub struct BuilderNodeList {
    pub(crate) nodes: Vec<BuilderNode>,
    /// Per-row scratch: `present[i]` is `true` if slot `i` was touched while
    /// scanning the current message at this level. Owned alongside `nodes` so
    /// each sub-plan reuses its own buffer instead of allocating a fresh
    /// `Vec<bool>` per nested-struct / list / map occurrence on the hot path.
    /// Reset between rows / sub-rows via [`BuilderNodeList::reset_present`].
    pub(crate) present: Vec<bool>,
}

/// One Arrow column's builder plus the dispatch info `scan_message` needs:
/// scalar kind for primitive variants, sub-plan for nested ones. Carrying it
/// here lets the scan loop dispatch on the node alone without re-indexing
/// `plan.slots`.
pub enum BuilderNode {
    Scalar {
        kind: ScalarKind,
        builder: TypedBuilder,
    },
    /// Proto enum field rendered into a STRING column by name (parity with the
    /// arrow_stream path). `builder` is always a `TypedBuilder::LargeUtf8`;
    /// `desc` supplies the number->name lookup at scan time.
    EnumString {
        desc: prost_reflect::EnumDescriptor,
        builder: TypedBuilder,
    },
    /// Repeated proto enum rendered into an Arrow `List<LargeUtf8>` by name.
    /// Same offset+current_offset bookkeeping as `RepeatedScalar`; `values` is
    /// always a `TypedBuilder::LargeUtf8` and `desc` supplies the lookup.
    RepeatedEnumString {
        desc: prost_reflect::EnumDescriptor,
        values: TypedBuilder,
        offsets: Vec<i32>,
        current_offset: i32,
    },
    /// Singular nested message. `validity[i]` tells whether row `i` had this
    /// field present (true) or absent (false — child values are null-filled).
    Struct {
        sub_plan: Arc<MessagePlan>,
        children: BuilderNodeList,
        validity: Vec<bool>,
    },
    /// Repeated nested message -> Arrow `List<Struct>`.
    /// `offsets[i]` = total element count after row `i`. `offsets[0] = 0`.
    /// `current_offset` tracks the running count across scan.
    RepeatedMessage {
        sub_plan: Arc<MessagePlan>,
        children: BuilderNodeList,
        offsets: Vec<i32>,
        current_offset: i32,
    },
    /// Repeated scalar -> Arrow `List<primitive>`. Same offset+current_offset
    /// bookkeeping as `RepeatedMessage`, but the child is a single typed
    /// primitive builder rather than a tree.
    RepeatedScalar {
        kind: ScalarKind,
        values: TypedBuilder,
        offsets: Vec<i32>,
        current_offset: i32,
    },
    /// Proto map -> Arrow `Map<Struct(...)>`. Wire-level handling is identical
    /// to `RepeatedMessage` (proto maps are `repeated MapEntry`); the finish
    /// step assembles a `MapArray` reusing the user-supplied `entry_field`
    /// verbatim — its name, nullability, and metadata are all preserved.
    /// Arrow's Map type doesn't mandate a specific entry name (Spark favors
    /// "key_value", the Arrow spec uses "entries"); honoring the caller's
    /// choice is what lets `RecordBatch::try_new` accept the assembled batch.
    Map {
        sub_plan: Arc<MessagePlan>,
        children: BuilderNodeList,
        offsets: Vec<i32>,
        current_offset: i32,
        entry_field: Arc<Field>,
    },
}

impl BuilderNodeList {
    /// Allocate a builder tree matching `plan`, with capacity for `capacity` rows.
    ///
    /// Called **once per batch** by [`WireToArrowEncoder::encode_batch`], not
    /// once per sink — Arrow's `*Builder::finish()` consumes the internal
    /// buffers to produce the output `ArrayRef`, so the tree is single-use.
    /// The shared, immutable state (`Arc<MessagePlan>`, `Arc<Schema>`) lives
    /// on the encoder and is what costs once per sink.
    ///
    /// [`WireToArrowEncoder::encode_batch`]: super::WireToArrowEncoder::encode_batch
    pub fn with_capacity(plan: &MessagePlan, capacity: usize) -> Result<Self> {
        let mut nodes = Vec::with_capacity(plan.slots.len());
        for (slot, field) in plan.slots.iter().zip(plan.arrow_fields.iter()) {
            let node = match slot {
                PlanSlot::Scalar(kind) => BuilderNode::Scalar {
                    kind: *kind,
                    builder: TypedBuilder::new(field.data_type(), capacity),
                },
                PlanSlot::EnumString(desc) => BuilderNode::EnumString {
                    desc: desc.clone(),
                    builder: TypedBuilder::new(field.data_type(), capacity),
                },
                PlanSlot::Struct(sub_plan) => BuilderNode::Struct {
                    sub_plan: Arc::clone(sub_plan),
                    children: BuilderNodeList::with_capacity(sub_plan, capacity)?,
                    validity: Vec::with_capacity(capacity),
                },
                PlanSlot::RepeatedMessage(sub_plan) => {
                    let mut offsets = Vec::with_capacity(capacity + 1);
                    offsets.push(0);
                    BuilderNode::RepeatedMessage {
                        sub_plan: Arc::clone(sub_plan),
                        // List lengths tend to be small; 2x rows is a rough guess.
                        children: BuilderNodeList::with_capacity(sub_plan, capacity * 2)?,
                        offsets,
                        current_offset: 0,
                    }
                }
                PlanSlot::RepeatedScalar(kind) => {
                    let element_type = match field.data_type() {
                        DataType::List(element_field) => element_field.data_type(),
                        _ => {
                            return Err(WireToArrowError::PlanBuilderMismatch {
                                site: "with_capacity:repeated_scalar_non_list",
                            });
                        }
                    };
                    let mut offsets = Vec::with_capacity(capacity + 1);
                    offsets.push(0);
                    BuilderNode::RepeatedScalar {
                        kind: *kind,
                        values: TypedBuilder::new(element_type, capacity * 2),
                        offsets,
                        current_offset: 0,
                    }
                }
                PlanSlot::RepeatedEnumString(desc) => {
                    let element_type = match field.data_type() {
                        DataType::List(element_field) => element_field.data_type(),
                        _ => {
                            return Err(WireToArrowError::PlanBuilderMismatch {
                                site: "with_capacity:repeated_enum_string_non_list",
                            });
                        }
                    };
                    let mut offsets = Vec::with_capacity(capacity + 1);
                    offsets.push(0);
                    BuilderNode::RepeatedEnumString {
                        desc: desc.clone(),
                        values: TypedBuilder::new(element_type, capacity * 2),
                        offsets,
                        current_offset: 0,
                    }
                }
                PlanSlot::Map(sub_plan) => {
                    let entry_field = match field.data_type() {
                        DataType::Map(entry_field, _) => Arc::clone(entry_field),
                        _ => {
                            return Err(WireToArrowError::PlanBuilderMismatch {
                                site: "with_capacity:map_non_map_arrow_type",
                            });
                        }
                    };
                    let mut offsets = Vec::with_capacity(capacity + 1);
                    offsets.push(0);
                    BuilderNode::Map {
                        sub_plan: Arc::clone(sub_plan),
                        children: BuilderNodeList::with_capacity(sub_plan, capacity * 2)?,
                        offsets,
                        current_offset: 0,
                        entry_field,
                    }
                }
                // No proto tag points here, so the slot is null-padded each
                // row by `finalize_row`'s "tag wasn't seen" branch.
                PlanSlot::Absent => build_absent_node(field, capacity)?,
            };
            nodes.push(node);
        }
        let present = vec![false; plan.slots.len()];
        Ok(Self { nodes, present })
    }

    /// Zero `present` ahead of scanning a row / sub-row. `slice::fill(false)`
    /// lowers to memset.
    #[inline]
    pub fn reset_present(&mut self) {
        self.present.fill(false);
    }

    /// After scanning one message, push per-row bookkeeping (struct validity,
    /// list offsets) and pad scalars whose tag wasn't seen.
    ///
    /// Padding rule for absent singular Scalars:
    /// - **Inside a Map entry sub-plan** (`plan.inside_map_entry == true`):
    ///   write the proto3 scalar default (`""`, `0`, `false`, `b""`). Arrow's
    ///   Map type declares the key non-nullable; proto3 wire format elides
    ///   default-valued singular fields *inside* MapEntry messages too, so a
    ///   null would fail `StructArray::try_new` at finish.
    /// - **Elsewhere**: write null. The plan-build non-nullability check
    ///   rejects schemas that can't tolerate the null up front.
    ///
    /// Infallible: the (`ScalarKind`, `TypedBuilder`) pairing the proto3-default
    /// helper relies on is enforced at plan-build time
    /// (`ScalarKind::matches_arrow_type` inside `MessagePlan::build_at_depth`),
    /// so there's no per-row failure mode here that would otherwise force the
    /// caller to tear down the whole batch.
    #[inline]
    pub fn finalize_row(&mut self, plan: &MessagePlan) {
        let Self { nodes, present } = self;
        debug_assert_eq!(plan.slots.len(), nodes.len());
        debug_assert_eq!(plan.slots.len(), present.len());
        let inside_map_entry = plan.inside_map_entry;
        for (node, &was_present) in nodes.iter_mut().zip(present.iter()) {
            match node {
                BuilderNode::Scalar { kind, builder } => {
                    if !was_present {
                        if inside_map_entry {
                            append_proto3_default(*kind, builder);
                        } else {
                            builder.append_null();
                        }
                    }
                }
                // An absent enum field is elided by proto3 at its zero value;
                // `proto_to_value` only walks present fields, so it yields null
                // (not the name of value 0). Match that: always null on absent.
                // Enum fields never appear inside a map entry, so there is no
                // proto3-default branch here.
                BuilderNode::EnumString { builder, .. } => {
                    if !was_present {
                        builder.append_null();
                    }
                }
                BuilderNode::Struct {
                    children, validity, ..
                } => {
                    validity.push(was_present);
                    if !was_present {
                        children.fill_null_row();
                    }
                }
                // All list-flavored slots push an offsets marker per row.
                // For proto repeated fields (including maps), the outer list
                // itself is never null — absent just means empty list.
                BuilderNode::RepeatedMessage {
                    offsets,
                    current_offset,
                    ..
                }
                | BuilderNode::RepeatedScalar {
                    offsets,
                    current_offset,
                    ..
                }
                | BuilderNode::RepeatedEnumString {
                    offsets,
                    current_offset,
                    ..
                }
                | BuilderNode::Map {
                    offsets,
                    current_offset,
                    ..
                } => {
                    offsets.push(*current_offset);
                }
            }
        }
    }

    /// Recursively append nulls / empty lists to the entire subtree so row
    /// counts line up when a parent struct is null.
    pub fn fill_null_row(&mut self) {
        for node in self.nodes.iter_mut() {
            match node {
                BuilderNode::Scalar { builder, .. }
                | BuilderNode::EnumString { builder, .. } => builder.append_null(),
                BuilderNode::Struct {
                    children, validity, ..
                } => {
                    validity.push(false);
                    children.fill_null_row();
                }
                BuilderNode::RepeatedMessage {
                    offsets,
                    current_offset,
                    ..
                }
                | BuilderNode::RepeatedScalar {
                    offsets,
                    current_offset,
                    ..
                }
                | BuilderNode::RepeatedEnumString {
                    offsets,
                    current_offset,
                    ..
                }
                | BuilderNode::Map {
                    offsets,
                    current_offset,
                    ..
                } => {
                    offsets.push(*current_offset);
                }
            }
        }
    }

    /// Finalize this level and return the Arrow arrays in schema order.
    /// `Absent` slots fall through to the same per-variant branches: the
    /// builders are already null-filled by `finalize_row`.
    pub fn finish(&mut self, plan: &MessagePlan) -> Result<Vec<ArrayRef>> {
        let mut out: Vec<ArrayRef> = Vec::with_capacity(plan.slots.len());
        for (idx, node) in self.nodes.iter_mut().enumerate() {
            let arrow_field = &plan.arrow_fields[idx];
            let arr: ArrayRef = match node {
                BuilderNode::Scalar { builder, .. }
                | BuilderNode::EnumString { builder, .. } => builder.finish(),
                BuilderNode::Struct {
                    sub_plan,
                    children,
                    validity,
                } => {
                    let child_arrays = children.finish(sub_plan)?;
                    let null_buf = NullBuffer::from(std::mem::take(validity));
                    Arc::new(
                        StructArray::try_new(
                            sub_plan.arrow_fields.clone(),
                            child_arrays,
                            Some(null_buf),
                        )
                        .map_err(|e| WireToArrowError::ArrayAssembly {
                            kind: "struct",
                            source: e,
                        })?,
                    )
                }
                BuilderNode::RepeatedMessage {
                    sub_plan,
                    children,
                    offsets,
                    ..
                } => {
                    let child_arrays = children.finish(sub_plan)?;
                    let struct_arr =
                        StructArray::try_new(sub_plan.arrow_fields.clone(), child_arrays, None)
                            .map_err(|e| WireToArrowError::ArrayAssembly {
                                kind: "list element struct",
                                source: e,
                            })?;
                    let offset_buffer =
                        OffsetBuffer::new(ScalarBuffer::from(std::mem::take(offsets)));
                    let element_field = Arc::new(Field::new(
                        "item",
                        DataType::Struct(sub_plan.arrow_fields.clone()),
                        true,
                    ));
                    Arc::new(
                        ListArray::try_new(
                            element_field,
                            offset_buffer,
                            Arc::new(struct_arr),
                            None,
                        )
                        .map_err(|e| WireToArrowError::ArrayAssembly {
                            kind: "list",
                            source: e,
                        })?,
                    )
                }
                BuilderNode::RepeatedScalar { values, offsets, .. } => {
                    let values_array = values.finish();
                    let offset_buffer =
                        OffsetBuffer::new(ScalarBuffer::from(std::mem::take(offsets)));
                    // Preserve the element field the schema declared (name
                    // typically "item", but follow the caller's choice).
                    let element_field = match arrow_field.data_type() {
                        DataType::List(f) => Arc::clone(f),
                        other => {
                            return Err(WireToArrowError::UnsupportedCombination {
                                name: arrow_field.name().to_string(),
                                kind: "RepeatedScalar".to_string(),
                                arrow_type: format!("{other:?}"),
                                repeated: true,
                            });
                        }
                    };
                    Arc::new(
                        ListArray::try_new(element_field, offset_buffer, values_array, None)
                            .map_err(|e| WireToArrowError::ArrayAssembly {
                                kind: "list (scalar)",
                                source: e,
                            })?,
                    )
                }
                BuilderNode::RepeatedEnumString { values, offsets, .. } => {
                    let values_array = values.finish();
                    let offset_buffer =
                        OffsetBuffer::new(ScalarBuffer::from(std::mem::take(offsets)));
                    let element_field = match arrow_field.data_type() {
                        DataType::List(f) => Arc::clone(f),
                        other => {
                            return Err(WireToArrowError::UnsupportedCombination {
                                name: arrow_field.name().to_string(),
                                kind: "RepeatedEnumString".to_string(),
                                arrow_type: format!("{other:?}"),
                                repeated: true,
                            });
                        }
                    };
                    Arc::new(
                        ListArray::try_new(element_field, offset_buffer, values_array, None)
                            .map_err(|e| WireToArrowError::ArrayAssembly {
                                kind: "list (enum string)",
                                source: e,
                            })?,
                    )
                }
                BuilderNode::Map {
                    sub_plan,
                    children,
                    offsets,
                    entry_field,
                    ..
                } => {
                    let child_arrays = children.finish(sub_plan)?;
                    let struct_arr =
                        StructArray::try_new(sub_plan.arrow_fields.clone(), child_arrays, None)
                            .map_err(|e| WireToArrowError::ArrayAssembly {
                                kind: "map entry struct",
                                source: e,
                            })?;
                    let offset_buffer =
                        OffsetBuffer::new(ScalarBuffer::from(std::mem::take(offsets)));
                    // Reuse the user-supplied entry Field unchanged: Arrow's
                    // Map spec doesn't pin the entry name ("entries" is
                    // canonical; Spark/Delta use "key_value"), and
                    // a name mismatch makes `RecordBatch::try_new` reject the
                    // whole batch at finish. Honoring the caller's name +
                    // nullability + metadata avoids that footgun. The encoder
                    // never emits null entries (only empty maps), so a
                    // declared non-nullable entry is also safe.
                    Arc::new(
                        MapArray::try_new(
                            Arc::clone(entry_field),
                            offset_buffer,
                            struct_arr,
                            None,
                            false,
                        )
                        .map_err(|e| WireToArrowError::ArrayAssembly {
                            kind: "map",
                            source: e,
                        })?,
                    )
                }
            };
            out.push(arr);
        }
        Ok(out)
    }
}

/// Build a null-filled builder tree matching the Arrow `field`'s shape, used
/// for [`PlanSlot::Absent`] columns. The builder is the same shape as a normal
/// column of that Arrow type, but no wire tags ever dispatch to it so it stays
/// fully null-padded by `finalize_row` / `fill_null_row`.
fn build_absent_node(field: &Field, capacity: usize) -> Result<BuilderNode> {
    // Inert `kind` for variants that carry one — the slot is never
    // dispatched, so the value never matters.
    const ABSENT_KIND: ScalarKind = ScalarKind::Int32;
    Ok(match field.data_type() {
        DataType::Struct(inner_fields) => {
            let sub_plan = Arc::new(MessagePlan::all_absent(inner_fields));
            BuilderNode::Struct {
                children: BuilderNodeList::with_capacity(&sub_plan, capacity)?,
                sub_plan,
                validity: Vec::with_capacity(capacity),
            }
        }
        DataType::List(element_field) => {
            let mut offsets = Vec::with_capacity(capacity + 1);
            offsets.push(0);
            match element_field.data_type() {
                DataType::Struct(inner_fields) => {
                    let sub_plan = Arc::new(MessagePlan::all_absent(inner_fields));
                    BuilderNode::RepeatedMessage {
                        children: BuilderNodeList::with_capacity(&sub_plan, capacity * 2)?,
                        sub_plan,
                        offsets,
                        current_offset: 0,
                    }
                }
                _ => BuilderNode::RepeatedScalar {
                    kind: ABSENT_KIND,
                    values: TypedBuilder::new(element_field.data_type(), capacity * 2),
                    offsets,
                    current_offset: 0,
                },
            }
        }
        DataType::Map(entry_field, _) => {
            let inner_fields = match entry_field.data_type() {
                DataType::Struct(fs) => fs,
                _ => {
                    return Err(WireToArrowError::PlanBuilderMismatch {
                        site: "build_absent_node:map_entry_non_struct",
                    });
                }
            };
            let sub_plan = Arc::new(MessagePlan::all_absent(inner_fields));
            let mut offsets = Vec::with_capacity(capacity + 1);
            offsets.push(0);
            BuilderNode::Map {
                children: BuilderNodeList::with_capacity(&sub_plan, capacity * 2)?,
                sub_plan,
                offsets,
                current_offset: 0,
                entry_field: Arc::clone(entry_field),
            }
        }
        // Scalar Arrow types — build a primitive builder. `TypedBuilder::new`
        // still has an internal `unsupported leaf DataType` panic, but it's
        // a build-bug-only path: `validate_arrow_leaf_types` rejects
        // unsupported leaves at plan-build, so this call can't see one.
        _ => BuilderNode::Scalar {
            kind: ABSENT_KIND,
            builder: TypedBuilder::new(field.data_type(), capacity),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{Array, AsArray};

    #[test]
    fn int32_builder_roundtrip() {
        let mut tb = TypedBuilder::new(&DataType::Int32, 4);
        if let TypedBuilder::Int32(b) = &mut tb {
            b.append_value(1);
            b.append_value(2);
        }
        tb.append_null();
        let arr = tb.finish();
        let i32arr = arr.as_primitive::<arrow::datatypes::Int32Type>();
        assert_eq!(i32arr.len(), 3);
        assert_eq!(i32arr.value(0), 1);
        assert_eq!(i32arr.value(1), 2);
        assert!(i32arr.is_null(2));
    }

    #[test]
    fn string_builder_roundtrip() {
        let mut tb = TypedBuilder::new(&DataType::LargeUtf8, 4);
        if let TypedBuilder::LargeUtf8(b) = &mut tb {
            b.append_value("hello");
            b.append_value("world");
        }
        tb.append_null();
        let arr = tb.finish();
        assert_eq!(arr.len(), 3);
    }

    #[test]
    #[should_panic(expected = "unsupported leaf DataType")]
    fn unsupported_type_panics() {
        // Date32 is not in our scope.
        let _ = TypedBuilder::new(&DataType::Date32, 1);
    }
}
