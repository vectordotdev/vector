// Maximum integer value that can be represented exactly in f64 (2^53).
pub const F64_SAFE_INT_MAX: i64 = 1_i64 << 53;

pub fn u64_to_f64_safe(value: u64) -> f64 {
    let capped = value.min(F64_SAFE_INT_MAX as u64);
    #[allow(clippy::cast_precision_loss)]
    {
        capped as f64
    }
}
