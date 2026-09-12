//! Encoding plan: a tree describing how to decode a given proto message into
//! a set of Arrow column builders.
//!
//! Built once per (proto descriptor, Arrow schema) pair. Immutable after build.

use std::sync::Arc;

use arrow::datatypes::{DataType, Fields, TimeUnit};
use prost_reflect::{Cardinality, EnumDescriptor, Kind, MessageDescriptor};

use super::builders::TypedBuilder;
use super::errors::{Result, WireToArrowError};

/// Maximum nesting depth permitted in a plan. Arrow schemas can in principle
/// nest arbitrarily deep, but the build walk recurses 1:1 with structural
/// depth and would blow the Rust call stack on pathological input. Real-world
/// schemas are well under this; the cap exists to keep DoS-shaped input
/// (deep schema at plan-build time, or deep wire-bytes nesting at scan time)
/// from running to stack overflow.
///
/// `scan_message` recursion is bounded by the plan, so capping the plan caps
/// both paths.
pub const MAX_NESTING_DEPTH: usize = 64;

/// Proto wire-type codes (the low 3 bits of a tag).
///
/// The [`super::wire::WireType`] enum is private to that module, so we
/// redeclare the codes here for use across this module's public-API
/// surface (`ScalarKind::wire_type`, error fields, packed-scalar dispatch).
/// Keep these in sync with the proto spec: <https://protobuf.dev/programming-guides/encoding/#structure>
pub(super) const WT_VARINT: u8 = 0;
pub(super) const WT_I64: u8 = 1;
pub(super) const WT_LEN: u8 = 2;
pub(super) const WT_I32: u8 = 5;

/// "No slot maps to this proto field number" in [`MessagePlan::slot_by_proto_field`].
pub(crate) const SLOT_UNKNOWN: u32 = u32::MAX;

/// Proto scalar kinds that this encoder can read off the wire and append to
/// Arrow primitive builders. Proto enums map to `Int32` by default (or to a
/// `LargeUtf8` value name when the target column is a string).
#[derive(Clone, Copy, Debug)]
pub enum ScalarKind {
    Int32,
    Int64,
    UInt32,
    UInt64,
    SInt32,
    SInt64,
    Bool,
    Fixed32,
    SFixed32,
    Float,
    Fixed64,
    SFixed64,
    Double,
    String,
    Bytes,
}

impl ScalarKind {
    /// Proto wire type expected for values of this kind.
    pub fn wire_type(self) -> u8 {
        match self {
            ScalarKind::Int32
            | ScalarKind::Int64
            | ScalarKind::UInt32
            | ScalarKind::UInt64
            | ScalarKind::SInt32
            | ScalarKind::SInt64
            | ScalarKind::Bool => WT_VARINT,
            ScalarKind::Fixed32 | ScalarKind::SFixed32 | ScalarKind::Float => WT_I32,
            ScalarKind::Fixed64 | ScalarKind::SFixed64 | ScalarKind::Double => WT_I64,
            ScalarKind::String | ScalarKind::Bytes => WT_LEN,
        }
    }

    /// True iff `dt` is the Arrow leaf type that pairs with this proto
    /// scalar kind. Enforced at plan-build by
    /// [`MessagePlan::build_at_depth`] so the runtime appenders
    /// ([`append_scalar_from_wire`] and [`append_proto3_default`]) can
    /// assume the pairing is well-formed and don't need a runtime fallthrough
    /// for kind/builder mismatches.
    ///
    /// [`append_scalar_from_wire`]: super::append::append_scalar_from_wire
    /// [`append_proto3_default`]: super::append::append_proto3_default
    pub(super) fn matches_arrow_type(self, dt: &DataType) -> bool {
        match (self, dt) {
            (ScalarKind::Int32 | ScalarKind::SInt32 | ScalarKind::SFixed32, DataType::Int32) => {
                true
            }
            (
                ScalarKind::Int64 | ScalarKind::SInt64 | ScalarKind::SFixed64,
                DataType::Int64 | DataType::Timestamp(TimeUnit::Microsecond, _),
            ) => true,
            (ScalarKind::UInt32 | ScalarKind::Fixed32, DataType::UInt32) => true,
            (ScalarKind::UInt64 | ScalarKind::Fixed64, DataType::UInt64) => true,
            (ScalarKind::Float, DataType::Float32) => true,
            (ScalarKind::Double, DataType::Float64) => true,
            (ScalarKind::Bool, DataType::Boolean) => true,
            (ScalarKind::String, DataType::LargeUtf8) => true,
            (ScalarKind::Bytes, DataType::LargeBinary) => true,
            _ => false,
        }
    }

