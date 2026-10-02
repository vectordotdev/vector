//! VictoriaMetrics `vmrange` histograms.
//!
//! This is a port of the bucket layout in [`VictoriaMetrics/metrics`][histogram_go]. Buckets are
//! log-scale with 18 buckets per power of ten between `1e-9` and `1e18`, plus a lower and an upper
//! overflow bucket. Each bucket is exposed as a `<name>_bucket{vmrange="<start>...<end>"}` series.
//!
//! The bucket index and the label text must match the Go implementation exactly, otherwise series
//! written by Vector do not merge with series written by applications instrumented with the
//! VictoriaMetrics client libraries.
//!
//! [histogram_go]: https://github.com/VictoriaMetrics/metrics/blob/master/histogram.go

use std::{collections::BTreeMap, sync::LazyLock};

use vector_lib::{ByteSizeOf, event::metric::Sample};

const E10_MIN: i32 = -9;
const E10_MAX: i32 = 18;
const BUCKETS_PER_DECIMAL: usize = 18;
const BUCKETS_COUNT: usize = (E10_MAX - E10_MIN) as usize * BUCKETS_PER_DECIMAL;

/// Slot of the lower overflow bucket, holding values below `1e-9` (including zero).
const LOWER_SLOT: u16 = 0;
/// Slot of the upper overflow bucket, holding values of `1e18` and above.
const UPPER_SLOT: u16 = BUCKETS_COUNT as u16 + 1;

/// `math.Pow(10, 1.0/18)` as computed by Go.
///
/// Rust's `powf` is not guaranteed to produce the same last bit, and the bucket bounds are derived
/// by repeated multiplication, so the Go value is pinned.
const BUCKET_MULTIPLIER: f64 = f64::from_bits(0x3ff2_2ef4_8683_80ca);

/// `1 / math.Ln10` as computed by Go.
const INV_LN10: f64 = f64::from_bits(0x3fdb_cb7b_1526_e50e);

/// Labels indexed by slot: the lower bucket, the regular buckets, then the upper bucket.
static LABELS: LazyLock<Vec<String>> = LazyLock::new(|| {
    let mut labels = Vec::with_capacity(BUCKETS_COUNT + 2);
    labels.push(format!("0...{}", format_go_exp(1e-9)));

    let mut value = 1e-9;
    let mut start = format_go_exp(value);
    for _ in 0..BUCKETS_COUNT {
        value *= BUCKET_MULTIPLIER;
        let end = format_go_exp(value);
        labels.push(format!("{start}...{end}"));
        start = end;
    }

    labels.push(format!("{}...+Inf", format_go_exp(1e18)));
    labels
});

/// Formats `value` like Go's `fmt.Sprintf("%.3e", value)`.
///
/// Rust's `{:.3e}` prints the exponent without padding (`1.000e-9`), while Go always prints a sign
/// and at least two digits (`1.000e-09`).
fn format_go_exp(value: f64) -> String {
    let formatted = format!("{value:.3e}");
    let (mantissa, exponent) = formatted
        .split_once('e')
        .expect("exponential formatting always contains an exponent");
    let exponent: i32 = exponent
        .parse()
        .expect("exponential formatting always produces an integer exponent");
    let sign = if exponent < 0 { '-' } else { '+' };
    format!("{mantissa}e{sign}{:02}", exponent.unsigned_abs())
}

/// Returns the slot a value is recorded in, or `None` for NaN and negative values.
fn slot(value: f64) -> Option<u16> {
    if value.is_nan() || value < 0.0 {
        return None;
    }

    // Go computes `math.Log10(v)` as `math.Log(v) * (1 / math.Ln10)`. The multiplication and the
    // subtraction are kept separate: Go on amd64 does not fuse them, and it is the reference
    // behavior. (Go on arm64 fuses them into a single FMA, which moves some exact powers of ten
    // into the next bucket.)
    let log10 = value.ln() * INV_LN10;
    let index = (log10 - f64::from(E10_MIN)) * BUCKETS_PER_DECIMAL as f64;

    if index < 0.0 {
        Some(LOWER_SLOT)
    } else if index >= BUCKETS_COUNT as f64 {
        Some(UPPER_SLOT)
    } else {
        let mut bucket = index as usize;
        // Exact powers of ten belong to the lower bucket, matching `le`-based histograms.
        if index == bucket as f64 && bucket > 0 {
            bucket -= 1;
        }
        Some(bucket as u16 + 1)
    }
}

/// A cumulative `vmrange` histogram.
///
/// Only non-empty buckets are stored, so a histogram holds at most 488 counters regardless of how
/// many values were recorded.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct VmHistogram {
    buckets: BTreeMap<u16, u64>,
    sum: f64,
    count: u64,
}

impl VmHistogram {
    /// Creates a histogram from distribution samples, counting each sample `rate` times.
    pub(super) fn from_samples(samples: &[Sample]) -> Self {
        let mut histogram = Self::default();
        histogram.record_samples(samples);
        histogram
    }

