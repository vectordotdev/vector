use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use metrics::{atomics::AtomicU64, GaugeFn, HistogramFn};
use metrics_util::registry::Storage;

use crate::event::{metric::Bucket, MetricValue};

pub(super) struct VectorStorage;

impl<K> Storage<K> for VectorStorage {
    type Counter = Arc<AtomicU64>;
    type Gauge = Arc<AtomicF64>;
    type Histogram = Arc<Histogram>;

    fn counter(&self, _: &K) -> Self::Counter {
        Arc::new(AtomicU64::new(0))
    }

    fn gauge(&self, _: &K) -> Self::Gauge {
        Arc::new(AtomicF64::new(0.0))
    }

    fn histogram(&self, _: &K) -> Self::Histogram {
        Arc::new(Histogram::new())
    }
}

#[derive(Debug)]
pub(super) struct AtomicF64 {
    inner: AtomicU64,
}

impl AtomicF64 {
    fn new(init: f64) -> Self {
        Self {
            inner: AtomicU64::new(init.to_bits()),
        }
    }

    fn fetch_update(
        &self,
        set_order: Ordering,
        fetch_order: Ordering,
        mut f: impl FnMut(f64) -> f64,
    ) {
        self.inner
            .fetch_update(set_order, fetch_order, |x| {
                Some(f(f64::from_bits(x)).to_bits())
            })
            .expect("Cannot fail");
    }

    pub(super) fn load(&self, order: Ordering) -> f64 {
        f64::from_bits(self.inner.load(order))
    }
}

impl GaugeFn for AtomicF64 {
    fn increment(&self, amount: f64) {
        self.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| value + amount);
    }

    fn decrement(&self, amount: f64) {
        self.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| value - amount);
    }

    fn set(&self, value: f64) {
        self.inner.store(f64::to_bits(value), Ordering::Relaxed);
    }
}

#[derive(Debug)]
pub(super) struct Histogram {
    buckets: Box<[(f64, AtomicU32); 20]>,
    count: AtomicU64,
    sum: AtomicF64,
}

impl Histogram {
    const MIN_BUCKET: f64 = 0.015_625; // (-6_f64).exp2() is not const yet
    const MIN_BUCKET_EXP: f64 = -6.0;
    const BUCKETS: usize = 20;

    pub(crate) fn new() -> Self {
        // Box to avoid having this large array inline to the structure, blowing
        // out cache coherence.
        //
        // The sequence here is based on powers of two. Other sequences are more
        // suitable for different distributions but since our present use case
        // is mostly non-negative and measures smallish latencies we cluster
        // around but never quite get to zero with an increasingly coarse
        // long-tail. This also lets us find the right bucket to record into using simple
        // constant-time math operations instead of a loop-and-compare construct.
        let buckets = Box::new([
            ((-6_f64).exp2(), AtomicU32::new(0)),
            ((-5_f64).exp2(), AtomicU32::new(0)),
            ((-4_f64).exp2(), AtomicU32::new(0)),
            ((-3_f64).exp2(), AtomicU32::new(0)),
            ((-2_f64).exp2(), AtomicU32::new(0)),
            ((-1_f64).exp2(), AtomicU32::new(0)),
            (0_f64.exp2(), AtomicU32::new(0)),
            (1_f64.exp2(), AtomicU32::new(0)),
            (2_f64.exp2(), AtomicU32::new(0)),
            (3_f64.exp2(), AtomicU32::new(0)),
            (4_f64.exp2(), AtomicU32::new(0)),
            (5_f64.exp2(), AtomicU32::new(0)),
            (6_f64.exp2(), AtomicU32::new(0)),
            (7_f64.exp2(), AtomicU32::new(0)),
            (8_f64.exp2(), AtomicU32::new(0)),
            (9_f64.exp2(), AtomicU32::new(0)),
            (10_f64.exp2(), AtomicU32::new(0)),
            (11_f64.exp2(), AtomicU32::new(0)),
            (12_f64.exp2(), AtomicU32::new(0)),
            (f64::INFINITY, AtomicU32::new(0)),
        ]);
        Self {
            buckets,
            count: AtomicU64::new(0),
            sum: AtomicF64::new(0.0),
        }
    }