    /// Map a `prost_reflect::Kind` to a `ScalarKind`. Returns `None` for Kinds
    /// that aren't scalars (Message types are handled at the plan level).
    pub fn from_proto_kind(kind: &Kind) -> Option<Self> {
        Some(match kind {
            Kind::Int32 => ScalarKind::Int32,
            Kind::Int64 => ScalarKind::Int64,
            Kind::Uint32 => ScalarKind::UInt32,
            Kind::Uint64 => ScalarKind::UInt64,
            Kind::Sint32 => ScalarKind::SInt32,
            Kind::Sint64 => ScalarKind::SInt64,
            Kind::Fixed32 => ScalarKind::Fixed32,
            Kind::Fixed64 => ScalarKind::Fixed64,
            Kind::Sfixed32 => ScalarKind::SFixed32,
            Kind::Sfixed64 => ScalarKind::SFixed64,
            Kind::Float => ScalarKind::Float,
            Kind::Double => ScalarKind::Double,
            Kind::Bool => ScalarKind::Bool,
            Kind::String => ScalarKind::String,
            Kind::Bytes => ScalarKind::Bytes,
            // Proto enums carry over as int32 on the Arrow side.
            Kind::Enum(_) => ScalarKind::Int32,
            Kind::Message(_) => return None,
        })
    }
}

/// One entry per Arrow field at this message level: describes how to route
/// wire-bytes values into the corresponding Arrow column builder.
#[derive(Debug)]
pub enum PlanSlot {
    Scalar(ScalarKind),
    Struct(Arc<MessagePlan>),
    RepeatedMessage(Arc<MessagePlan>),
    /// Repeated scalar field (e.g. `repeated int32`) -> Arrow `List<primitive>`.
    /// Handles both packed and unpacked wire encodings at scan time.
    RepeatedScalar(ScalarKind),
    /// Proto enum field paired to a STRING Arrow column: render the varint as
    /// its enum-value *name* (e.g. `1` -> `"SUCCESS"`), matching the
    /// arrow_stream / `proto_to_value` path. The default enum mapping is
    /// `ScalarKind::Int32` (see [`ScalarKind::from_proto_kind`]); this variant
    /// is only chosen when the target Arrow leaf is `LargeUtf8`. Carries the
    /// [`EnumDescriptor`] for the number->name lookup at scan time. Kept out of
    /// [`ScalarKind`] so that enum stays `Copy` and the primitive hot path is
    /// untouched.
    EnumString(EnumDescriptor),
    /// Repeated proto enum field paired to an Arrow `List<LargeUtf8>` column:
    /// render each enum varint as its value name (the repeated analogue of
    /// [`PlanSlot::EnumString`]). Handles both packed and unpacked wire
    /// encodings at scan time, like [`PlanSlot::RepeatedScalar`]. A repeated
    /// enum paired with `List<Int32>` still falls through to
    /// [`PlanSlot::RepeatedScalar`] and stays numeric.
    RepeatedEnumString(EnumDescriptor),
    /// Proto `map<K, V>` -> Arrow `Map<Struct(key, value)>`. On the wire, maps
    /// are encoded as `repeated MapEntry` where `MapEntry` is a generated
    /// message with field 1 = key and field 2 = value; we scan them the same
    /// way as `RepeatedMessage` and assemble a `MapArray` at finish time.
    Map(Arc<MessagePlan>),
    /// Arrow column has no matching proto field — always emits null (or empty
    /// list / all-null struct). Happens when the Arrow schema has more columns
    /// than the producer's proto — typically because a field was deleted from
    /// the proto schema but the Arrow schema hasn't been updated yet, or the
    /// producer is running an older version. The scanner never dispatches to
    /// these slots; `finalize_row` null-pads them for every row.
    Absent,
}