    /// Records distribution samples, counting each sample `rate` times.
    pub(super) fn record_samples(&mut self, samples: &[Sample]) {
        for sample in samples {
            self.record(sample.value, sample.rate);
        }
    }

    /// Records `value` `times` times. NaN and negative values are ignored.
    pub(super) fn record(&mut self, value: f64, times: u32) {
        if times == 0 {
            return;
        }
        if let Some(slot) = slot(value) {
            *self.buckets.entry(slot).or_default() += u64::from(times);
            self.sum += value * f64::from(times);
            self.count += u64::from(times);
        }
    }

    /// Iterates over the non-empty buckets in ascending order, yielding each `vmrange` label and
    /// its count.
    pub(super) fn buckets(&self) -> impl Iterator<Item = (&'static str, u64)> + '_ {
        self.buckets
            .iter()
            .map(|(slot, count)| (LABELS[usize::from(*slot)].as_str(), *count))
    }

    pub(super) const fn sum(&self) -> f64 {
        self.sum
    }

    pub(super) const fn count(&self) -> u64 {
        self.count
    }

    /// The number of non-empty buckets.
    pub(super) fn bucket_len(&self) -> usize {
        self.buckets.len()
    }
}

impl ByteSizeOf for VmHistogram {
    fn allocated_bytes(&self) -> usize {
        // A `BTreeMap` node stores keys and values inline; this approximates the per-entry cost.
        self.buckets.len() * (size_of::<u16>() + size_of::<u64>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOLDEN: &str = include_str!("../../../tests/data/victoriametrics/vmrange_golden.txt");

    fn golden_lines(kind: &str) -> impl Iterator<Item = (&'static str, &'static str)> {
        GOLDEN
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .filter_map(move |line| {
                let mut parts = line.splitn(3, ' ');
                (parts.next() == Some(kind)).then(|| (parts.next().unwrap(), parts.next().unwrap()))
            })
    }

    fn label_for(value: f64) -> Option<&'static str> {
        slot(value).map(|slot| LABELS[usize::from(slot)].as_str())
    }

    #[test]
    fn labels_match_go() {
        let ranges = golden_lines("range").collect::<Vec<_>>();
        assert_eq!(ranges.len(), BUCKETS_COUNT);
        for (index, label) in ranges {
            let index: usize = index.parse().unwrap();
            assert_eq!(LABELS[index + 1], label, "bucket {index}");
        }
        assert_eq!(LABELS[usize::from(LOWER_SLOT)], "0...1.000e-09");
        assert_eq!(LABELS[usize::from(UPPER_SLOT)], "1.000e+18...+Inf");
    }

    #[test]
    fn value_buckets_match_go() {
        let mut checked = 0;
        for (input, expected) in golden_lines("value") {
            let value = match input {
                "NaN" => f64::NAN,
                "+Inf" => f64::INFINITY,
                other => other.parse::<f64>().unwrap(),
            };
            let actual = label_for(value).unwrap_or("skip");
            assert_eq!(actual, expected, "value {input}");
            checked += 1;
        }
        assert!(checked > 50);
    }

    #[test]
    fn formats_like_go() {
        assert_eq!(format_go_exp(1e-9), "1.000e-09");
        assert_eq!(format_go_exp(1.0), "1.000e+00");
        assert_eq!(format_go_exp(1e18), "1.000e+18");
        assert_eq!(format_go_exp(1e100), "1.000e+100");
        assert_eq!(format_go_exp(0.12345), "1.235e-01");
    }

    #[test]
    fn records_with_rate() {
        let mut histogram = VmHistogram::default();
        histogram.record(0.5, 3);
        histogram.record(0.5, 1);
        histogram.record(20.0, 1);

        assert_eq!(histogram.count(), 5);
        assert_eq!(histogram.sum(), 22.0);
        assert_eq!(
            histogram.buckets().collect::<Vec<_>>(),
            vec![("4.642e-01...5.275e-01", 4), ("1.896e+01...2.154e+01", 1)]
        );
    }

    #[test]
    fn ignores_nan_negative_and_zero_rate() {
        let mut histogram = VmHistogram::default();
        histogram.record(f64::NAN, 1);
        histogram.record(-1.0, 1);
        histogram.record(1.0, 0);

        assert_eq!(histogram, VmHistogram::default());
    }

    #[test]
    fn overflow_buckets() {
        let mut histogram = VmHistogram::default();
        histogram.record(0.0, 1);
        histogram.record(1e30, 2);

        assert_eq!(
            histogram.buckets().collect::<Vec<_>>(),
            vec![("0...1.000e-09", 1), ("1.000e+18...+Inf", 2)]
        );
    }

    #[test]
    fn bucket_count_is_bounded() {
        let mut histogram = VmHistogram::default();
        let mut value = 1e-12;
        while value < 1e21 {
            histogram.record(value, 1);
            value *= 1.01;
        }
        assert_eq!(histogram.bucket_len(), BUCKETS_COUNT + 2);
    }
}
