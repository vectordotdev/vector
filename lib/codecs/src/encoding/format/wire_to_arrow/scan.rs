//! Per-message wire walks.
//!
//! [`scan_message`] is the hot path — it walks one proto message's wire
//! bytes and appends decoded values into the matching Arrow column
//! builders. [`validate_message`] is the side-effect-free mirror used by
//! `WireToArrowEncoder::encode_batch` to detect malformed rows before
//! any builder is mutated, so a single poison row can be dropped without
//! poisoning the whole batch.

use super::wire::try_parse_field;

use super::append::{
    append_enum_string_from_wire, append_repeated_enum_string, append_repeated_scalar,
    append_scalar_from_wire, expect_len, validate_enum_string_from_wire,
    validate_repeated_enum_string, validate_repeated_scalar,
    validate_scalar_from_wire,
};
use super::builders::{self, BuilderNodeList};
use super::errors::{Result, WireToArrowError};
use super::plan::{MessagePlan, PlanSlot, SLOT_UNKNOWN};

/// Scan one proto message's wire bytes, appending values into `builders`.
///
/// Sets `builders.present[i] = true` for each slot `i` touched by any tag in
/// this message. The caller is responsible for resetting `present` (via
/// [`BuilderNodeList::reset_present`]) before invoking, and for calling
/// [`BuilderNodeList::finalize_row`] afterwards. Sub-messages reuse their own
/// level's `present` buffer, so no per-occurrence allocation happens on the
/// hot path.
pub(super) fn scan_message(
    plan: &MessagePlan,
    mut bytes: &[u8],
    builders: &mut BuilderNodeList,
) -> Result<()> {
    let dispatch_table = plan.slot_by_proto_field.as_slice();
    while !bytes.is_empty() {
        let (field, rest) = try_parse_field(bytes)?;
        bytes = rest;
        let field_number = field.field_num as usize;

        // Out-of-range or unknown field number — skip; `try_parse_field`
        // already consumed the value.
        let Some(&slot_idx) = dispatch_table.get(field_number) else {
            continue;
        };
        if slot_idx == SLOT_UNKNOWN {
            continue;
        }
        let slot_idx = slot_idx as usize;

        let BuilderNodeList { nodes, present } = &mut *builders;
        match &mut nodes[slot_idx] {
            builders::BuilderNode::Scalar { kind, builder } => {
                append_scalar_from_wire(*kind, &field.value, builder)?;
                present[slot_idx] = true;
            }
            builders::BuilderNode::EnumString { desc, builder } => {
                append_enum_string_from_wire(desc, &field.value, builder)?;
                present[slot_idx] = true;
            }
            builders::BuilderNode::Struct {
                sub_plan, children, ..
            } => {
                let sub_bytes = expect_len(&field.value)?;
                children.reset_present();
                scan_message(sub_plan, sub_bytes, children)?;
                children.finalize_row(sub_plan);
                present[slot_idx] = true;
            }
            builders::BuilderNode::RepeatedMessage {
                sub_plan,
                children,
                current_offset,
                ..
            }
            | builders::BuilderNode::Map {
                sub_plan,
                children,
                current_offset,
                ..
            } => {
                let sub_bytes = expect_len(&field.value)?;
                children.reset_present();
                scan_message(sub_plan, sub_bytes, children)?;
                children.finalize_row(sub_plan);
                // Cumulative across the batch — checked_add converts what
                // would otherwise be a release-mode wrap + OffsetBuffer
                // assertion (process panic at finish) into a clean batch
                // failure. Per-row validate doesn't catch this case because
                // the overflow can be aggregate across many small rows.
                *current_offset =
                    current_offset
                        .checked_add(1)
                        .ok_or(WireToArrowError::OffsetOverflow {
                            site: "scan_message:repeated_message_or_map",
                        })?;
                present[slot_idx] = true;
            }
            builders::BuilderNode::RepeatedScalar {
                kind,
                values,
                current_offset,
                ..
            } => {
                append_repeated_scalar(*kind, &field.value, values, current_offset)?;
                present[slot_idx] = true;
            }
            builders::BuilderNode::RepeatedEnumString {
                desc,
                values,
                current_offset,
                ..
            } => {
                append_repeated_enum_string(desc, &field.value, values, current_offset)?;
                present[slot_idx] = true;
            }
        }
    }
    Ok(())
}