/// Plan for encoding one proto message type into a set of Arrow column builders.
#[derive(Debug)]
pub struct MessagePlan {
    /// One entry per Arrow field at this level, in schema order.
    pub(crate) slots: Vec<PlanSlot>,
    /// Reverse index from proto field number to slot index, with
    /// [`SLOT_UNKNOWN`] marking unknown / out-of-range fields. Dense vector,
    /// no hashing on the hot path.
    pub(crate) slot_by_proto_field: Vec<u32>,
    /// Arrow `Fields` at this level, kept for assembly of `StructArray` / `ListArray`.
    pub(crate) arrow_fields: Fields,
    /// True if this plan describes the entry sub-message of a `map<K, V>`
    /// slot. Proto3 elides singular fields at their default value on the
    /// wire — including *inside* `MapEntry` messages — but Arrow's Map type
    /// declares the key field non-nullable. `finalize_row` consults this
    /// flag and materializes the proto3 scalar default (e.g. `""` for
    /// String, `0` for Int32) for absent scalar slots instead of writing a
    /// null, which would fail `StructArray::try_new` at batch finish.
    pub(crate) inside_map_entry: bool,
}

impl MessagePlan {
    /// Build a plan from a proto message descriptor and a matching Arrow `Fields`.
    ///
    /// Fields in the Arrow schema must exist (by name) in the proto descriptor.
    /// Proto fields absent from the Arrow schema are treated as unknown and will
    /// be skipped at scan time.
    ///
    /// # Self-referential proto types
    ///
    /// Proto schemas can reference themselves (e.g. `message Tree { Tree left
    /// = 1; }`), but Arrow schemas cannot carry a recursive type. Recursion
    /// in this builder terminates because we only descend into
    /// `Kind::Message(_)` fields when the Arrow target at that path is also
    /// a nested type (`Struct` / `List<Struct>` / `Map`). Arrow schemas are
    /// finite by construction (Arrow schemas don't produce cyclic types), so
    /// each recursion step strictly reduces the
    /// remaining Arrow depth. Proto self-reference past the depth declared
    /// in the Arrow schema is treated as an unknown field and skipped at
    /// scan time.
    ///
    /// A hard depth cap of [`MAX_NESTING_DEPTH`] guards against pathological
    /// schemas that would otherwise overflow the Rust call stack at build
    /// time. `scan_message`'s recursion is bounded by the plan, so this cap
    /// also bounds the scan-time recursion driven by attacker-controlled
    /// wire bytes.
    ///
    /// # Oneof
    ///
    /// Proto `oneof` is purely an annotation; on the wire each variant is a
    /// normal singular field with its own tag, and the receiver takes
    /// whichever variant appeared last in the bytes. No special handling is
    /// needed at the plan level — each variant becomes its own `PlanSlot`
    /// (Scalar / Struct / etc.) and the normal "absent slot => null"
    /// machinery produces the correct Arrow output.
    pub fn build(descriptor: &MessageDescriptor, fields: &Fields) -> Result<Self> {
        Self::build_at_depth(descriptor, fields, 0, /* inside_map_entry */ false)
    }

