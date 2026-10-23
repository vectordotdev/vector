/// Converts an integer to the `f64` value recorded by gauges and histograms.
///
/// Integers with a magnitude above 2^53 lose precision. That is acceptable for metric values, which
/// are stored as `f64` anyway.
pub trait ToF64Lossy {
    fn to_f64_lossy(self) -> f64;
}

macro_rules! impl_to_f64_lossy {
    ($($ty:ty),*) => {
        $(
            impl ToF64Lossy for $ty {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "metric values are f64, so precision loss above 2^53 is accepted"
                )]
                fn to_f64_lossy(self) -> f64 {
                    self as f64
                }
            }
        )*
    };
}

impl_to_f64_lossy!(usize, u64, i64);

#[cfg(test)]
mod tests {
    use super::ToF64Lossy;

    #[test]
    fn converts_exactly_up_to_2_pow_53() {
        assert_eq!(42_usize.to_f64_lossy().to_bits(), 42.0_f64.to_bits());
        assert_eq!(
            (1_u64 << 53).to_f64_lossy().to_bits(),
            9_007_199_254_740_992.0_f64.to_bits()
        );
        assert_eq!((-7_i64).to_f64_lossy().to_bits(), (-7.0_f64).to_bits());
    }
}
