use metrics::IntoF64;

/// Wraps an integer metric value so it can be passed to gauges and histograms.
///
/// `metrics` intentionally does not implement [`IntoF64`] for 64-bit and pointer-sized integers,
/// because integers with a magnitude above 2^53 may lose precision as `f64`. That loss is
/// acceptable for metric values, so wrapping a value in `LossyF64` opts into the conversion
/// explicitly.
#[derive(Clone, Copy, Debug)]
pub struct LossyF64<T>(pub T);

impl IntoF64 for LossyF64<u64> {
    #[expect(
        clippy::cast_precision_loss,
        reason = "Metric values are `f64`; precision loss above 2^53 is acceptable"
    )]
    fn into_f64(self) -> f64 {
        self.0 as f64
    }
}

impl IntoF64 for LossyF64<i64> {
    #[expect(
        clippy::cast_precision_loss,
        reason = "Metric values are `f64`; precision loss above 2^53 is acceptable"
    )]
    fn into_f64(self) -> f64 {
        self.0 as f64
    }
}

impl IntoF64 for LossyF64<usize> {
    #[expect(
        clippy::cast_precision_loss,
        reason = "Metric values are `f64`; precision loss above 2^53 is acceptable"
    )]
    fn into_f64(self) -> f64 {
        self.0 as f64
    }
}

#[cfg(test)]
mod tests {
    use metrics::IntoF64;

    use super::LossyF64;

    #[test]
    fn converts_exactly_up_to_2_pow_53() {
        assert_eq!(LossyF64(42_usize).into_f64().to_bits(), 42.0_f64.to_bits());
        assert_eq!(
            LossyF64(1_u64 << 53).into_f64().to_bits(),
            9_007_199_254_740_992.0_f64.to_bits()
        );
        assert_eq!(LossyF64(-7_i64).into_f64().to_bits(), (-7.0_f64).to_bits());
    }
}