    /// Recursive helper for [`build`]; `depth` is the current nesting level
    /// (0 at the top), `inside_map_entry` is true when called for a Map's
    /// entry sub-plan. Map entries have Arrow-spec-mandated nullability
    /// (key non-nullable, value typically nullable), so the singular-field
    /// nullability check is suppressed inside that recursion to avoid
    /// false positives. Returns [`WireToArrowError::SchemaTooDeep`] once
    /// the level being built would exceed [`MAX_NESTING_DEPTH`].
    fn build_at_depth(
        descriptor: &MessageDescriptor,
        fields: &Fields,
        depth: usize,
        inside_map_entry: bool,
    ) -> Result<Self> {
        if depth >= MAX_NESTING_DEPTH {
            return Err(WireToArrowError::SchemaTooDeep {
                limit: MAX_NESTING_DEPTH,
            });
        }
        let mut slots = Vec::with_capacity(fields.len());
        let mut max_field_num = 0u32;
        // `slot_proto_numbers[i] = Some(n)` means slot i maps to proto field n;
        // `None` means slot i is `Absent` (no proto tag maps here) and is skipped
        // by the reverse-index build below.
        let mut slot_proto_numbers: Vec<Option<u32>> = Vec::with_capacity(fields.len());

        for arrow_field in fields.iter() {
            let Some(proto_field) = descriptor.get_field_by_name(arrow_field.name()) else {
                // Inside a Map entry sub-plan, a name that doesn't match the
                // proto MapEntry's `key`/`value` is structurally broken: there's
                // no proto field to read from, the Map type's non-null key
                // contract still applies, and the absent-padding path would
                // hand a kind/builder pair to `append_proto3_default` that it
                // can't satisfy — which is now a panic (`unreachable!`) rather
                // than a Result. Reject up front so the failure shows up at
                // sink init with a clear message.
                if inside_map_entry {
                    return Err(WireToArrowError::MapEntryFieldNotInProto {
                        name: arrow_field.name().to_string(),
                    });
                }
                // Schema drift: the Arrow column exists but the proto doesn't
                // carry it. We can only emit all-null for such a column, so
                // a non-nullable declaration is a hard mismatch — error
                // early before any data flows.
                if !arrow_field.is_nullable() {
                    return Err(WireToArrowError::NonNullableNotGuaranteed {
                        name: arrow_field.name().to_string(),
                        reason: "the proto descriptor does not carry this field, \
                                 so the column would be all-null",
                    });
                }
                // Absent slots get their builders constructed via
                // `build_absent_node` -> `TypedBuilder::new` for every
                // primitive leaf in the Arrow type. Validate them at
                // plan-build so an unsupported leaf type surfaces here
                // instead of panicking on the first batch.
                validate_arrow_leaf_types(arrow_field.name(), arrow_field.data_type())?;
                // Log + metric + keep going — the column becomes
                // always-null. Typical cause: a field was removed from the
                // proto before the target schema was updated.
                tracing::warn!(
                    message = "proto descriptor is missing a field declared in the Arrow schema; \
                               the column will be emitted as all-null",
                    field = %arrow_field.name(),
                    descriptor = %descriptor.full_name(),
                );
                metrics::counter!(
                    "wire_to_arrow_missing_proto_field",
                    "field" => arrow_field.name().to_string(),
                    "descriptor" => descriptor.full_name().to_string(),
                )
                .increment(1);
                slots.push(PlanSlot::Absent);
                slot_proto_numbers.push(None);
                continue;
            };
            max_field_num = max_field_num.max(proto_field.number());
            slot_proto_numbers.push(Some(proto_field.number()));

            let is_repeated = proto_field.cardinality() == Cardinality::Repeated;
            let kind = proto_field.kind();

            // Maps take precedence: proto map fields have `is_map() == true` and
            // cardinality Repeated, but we dispatch differently from a bare
            // repeated-message field.
            let slot = if proto_field.is_map() {
                let entry_desc = match &kind {
                    Kind::Message(m) => m,
                    _ => {
                        return Err(WireToArrowError::UnsupportedCombination {
                            name: arrow_field.name().to_string(),
                            kind: format!("{kind:?}"),
                            arrow_type: format!("{:?}", arrow_field.data_type()),
                            repeated: is_repeated,
                        });
                    }
                };
                let entry_fields = match arrow_field.data_type() {
                    DataType::Map(entry_field, _keys_sorted) => match entry_field.data_type() {
                        DataType::Struct(fs) => fs,
                        other => {
                            return Err(WireToArrowError::UnsupportedCombination {
                                name: arrow_field.name().to_string(),
                                kind: format!("{kind:?}"),
                                arrow_type: format!("Map(entry_type = {other:?})"),
                                repeated: is_repeated,
                            });
                        }
                    },
                    other => {
                        return Err(WireToArrowError::UnsupportedCombination {
                            name: arrow_field.name().to_string(),
                            kind: format!("{kind:?}"),
                            arrow_type: format!("{other:?}"),
                            repeated: is_repeated,
                        });
                    }
                };
                let sub = MessagePlan::build_at_depth(
                    entry_desc,
                    entry_fields,
                    depth + 1,
                    /* inside_map_entry */ true,
                )?;
                PlanSlot::Map(Arc::new(sub))
            } else {
                match (&kind, arrow_field.data_type(), is_repeated) {
                    // Singular proto enum -> STRING column: render the enum
                    // value *name* rather than its number. Enum + an integer
                    // column falls through to the generic scalar arm below,
                    // where `from_proto_kind` maps it to `Int32` as before, so
                    // existing enum->int tables are unaffected.
                    (Kind::Enum(enum_desc), DataType::LargeUtf8, false) => {
                        PlanSlot::EnumString(enum_desc.clone())
                    }
                    // Singular scalar.
                    (_, dt, false) if !matches!(dt, DataType::Struct(_) | DataType::List(_)) => {
                        validate_arrow_leaf_types(arrow_field.name(), dt)?;
                        let sk = ScalarKind::from_proto_kind(&kind).ok_or_else(|| {
                            WireToArrowError::UnsupportedKind {
                                name: arrow_field.name().to_string(),
                                kind: format!("{kind:?}"),
                            }
                        })?;
                        // Reject mismatched (proto scalar, Arrow leaf) pairings
                        // up front. The runtime appenders rely on this invariant
                        // to avoid a per-row fallthrough that would otherwise
                        // fail the whole batch rather than the offending row.
                        if !sk.matches_arrow_type(dt) {
                            return Err(WireToArrowError::UnsupportedCombination {
                                name: arrow_field.name().to_string(),
                                kind: format!("{kind:?}"),
                                arrow_type: format!("{dt:?}"),
                                repeated: false,
                            });
                        }
                        PlanSlot::Scalar(sk)
                    }
                    // Singular nested message.
                    (Kind::Message(inner_desc), DataType::Struct(inner_fields), false) => {
                        let sub = MessagePlan::build_at_depth(
                            inner_desc,
                            inner_fields,
                            depth + 1,
                            /* inside_map_entry */ false,
                        )?;
                        PlanSlot::Struct(Arc::new(sub))
                    }
                    // Repeated nested message -> Arrow List<Struct>.
                    (Kind::Message(inner_desc), DataType::List(element_field), true) => {
                        let inner_fields = match element_field.data_type() {
                            DataType::Struct(fs) => fs,
                            other => {
                                return Err(WireToArrowError::RepeatedNonStructList {
                                    name: arrow_field.name().to_string(),
                                    element: format!("{other:?}"),
                                });
                            }
                        };
                        let sub = MessagePlan::build_at_depth(
                            inner_desc,
                            inner_fields,
                            depth + 1,
                            /* inside_map_entry */ false,
                        )?;
                        PlanSlot::RepeatedMessage(Arc::new(sub))
                    }
                    // Repeated proto enum -> Arrow List<LargeUtf8>: render each
                    // element's value *name*. A repeated enum paired with an
                    // integer element type falls through to the RepeatedScalar
                    // arm below and stays numeric (Int32), as before.
                    (Kind::Enum(enum_desc), DataType::List(item_field), true)
                        if matches!(item_field.data_type(), DataType::LargeUtf8) =>
                    {
                        PlanSlot::RepeatedEnumString(enum_desc.clone())
                    }
                    // Repeated scalar -> Arrow List<primitive>.
                    (_, DataType::List(item_field), true) => {
                        validate_arrow_leaf_types(item_field.name(), item_field.data_type())?;
                        let sk = ScalarKind::from_proto_kind(&kind).ok_or_else(|| {
                            WireToArrowError::UnsupportedKind {
                                name: arrow_field.name().to_string(),
                                kind: format!("{kind:?}"),
                            }
                        })?;
                        if !sk.matches_arrow_type(item_field.data_type()) {
                            return Err(WireToArrowError::UnsupportedCombination {
                                name: arrow_field.name().to_string(),
                                kind: format!("{kind:?}"),
                                arrow_type: format!("List<{:?}>", item_field.data_type()),
                                repeated: true,
                            });
                        }
                        PlanSlot::RepeatedScalar(sk)
                    }
                    (k, dt, r) => {
                        return Err(WireToArrowError::UnsupportedCombination {
                            name: arrow_field.name().to_string(),
                            kind: format!("{k:?}"),
                            arrow_type: format!("{dt:?}"),
                            repeated: r,
                        });
                    }
                }
            };
            // Singular slots (Scalar, Struct) emit a null whenever the
            // tag is absent from the wire — which proto3 does by default
            // for default-valued fields. A non-nullable Arrow declaration
            // would trip a generic `RecordBatch::try_new` failure deep in
            // encode_batch and drop the whole batch with no row context.
            // Reject the mismatch up front. List<…>/Map<…> outer columns
            // are exempt: the encoder always emits at least an empty
            // list / empty map per row, so the outer column never holds
            // a null. Map entry sub-plans are also exempt: Arrow's Map
            // type itself dictates the non-null key contract, so the
            // check would be a false positive there.
            if !arrow_field.is_nullable() && !inside_map_entry {
                match slot {
                    // EnumString is a singular field too: absent -> null (parity
                    // with `proto_to_value`), so a non-nullable column can't be
                    // guaranteed and is rejected alongside Scalar/Struct.
                    PlanSlot::Scalar(_) | PlanSlot::Struct(_) | PlanSlot::EnumString(_) => {
                        return Err(WireToArrowError::NonNullableNotGuaranteed {
                            name: arrow_field.name().to_string(),
                            reason: "proto3 singular fields are omitted at default value, \
                                     so the column may contain nulls",
                        });
                    }
                    PlanSlot::RepeatedMessage(_)
                    | PlanSlot::RepeatedScalar(_)
                    | PlanSlot::RepeatedEnumString(_)
                    | PlanSlot::Map(_)
                    | PlanSlot::Absent => {}
                }
            }
            slots.push(slot);
        }

        let mut slot_by_proto_field = vec![SLOT_UNKNOWN; (max_field_num as usize) + 1];
        for (slot_idx, pn) in slot_proto_numbers.iter().enumerate() {
            if let Some(pn) = pn {
                slot_by_proto_field[*pn as usize] = slot_idx as u32;
            }
        }

        // Opposite-direction drift: proto fields the Arrow schema doesn't
        // carry. These would be silently skipped at scan time (matching
        // proto's standard "ignore unknown fields" behavior), but if the
        // descriptor reflects the current producer schema, it signals
        // "producer emits this field but the target schema hasn't caught up."
        // Log + count once at plan build so operators notice.
        let arrow_field_names: std::collections::HashSet<&str> =
            fields.iter().map(|f| f.name().as_str()).collect();
        for proto_field in descriptor.fields() {
            if !arrow_field_names.contains(proto_field.name()) {
                tracing::warn!(
                    message = "proto descriptor has a field not declared in the Arrow schema; \
                               occurrences on the wire will be silently skipped",
                    field = %proto_field.name(),
                    descriptor = %descriptor.full_name(),
                );
                metrics::counter!(
                    "wire_to_arrow_extra_proto_field",
                    "field" => proto_field.name().to_string(),
                    "descriptor" => descriptor.full_name().to_string(),
                )
                .increment(1);
            }
        }

        Ok(MessagePlan {
            slots,
            slot_by_proto_field,
            arrow_fields: fields.clone(),
            inside_map_entry,
        })
    }

}