/// Walk one proto message's wire bytes without touching any builders, surfacing
/// every decode error that [`scan_message`] would produce for the same input.
/// [`WireToArrowEncoder::encode_batch`] runs this as a pre-pass per message so
/// rows that fail can be dropped from the batch cleanly — no half-appended
/// leaves, no finalized nested sub-rows — and replaced with a `dropped`
/// counter instead of failing the entire batch.
///
/// The two-pass cost is acceptable because (a) the parse walk is small
/// relative to value appends + buffer growth on the real scan, and (b) Arrow
/// `*Builder` types expose no public rollback API, so an in-place
/// "snapshot + truncate on error" alternative isn't viable.
///
/// Must stay in lock-step with [`scan_message`]: any wire byte sequence that
/// is accepted here must also be accepted there, and vice versa. If the two
/// diverge (validate accepts but scan errors), the real scan's `?` in
/// `encode_batch` will bubble it out as a batch-level failure — that's a
/// clear signal of a code bug rather than user input.
///
/// [`WireToArrowEncoder::encode_batch`]: super::encoder::WireToArrowEncoder::encode_batch
pub(super) fn validate_message(plan: &MessagePlan, mut bytes: &[u8]) -> Result<()> {
    // Track which singular slots (Scalar / Struct) have been seen in this
    // message so a duplicate tag drops the row instead of corrupting
    // column-length alignment downstream in scan_message. Proto3 parsers
    // are required to accept duplicate singular tags (last-wins for
    // scalars, merge for sub-messages), but Arrow `*Builder` types don't
    // expose retraction, so implementing last-wins would require either
    // per-row scratch buffers or a lookahead pass. Dropping the row
    // preserves per-row isolation; the wire_to_arrow_rows_dropped metric
    // gives operators a signal if real producers start tripping this.
    //
    // Stack-allocated for plans with <=128 slots per level (covers every
    // realistic Arrow schema we encode); heap-allocated bitvec for wider
    // plans. Common case is zero allocations on the hot path.
    let mut seen_singular = SeenSingular::with_capacity(plan.slots.len());

    while !bytes.is_empty() {
        let (field, rest) = try_parse_field(bytes)?;
        bytes = rest;
        let field_number = field.field_num as usize;

        let Some(&slot_idx) = plan.slot_by_proto_field.get(field_number) else {
            continue;
        };
        if slot_idx == SLOT_UNKNOWN {
            continue;
        }
        let slot_idx = slot_idx as usize;
        let slot = &plan.slots[slot_idx];

        match slot {
            PlanSlot::Scalar(sk) => {
                if seen_singular.test_and_set(slot_idx) {
                    return Err(WireToArrowError::DuplicateSingularField {
                        field_number: field.field_num as u32,
                    });
                }
                validate_scalar_from_wire(*sk, &field.value)?;
            }
            PlanSlot::Struct(sub_plan) => {
                if seen_singular.test_and_set(slot_idx) {
                    return Err(WireToArrowError::DuplicateSingularField {
                        field_number: field.field_num as u32,
                    });
                }
                let sub_bytes = expect_len(&field.value)?;
                validate_message(sub_plan, sub_bytes)?;
            }
            PlanSlot::RepeatedMessage(sub_plan) | PlanSlot::Map(sub_plan) => {
                let sub_bytes = expect_len(&field.value)?;
                validate_message(sub_plan, sub_bytes)?;
            }
            PlanSlot::RepeatedScalar(sk) => validate_repeated_scalar(*sk, &field.value)?,
            PlanSlot::RepeatedEnumString(desc) => {
                validate_repeated_enum_string(desc, &field.value)?
            }
            PlanSlot::EnumString(desc) => {
                if seen_singular.test_and_set(slot_idx) {
                    return Err(WireToArrowError::DuplicateSingularField {
                        field_number: field.field_num as u32,
                    });
                }
                validate_enum_string_from_wire(desc, &field.value)?;
            }
            // No proto field number ever points at an Absent slot (Absent
            // slots are Arrow columns the proto descriptor lacks), so this
            // arm is unreachable in practice. Mirror `scan_message`'s
            // fall-through and surface it as a code-bug signal.
            PlanSlot::Absent => {
                return Err(WireToArrowError::PlanBuilderMismatch {
                    site: "validate_message:absent_slot_unreachable",
                });
            }
        }
    }
    Ok(())
}

/// Bitset for tracking which singular slots have already been seen in one
/// `validate_message` call. Inline `u128` covers plans with up to 128
/// singular Scalar/Struct slots per level — every realistic Arrow schema
/// fits — so the common case is allocation-free on the per-row hot path.
/// Wider plans fall back to a heap `Vec<u64>`.
enum SeenSingular {
    Small(u128),
    Large(Vec<u64>),
}

impl SeenSingular {
    #[inline]
    fn with_capacity(slot_count: usize) -> Self {
        if slot_count <= 128 {
            Self::Small(0)
        } else {
            Self::Large(vec![0u64; slot_count.div_ceil(64)])
        }
    }

    /// Set the bit for `idx` and return whether it was already set.
    /// Used by `validate_message` to detect duplicate singular tags
    /// (`true` on the second occurrence of any Scalar/Struct slot).
    #[inline]
    fn test_and_set(&mut self, idx: usize) -> bool {
        match self {
            Self::Small(bits) => {
                let mask = 1u128 << idx;
                let already = (*bits & mask) != 0;
                *bits |= mask;
                already
            }
            Self::Large(words) => {
                let word = &mut words[idx / 64];
                let mask = 1u64 << (idx % 64);
                let already = (*word & mask) != 0;
                *word |= mask;
                already
            }
        }
    }
}