    pub(self) fn bucket_index(value: f64) -> usize {
        // The buckets are all powers of two, so compute the ceiling of the log_2 of the
        // value. Apply a lower bound to prevent zero or negative values from blowing up the log.
        let log = value.max(Self::MIN_BUCKET).log2().ceil();
        // Offset it based on the minimum bucket's exponent. The result will be non-negative thanks
        // to the `.max` above, so we can coerce it directly to `usize`.
        #[allow(clippy::cast_possible_truncation)] // The log will always be smaller than `usize`.
        let index = (log - Self::MIN_BUCKET_EXP) as usize;
        // Now bound the value for values larger than the largest bucket.
        index.min(Self::BUCKETS - 1)
    }

    pub(super) fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    pub(super) fn sum(&self) -> f64 {
        self.sum.load(Ordering::Relaxed)
    }

    fn buckets(&self) -> Vec<Bucket> {
        self.buckets
            .iter()
            .map(|(upper_limit, count)| Bucket {
                upper_limit: *upper_limit,
                count: u64::from(count.load(Ordering::Relaxed)),
            })
            .collect()
    }

    pub(super) fn make_metric(&self) -> MetricValue {
        MetricValue::AggregatedHistogram {
            buckets: self.buckets(),
            count: self.count(),
            sum: self.sum(),
        }
    }
}

impl HistogramFn for Histogram {
    fn record(&self, value: f64) {
        let index = Self::bucket_index(value);
        self.buckets[index].1.fetch_add(1, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
        self.sum
            .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |cur| cur + value);
    }
}

#[cfg(test)]
mod test {
    use metrics::HistogramFn;
    use quickcheck::{QuickCheck, TestResult};

    use super::Histogram;

    // Adapted from https://users.rust-lang.org/t/assert-eq-for-float-numbers/7034/4?u=blt
    fn nearly_equal(a: f64, b: f64) -> bool {
        let abs_a = a.abs();
        let abs_b = b.abs();
        let diff = (a - b).abs();

        if a == b {
            // Handle infinities.
            true
        } else if a == 0.0 || b == 0.0 || diff < f64::MIN_POSITIVE {
            // One of a or b is zero (or both are extremely close to it,) use absolute error.
            diff < (f64::EPSILON * f64::MIN_POSITIVE)
        } else {
            // Use relative error.
            (diff / f64::min(abs_a + abs_b, f64::MAX)) < f64::EPSILON
        }
    }

    #[test]
    #[allow(clippy::needless_pass_by_value)] // `&[T]` does not implement `Arbitrary`
    fn histogram() {
        fn inner(values: Vec<f64>) -> TestResult {
            let sut = Histogram::new();
            let mut model_count: u64 = 0;
            let mut model_sum: f64 = 0.0;

            for value in values {
                if value.is_infinite() || value.is_nan() {
                    continue;
                }

                let index = Histogram::bucket_index(value);
                assert!(
                    value <= sut.buckets[index].0,
                    "Value {} is not less than the upper limit {}.",
                    value,
                    sut.buckets[index].0
                );
                if index > 0 {
                    assert!(
                        value > sut.buckets[index - 1].0,
                        "Value {} is not greater than the previous upper limit {}.",
                        value,
                        sut.buckets[index - 1].0
                    );
                }

                sut.record(value);
                model_count = model_count.wrapping_add(1);
                model_sum += value;

                assert_eq!(sut.count(), model_count);
                assert!(nearly_equal(sut.sum(), model_sum));
            }
            TestResult::passed()
        }

        QuickCheck::new()
            .tests(1_000)
            .max_tests(2_000)
            .quickcheck(inner as fn(Vec<f64>) -> TestResult);
    }
}
