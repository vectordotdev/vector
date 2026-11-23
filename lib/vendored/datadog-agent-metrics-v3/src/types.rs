//! V3 payload type definitions and protocol buffer field numbers.

/// V3 metric type values.
///
/// These match the `metricType` enum in `intake_v3.proto`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum V3MetricType {
    /// A monotonically increasing counter, submitted as per-interval deltas.
    Count = 1,
    /// A count normalized to a per-second rate.
    Rate = 2,
    /// A point-in-time value.
    Gauge = 3,
    /// A distribution summarized as a DDSketch.
    Sketch = 4,
}

impl V3MetricType {
    /// Returns the numeric value for encoding in the types column.
    pub const fn as_u64(self) -> u64 {
        self as u64
    }
}

/// V3 value type values.
///
/// These are encoded in bits 4-7 of the types column and indicate which
/// value array contains the metric's points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum V3ValueType {
    /// Value is zero, not stored explicitly.
    Zero = 0x00,

    /// Value is stored in vals_sint64.
    Sint64 = 0x10,

    /// Value is stored in vals_float32.
    Float32 = 0x20,

    /// Value is stored in vals_float64.
    Float64 = 0x30,
}

impl V3ValueType {
    /// Returns the numeric value for encoding in the types column.
    pub fn as_u64(self) -> u64 {
        self as u64
    }
}

/// Intermediate point classification for value type compaction.
///
/// This provides finer-grained classification than [`V3ValueType`] to avoid
/// precision loss when combining different value types. In particular, it
/// distinguishes small integers (that fit losslessly in f32) from large integers
/// (that don't), so that mixing a large integer with a Float32 value correctly
/// escalates to Float64 rather than silently truncating the integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
enum PointKind {
    /// Value is zero.
    Zero = 0,
    /// Integer with |v| <= 2^24, fits losslessly in both sint64 and f32.
    Int24 = 1,
    /// Integer with |v| > 2^24, fits in sint64 varint but NOT losslessly in f32.
    Int48 = 2,
    /// Fractional value exactly representable as f32.
    Float32 = 3,
    /// Everything else - requires full f64 precision.
    Float64 = 4,
}

/// Maximum integer magnitude that fits losslessly in f32 (2^24).
const F32_INT_MAX: i64 = 1 << 24;

impl PointKind {
    /// Classifies a single f64 value.
    fn for_value(v: f64) -> Self {
        if v == 0.0 {
            return Self::Zero;
        }

        // Varint range that fits in 7 bytes or less (49 bits).
        const VARINT_WIDTH: i32 = 7 * 7 - 1;
        const MAX_INT: i64 = 1 << VARINT_WIDTH;
        const MIN_INT: i64 = -MAX_INT;

        let i = v as i64;
        if (MIN_INT..MAX_INT).contains(&i) && (i as f64) == v {
            if (-F32_INT_MAX..=F32_INT_MAX).contains(&i) {
                return Self::Int24;
            }
            return Self::Int48;
        }

        if (v as f32 as f64) == v {
            return Self::Float32;
        }

        Self::Float64
    }

    /// Combines two point kinds into the smallest kind that can represent both.
    ///
    /// This is `max(self, other)` in all cases **except**:
    /// - `Int48 + Float32 = Float64` (and vice versa), because large integers
    ///   lose precision in f32, and fractional values can't be stored as sint64.
    fn union(self, other: Self) -> Self {
        match (self, other) {
            (Self::Int48, Self::Float32) | (Self::Float32, Self::Int48) => Self::Float64,
            _ => self.max(other),
        }
    }

    /// Converts to the wire-format value type.
    fn to_value_type(self) -> V3ValueType {
        match self {
            Self::Zero => V3ValueType::Zero,
            Self::Int24 | Self::Int48 => V3ValueType::Sint64,
            Self::Float32 => V3ValueType::Float32,
            Self::Float64 => V3ValueType::Float64,
        }
    }
}

/// Determines the best [`V3ValueType`] for a set of f64 values.
///
/// Uses [`PointKind`] internally to avoid precision loss when mixing
/// large integers with fractional float32 values.
pub(crate) fn value_type_for_values(values: impl Iterator<Item = f64>) -> V3ValueType {
    let mut kind = PointKind::Zero;
    for v in values {
        kind = kind.union(PointKind::for_value(v));
    }
    kind.to_value_type()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_kind_classification() {
        // Zero
        assert_eq!(PointKind::for_value(0.0), PointKind::Zero);

        // Small integers (fit in f32)
        assert_eq!(PointKind::for_value(100.0), PointKind::Int24);
        assert_eq!(PointKind::for_value(-100.0), PointKind::Int24);
        assert_eq!(PointKind::for_value((1 << 24) as f64), PointKind::Int24);
        assert_eq!(PointKind::for_value(-((1 << 24) as f64)), PointKind::Int24);

        // Large integers (don't fit losslessly in f32)
        assert_eq!(PointKind::for_value(((1 << 24) + 1) as f64), PointKind::Int48);
        assert_eq!(PointKind::for_value((1i64 << 30) as f64), PointKind::Int48);

        // Float32
        assert_eq!(PointKind::for_value(1.5), PointKind::Float32);
        assert_eq!(PointKind::for_value(2.75), PointKind::Float32);

        // Float64
        assert_eq!(PointKind::for_value(std::f64::consts::PI), PointKind::Float64);
        let large = ((1i64 << 50) + 1) as f64;
        assert_eq!(PointKind::for_value(large), PointKind::Float64);
    }

    #[test]
    fn test_point_kind_union() {
        // Standard widening (max)
        assert_eq!(PointKind::Zero.union(PointKind::Int24), PointKind::Int24);
        assert_eq!(PointKind::Int24.union(PointKind::Int48), PointKind::Int48);
        assert_eq!(PointKind::Int24.union(PointKind::Float32), PointKind::Float32);
        assert_eq!(PointKind::Float32.union(PointKind::Float64), PointKind::Float64);
        assert_eq!(PointKind::Float64.union(PointKind::Zero), PointKind::Float64);

        // The critical case: large integer + float32 must escalate to float64
        assert_eq!(PointKind::Int48.union(PointKind::Float32), PointKind::Float64);
        assert_eq!(PointKind::Float32.union(PointKind::Int48), PointKind::Float64);
    }

    #[test]
    fn test_value_type_for_values() {
        // All zeros
        assert_eq!(value_type_for_values([0.0, 0.0].into_iter()), V3ValueType::Zero);

        // Small integers
        assert_eq!(value_type_for_values([100.0, 200.0].into_iter()), V3ValueType::Sint64);

        // Large integers
        assert_eq!(
            value_type_for_values([(1i64 << 30) as f64, 200.0].into_iter()),
            V3ValueType::Sint64
        );

        // Small integer + float32 → Float32 (safe, small int fits in f32)
        assert_eq!(value_type_for_values([100.0, 1.5].into_iter()), V3ValueType::Float32);

        // Large integer + float32 → Float64 (the bug fix!)
        assert_eq!(
            value_type_for_values([(1i64 << 30) as f64, 1.5].into_iter()),
            V3ValueType::Float64
        );

        // Float64 value forces Float64
        assert_eq!(
            value_type_for_values([100.0, std::f64::consts::PI].into_iter()),
            V3ValueType::Float64
        );

        // Empty iterator
        assert_eq!(value_type_for_values(std::iter::empty()), V3ValueType::Zero);
    }
}