/// Recursively walk an Arrow `DataType` tree and verify every primitive
/// leaf is supported by [`TypedBuilder`]. Plan-build calls this so an
/// unsupported leaf (e.g. `Date32`) surfaces as a clean
/// [`WireToArrowError::UnsupportedArrowLeafType`] at serializer init,
/// instead of panicking in `TypedBuilder::new` on the first batch.
///
/// `Struct` / `List` / `Map` are structural; recurse through them. The
/// terminal case is a primitive leaf that either passes
/// [`TypedBuilder::supports`] or fails the check.
pub(super) fn validate_arrow_leaf_types(field_name: &str, dt: &DataType) -> Result<()> {
    match dt {
        DataType::Struct(inner_fields) => {
            for f in inner_fields.iter() {
                validate_arrow_leaf_types(f.name(), f.data_type())?;
            }
            Ok(())
        }
        DataType::List(item_field) => {
            validate_arrow_leaf_types(item_field.name(), item_field.data_type())
        }
        DataType::Map(entry_field, _) => {
            if let DataType::Struct(entry_fields) = entry_field.data_type() {
                for f in entry_fields.iter() {
                    validate_arrow_leaf_types(f.name(), f.data_type())?;
                }
            }
            Ok(())
        }
        leaf if TypedBuilder::supports(leaf) => Ok(()),
        leaf => Err(WireToArrowError::UnsupportedArrowLeafType {
            name: field_name.to_string(),
            arrow_type: format!("{leaf:?}"),
        }),
    }
}

impl MessagePlan {
    /// Build a plan whose slots are all `Absent`. Used by the builder layer to
    /// shape a null-filled sub-tree when an outer Arrow Struct / List / Map
    /// column is itself `Absent` (so every nested child has to null-pad per row).
    pub(crate) fn all_absent(fields: &Fields) -> Self {
        let slots = (0..fields.len()).map(|_| PlanSlot::Absent).collect();
        MessagePlan {
            slots,
            slot_by_proto_field: Vec::new(),
            arrow_fields: fields.clone(),
            inside_map_entry: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::datatypes::{Field, Schema};
    use prost_reflect::DescriptorPool;
    use std::path::PathBuf;

    fn load_person_descriptor() -> MessageDescriptor {
        let desc_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data/protobuf/protos/test_protobuf.desc");
        let bytes = std::fs::read(&desc_path).expect("read desc");
        DescriptorPool::decode(bytes.as_slice())
            .expect("decode pool")
            .get_message_by_name("test_protobuf.Person")
            .expect("Person descriptor")
    }

    #[test]
    fn build_scalar_plan() {
        let desc = load_person_descriptor();
        let schema = Schema::new(vec![
            Field::new("name", DataType::LargeUtf8, true),
            Field::new("id", DataType::Int32, true),
            Field::new("email", DataType::LargeUtf8, true),
        ]);
        let plan = MessagePlan::build(&desc, &Fields::from(schema.fields().clone())).unwrap();
        assert_eq!(plan.slots.len(), 3);
        assert!(matches!(plan.slots[0], PlanSlot::Scalar(ScalarKind::String)));
        assert!(matches!(plan.slots[1], PlanSlot::Scalar(ScalarKind::Int32)));
        assert!(matches!(plan.slots[2], PlanSlot::Scalar(ScalarKind::String)));
    }

    #[test]
    fn missing_proto_field_yields_absent_slot() {
        // Schema-drift tolerance: if the Arrow schema declares a column the
        // proto descriptor doesn't carry, the plan builder logs + increments
        // a metric and emits a `PlanSlot::Absent` so the column comes out as
        // all-null rather than failing the batch. Typical cause: a field was
        // removed from the proto but the target schema still has the column.
        let desc = load_person_descriptor();
        let schema = Schema::new(vec![
            Field::new("name", DataType::LargeUtf8, true),
            Field::new("deleted_in_proto", DataType::Int32, true),
            Field::new("id", DataType::Int32, true),
        ]);
        let plan = MessagePlan::build(&desc, &Fields::from(schema.fields().clone())).unwrap();
        assert!(matches!(plan.slots[0], PlanSlot::Scalar(ScalarKind::String)));
        assert!(matches!(plan.slots[1], PlanSlot::Absent));
        assert!(matches!(plan.slots[2], PlanSlot::Scalar(ScalarKind::Int32)));
        // No proto tag for slot 1 — so the reverse index never points at it.
        assert!(plan.slot_by_proto_field.iter().all(|entry| *entry != 1));
    }

    #[test]
    fn unsupported_combination_flagged() {
        // Person.id is a scalar int32. If we claim it's a Struct in Arrow,
        // the plan builder should reject.
        let desc = load_person_descriptor();
        let schema = Schema::new(vec![Field::new(
            "id",
            DataType::Struct(Fields::from(vec![Field::new("x", DataType::Int32, true)])),
            true,
        )]);
        let err = MessagePlan::build(&desc, &Fields::from(schema.fields().clone()))
            .expect_err("should fail");
        assert!(matches!(err, WireToArrowError::UnsupportedCombination { .. }));
    }

    #[test]
    fn scalar_kind_arrow_type_mismatch_flagged() {
        // Person.name is a proto String; declaring its Arrow column as Int32
        // is a mis-paired schema. Plan-build must catch this so the runtime
        // appenders can assume the (ScalarKind, TypedBuilder) pairing is
        // well-formed and don't need a fallthrough that fails the whole batch.
        let desc = load_person_descriptor();
        let schema = Schema::new(vec![Field::new("name", DataType::Int32, true)]);
        let err = MessagePlan::build(&desc, &Fields::from(schema.fields().clone()))
            .expect_err("plan-build must reject String/Int32 pairing");
        assert!(
            matches!(err, WireToArrowError::UnsupportedCombination { .. }),
            "expected UnsupportedCombination, got {err:?}"
        );
    }

    #[test]
    fn wire_type_for_scalars() {
        assert_eq!(ScalarKind::Int32.wire_type(), WT_VARINT);
        assert_eq!(ScalarKind::String.wire_type(), WT_LEN);
        assert_eq!(ScalarKind::Double.wire_type(), WT_I64);
        assert_eq!(ScalarKind::Float.wire_type(), WT_I32);
    }
}
