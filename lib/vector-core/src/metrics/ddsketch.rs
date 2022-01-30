use std::{
    cmp::{self, Ordering},
    mem,
};

use float_eq::FloatEq;
use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use vector_common::byte_size_of::ByteSizeOf;

use crate::event::{metric::Bucket, Metric, MetricValue};

const AGENT_DEFAULT_BIN_LIMIT: u16 = 4096;
const AGENT_DEFAULT_EPS: f64 = 1.0 / 128.0;
const AGENT_DEFAULT_MIN_VALUE: f64 = 1.0e-9;

const UV_INF: i16 = i16::MAX;
const MAX_KEY: i16 = UV_INF;

const INITIAL_BINS: u16 = 128;
const MAX_BIN_WIDTH: u16 = u16::MAX;

#[inline]
fn log_gamma(gamma_ln: f64, v: f64) -> f64 {
    v.ln() / gamma_ln
}

#[inline]
fn pow_gamma(gamma_v: f64, y: f64) -> f64 {
    gamma_v.powf(y)
}

#[inline]
fn lower_bound(gamma_v: f64, bias: i32, k: i16) -> f64 {
    if k < 0 {
        return -lower_bound(gamma_v, bias, -k);
    }

    if k == MAX_KEY {
        return f64::INFINITY;
    }

    if k == 0 {
        return 0.0;
    }

    pow_gamma(gamma_v, f64::from(i32::from(k) - bias))
}

#[derive(Debug, Snafu)]
pub enum MergeError {
    #[snafu(display("cannot merge two sketches with mismatched configuration parameters"))]
    MismatchedConfigs,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Config {
    bin_limit: u16,
    // gamma_ln is the natural log of gamma_v, used to speed up calculating log base gamma.
    gamma_v: f64,
    gamma_ln: f64,
    // Min and max values representable by a sketch with these params.
    //
    // key(x) =
    //    0 : -min > x < min
    //    1 : x == min
    //   -1 : x == -min
    // +Inf : x > max
    // -Inf : x < -max.
    norm_min: f64,
    // Bias of the exponent, used to ensure key(x) >= 1.
    norm_bias: i32,
}

impl Config {
    #[allow(clippy::cast_possible_truncation)]
    pub(self) fn new(mut eps: f64, min_value: f64, bin_limit: u16) -> Self {
        assert!(eps > 0.0 && eps < 1.0, "eps must be between 0.0 and 1.0");
        assert!(min_value > 0.0, "min value must be greater than 0.0");
        assert!(bin_limit > 0, "bin limit must be greater than 0");

        eps *= 2.0;
        let gamma_v = 1.0 + eps;
        let gamma_ln = eps.ln_1p();

        // SAFETY: We expect `log_gamma` to return a value between -2^16 and 2^16, so it will always
        // fit in an i32.
        let norm_eff_min = log_gamma(gamma_ln, min_value).floor() as i32;
        let norm_bias = -norm_eff_min + 1;

        let norm_min = lower_bound(gamma_v, norm_bias, 1);

        assert!(
            norm_min <= min_value,
            "norm min should not exceed min_value"
        );

        Self {
            bin_limit,
            gamma_v,
            gamma_ln,
            norm_min,
            norm_bias,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn relative_accuracy(&self) -> f64 {
        // Only used for unit tests, hence the allow.
        (self.gamma_v - 1.0) / 2.0
    }

    /// Gets the value lower bound of the bin at the given key.
    pub fn bin_lower_bound(&self, k: i16) -> f64 {
        lower_bound(self.gamma_v, self.norm_bias, k)
    }

    /// Gets the key for the given value.
    ///
    /// The key corresponds to the bin where this value would be represented. The value returned here
    /// is such that: γ^k <= v < γ^(k+1).
    #[allow(clippy::cast_possible_truncation)]
    pub fn key(&self, v: f64) -> i16 {
        if v < 0.0 {
            return -self.key(-v);
        }

        if v == 0.0 || (v > 0.0 && v < self.norm_min) || (v < 0.0 && v > -self.norm_min) {
            return 0;
        }

        // SAFETY: `rounded` is intentionally meant to be a whole integer, and additionally, based
        // on our target gamma ln, we expect `log_gamma` to return a value between -2^16 and 2^16,
        // so it will always fit in an i32.
        let rounded = round_to_even(self.log_gamma(v)) as i32;
        let key = rounded.wrapping_add(self.norm_bias);

        // SAFETY: Our upper bound of POS_INF_KEY is i16, and our lower bound is simply one, so
        // there is no risk of truncation via conversion.
        key.clamp(1, i32::from(MAX_KEY)) as i16
    }

    pub fn log_gamma(&self, v: f64) -> f64 {
        log_gamma(self.gamma_ln, v)
    }
}

impl Default for Config {
    fn default() -> Self {
        Config::new(
            AGENT_DEFAULT_EPS,
            AGENT_DEFAULT_MIN_VALUE,
            AGENT_DEFAULT_BIN_LIMIT,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bin {
    k: i16,
    n: u16,
}

impl Bin {
    #[allow(clippy::cast_possible_truncation)]
    fn increment(&mut self, n: u32) -> u32 {
        let next = n + u32::from(self.n);
        if next > u32::from(MAX_BIN_WIDTH) {
            self.n = MAX_BIN_WIDTH;
            return next - u32::from(MAX_BIN_WIDTH);
        }

        // SAFETY: We already know `next` is less than or equal to `MAX_BIN_WIDTH` if we got here, and
        // `MAX_BIN_WIDTH` is u16, so next can't possibly be larger than a u16.
        self.n = next as u16;
        0
    }
}

/// An implementation of [`DDSketch`][ddsketch] that mirrors the implementation from the Datadog agent.
///
/// This implementation is subtly different from the open-source implementations of `DDSketch`, as
/// Datadog made some slight tweaks to configuration values and in-memory layout to optimize it for
/// insertion performance within the agent.
///
/// We've mimiced the agent version of `DDSketch` here in order to support a future where we can take
/// sketches shipped by the agent, handle them internally, merge them, and so on, without any loss
/// of accuracy, eventually forwarding them to Datadog ourselves.
///
/// As such, this implementation is constrained in the same ways: the configuration parameters
/// cannot be changed, the collapsing strategy is fixed, and we support a limited number of methods
/// for inserting into the sketch.
///
/// Importantly, we have a special function, again taken from the agent version, to allow us to
/// interpolate histograms, specifically our own aggregated histograms, into a sketch so that we can
/// emit useful default quantiles, rather than having to ship the buckets -- upper bound and count
/// -- to a downstream system that might have no native way to do the same thing, basically
/// providing no value as they have no way to render useful data from them.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AgentDDSketch {
    #[serde(skip)]
    config: Config,
    #[serde(with = "bin_serialization")]
    bins: Vec<Bin>,
    count: u32,
    min: f64,
    max: f64,
    sum: f64,
    avg: f64,
}

impl AgentDDSketch {
    /// Creates a new `AgentDDSketch` based on a configuration that is identical to the one used by
    /// the Datadog agent itself.
    pub fn with_agent_defaults() -> Self {
        let config = Config::default();
        let initial_bins = cmp::max(INITIAL_BINS, config.bin_limit) as usize;

        Self {
            config,
            bins: Vec::with_capacity(initial_bins),
            count: 0,
            min: f64::MAX,
            max: f64::MIN,
            sum: 0.0,
            avg: 0.0,
        }
    }

    /// Creates a new `AgentDDSketch` based on the given inputs.
    ///
    /// This is _only_ useful for constructing a sketch from the raw components when the sketch has
    /// passed through the transform boundary into Lua/VRL and needs to be reconstructed.
    ///
    /// This is a light code smell, as our intention is to rigorously mediate access and mutation of
    /// a sketch through `AgentDDSketch` and the provided methods.
    pub fn from_raw(
        count: u32,
        min: f64,
        max: f64,
        sum: f64,
        avg: f64,
        keys: &[i16],
        counts: &[u16],
    ) -> Option<AgentDDSketch> {
        let bin_map = BinMap {
            keys: keys.into(),
            counts: counts.into(),
        };
        bin_map.into_bins().map(|bins| Self {
            config: Config::default(),
            bins,
            count,
            min,
            max,
            sum,
            avg,
        })
    }

    #[allow(dead_code)]
    fn bin_count(&self) -> usize {
        self.bins.len()
    }

    pub fn bins(&self) -> &[Bin] {
        &self.bins
    }

    pub fn bin_map(&self) -> BinMap {
        BinMap::from_bins(&self.bins)
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Whether or not this sketch is empty.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Number of samples currently represented by this sketch.
    pub fn count(&self) -> u32 {
        self.count
    }

    /// Minimum value seen by this sketch.
    ///
    /// Returns `None` if the sketch is empty.
    pub fn min(&self) -> Option<f64> {
        if self.is_empty() {
            None
        } else {
            Some(self.min)
        }
    }

    /// Maximum value seen by this sketch.
    ///
    /// Returns `None` if the sketch is empty.
    pub fn max(&self) -> Option<f64> {
        if self.is_empty() {
            None
        } else {
            Some(self.max)
        }
    }

    /// Sum of all values seen by this sketch.
    ///
    /// Returns `None` if the sketch is empty.
    pub fn sum(&self) -> Option<f64> {
        if self.is_empty() {
            None
        } else {
            Some(self.sum)
        }
    }

    /// Average value seen by this sketch.
    ///
    /// Returns `None` if the sketch is empty.
    pub fn avg(&self) -> Option<f64> {
        if self.is_empty() {
            None
        } else {
            Some(self.avg)
        }
    }

    /// Clears the sketch, removing all bins and resetting all statistics.
    pub fn clear(&mut self) {
        self.count = 0;
        self.min = f64::MAX;
        self.max = f64::MIN;
        self.avg = 0.0;
        self.sum = 0.0;
        self.bins.clear();
    }

    fn adjust_basic_stats(&mut self, v: f64, n: u32) {
        if v < self.min {
            self.min = v;
        }

        if v > self.max {
            self.max = v;
        }

        self.count += n;
        self.sum += v * f64::from(n);

        if n == 1 {
            self.avg += (v - self.avg) / f64::from(self.count);
        } else {
            // TODO: From the Agent source code, this method apparently loses precision when the
            // two averages -- v and self.avg -- are close.  Is there a better approach?
            self.avg = self.avg + (v - self.avg) * f64::from(n) / f64::from(self.count);
        }
    }

    fn insert_key_counts(&mut self, mut counts: Vec<(i16, u32)>) {
        // Counts need to be sorted by key.
        counts.sort_unstable_by(|(k1, _), (k2, _)| k1.cmp(k2));

        let mut temp = Vec::new();

        let mut bins_idx = 0;
        let mut key_idx = 0;
        let bins_len = self.bins.len();
        let counts_len = counts.len();

        // PERF TODO: there's probably a fast path to be had where could check if all if the counts
        // have existing bins that aren't yet full, and we just update them directly, although we'd
        // still be doing a linear scan to find them since keys aren't 1:1 with their position in
        // `self.bins` but using this method just to update one or two bins is clearly suboptimal
        // and we wouldn't really want to scan them all just to have to back out and actually do the
        // non-fast path.. maybe a first pass could be checking if the first/last key falls within
        // our known min/max key, and if it doesn't, then we know we have to go through the non-fast
        // path, and if it passes, we do the scan to see if we can just update bins directly?
        while bins_idx < bins_len && key_idx < counts_len {
            let bin = self.bins[bins_idx];
            let vk = counts[key_idx].0;
            let kn = counts[key_idx].1;

            match bin.k.cmp(&vk) {
                Ordering::Greater => {
                    generate_bins(&mut temp, vk, kn);
                    key_idx += 1;
                }
                Ordering::Less => {
                    temp.push(bin);
                    bins_idx += 1;
                }
                Ordering::Equal => {
                    generate_bins(&mut temp, bin.k, u32::from(bin.n) + kn);
                    bins_idx += 1;
                    key_idx += 1;
                }
            }
        }

        temp.extend_from_slice(&self.bins[bins_idx..]);

        while key_idx < counts_len {
            let vk = counts[key_idx].0;
            let kn = counts[key_idx].1;
            generate_bins(&mut temp, vk, kn);
            key_idx += 1;
        }

        trim_left(&mut temp, self.config.bin_limit);

        // PERF TODO: This is where we might do a mem::swap instead so that we could shove the
        // bin vector into an object pool but I'm not sure this actually matters at the moment.
        self.bins = temp;
    }

    fn insert_keys(&mut self, mut keys: Vec<i16>) {
        // Updating more than 4 billion keys would be very very weird and likely indicative of
        // something horribly broken.
        assert!(
            keys.len()
                <= u32::MAX
                    .try_into()
                    .expect("we don't support 16-bit systems")
        );

        keys.sort_unstable();

        let mut temp = Vec::new();

        let mut bins_idx = 0;
        let mut key_idx = 0;
        let bins_len = self.bins.len();
        let keys_len = keys.len();

        // PERF TODO: there's probably a fast path to be had where could check if all if the counts
        // have existing bins that aren't yet full, and we just update them directly, although we'd
        // still be doing a linear scan to find them since keys aren't 1:1 with their position in
        // `self.bins` but using this method just to update one or two bins is clearly suboptimal
        // and we wouldn't really want to scan them all just to have to back out and actually do the
        // non-fast path.. maybe a first pass could be checking if the first/last key falls within
        // our known min/max key, and if it doesn't, then we know we have to go through the non-fast
        // path, and if it passes, we do the scan to see if we can just update bins directly?
        while bins_idx < bins_len && key_idx < keys_len {
            let bin = self.bins[bins_idx];
            let vk = keys[key_idx];

            match bin.k.cmp(&vk) {
                Ordering::Greater => {
                    let kn = buf_count_leading_equal(&keys, key_idx);
                    generate_bins(&mut temp, vk, kn);
                    key_idx += kn as usize;
                }
                Ordering::Less => {
                    temp.push(bin);
                    bins_idx += 1;
                }
                Ordering::Equal => {
                    let kn = buf_count_leading_equal(&keys, key_idx);
                    generate_bins(&mut temp, bin.k, u32::from(bin.n) + kn);
                    bins_idx += 1;
                    key_idx += kn as usize;
                }
            }
        }

        temp.extend_from_slice(&self.bins[bins_idx..]);

        while key_idx < keys_len {
            let vk = keys[key_idx];
            let kn = buf_count_leading_equal(&keys, key_idx);
            generate_bins(&mut temp, vk, kn);
            key_idx += kn as usize;
        }

        trim_left(&mut temp, self.config.bin_limit);

        // PERF TODO: This is where we might do a mem::swap instead so that we could shove the
        // bin vector into an object pool but I'm not sure this actually matters at the moment.
        self.bins = temp;
    }

    pub fn insert(&mut self, v: f64) {
        // TODO: this should return a result that makes sure we have enough room to actually add 1
        // more sample without hitting `self.config.max_count()`
        self.adjust_basic_stats(v, 1);

        let key = self.config.key(v);
        self.insert_keys(vec![key]);
    }

    pub fn insert_many(&mut self, vs: &[f64]) {
        // TODO: this should return a result that makes sure we have enough room to actually add 1
        // more sample without hitting `self.config.max_count()`
        let mut keys = Vec::with_capacity(vs.len());
        for v in vs {
            self.adjust_basic_stats(*v, 1);
            keys.push(self.config.key(*v));
        }
        self.insert_keys(keys);
    }

    pub fn insert_n(&mut self, v: f64, n: u32) {
        // TODO: this should return a result that makes sure we have enough room to actually add N
        // more samples without hitting `self.config.max_count()`
        self.adjust_basic_stats(v, n);

        let key = self.config.key(v);
        self.insert_key_counts(vec![(key, n)]);
    }

    fn insert_interpolate_bucket(&mut self, lower: f64, upper: f64, count: u32) {
        // Find the keys for the bins where the lower bound and upper bound would end up, and
        // collect all of the keys in between, inclusive.
        let lower_key = self.config.key(lower);
        let upper_key = self.config.key(upper);
        let keys = (lower_key..=upper_key).collect::<Vec<_>>();

        let mut key_counts = Vec::new();
        let mut remaining_count = count;
        let distance = upper - lower;
        let mut start_idx = 0;
        let mut end_idx = 1;
        let mut lower_bound = self.config.bin_lower_bound(keys[start_idx]);
        let mut remainder = 0.0;

        while end_idx < keys.len() && remaining_count > 0 {
            // For each key, map the total distance between the input lower/upper bound against the sketch
            // lower/upper bound for the current sketch bin, which tells us how much of the input
            // count to apply to the current sketch bin.
            let upper_bound = self.config.bin_lower_bound(keys[end_idx]);
            let fkn = ((upper_bound - lower_bound) / distance) * f64::from(count);
            if fkn > 1.0 {
                remainder += fkn - fkn.trunc();
            }

            // SAFETY: This integer cast is intentional: we want to get the non-fractional part, as
            // we've captured the fractional part in the above conditional.
            #[allow(clippy::cast_possible_truncation)]
            let mut kn = fkn as u32;
            if remainder > 1.0 {
                kn += 1;
                remainder -= 1.0;
            }

            if kn > 0 {
                if kn > remaining_count {
                    kn = remaining_count;
                }

                self.adjust_basic_stats(lower_bound, kn);
                key_counts.push((keys[start_idx], kn));

                remaining_count -= kn;
                start_idx = end_idx;
                lower_bound = upper_bound;
            }

            end_idx += 1;
        }

        if remaining_count > 0 {
            let last_key = keys[start_idx];
            lower_bound = self.config.bin_lower_bound(last_key);
            self.adjust_basic_stats(lower_bound, remaining_count);
            key_counts.push((last_key, remaining_count));
        }

        self.insert_key_counts(key_counts);
    }

    pub fn insert_interpolate_buckets(&mut self, mut buckets: Vec<Bucket>) {
        // Buckets need to be sorted from lowest to highest so that we can properly calculate the
        // rolling lower/upper bounds.
        buckets.sort_by(|a, b| {
            let oa = OrderedFloat(a.upper_limit);
            let ob = OrderedFloat(b.upper_limit);

            oa.cmp(&ob)
        });

        let mut lower = f64::NEG_INFINITY;

        for bucket in buckets {
            let mut upper = bucket.upper_limit;
            if upper.is_sign_positive() && upper.is_infinite() {
                upper = lower;
            } else if lower.is_sign_negative() && lower.is_infinite() {
                lower = upper;
            }

            // Each bucket should only have the values that fit within that bucket, which is
            // generally enforced at the source level by converting from cumulative buckets, or
            // enforced by the internal structures that hold bucketed data i.e. Vector's internal
            // `Histogram` data structure used for collecting histograms from `metrics`.
            self.insert_interpolate_bucket(lower, upper, bucket.count);
            lower = bucket.upper_limit;
        }
    }

    /// Adds a bin directly into the sketch.
    ///
    /// Used only for unit testing so that we can create a sketch with an exact layout, which allows
    /// testing around the resulting bins when feeding in specific values, as well as generating
    /// explicitly bad layouts for testing.
    #[allow(dead_code)]
    pub(crate) fn insert_raw_bin(&mut self, k: i16, n: u16) {
        let v = self.config.bin_lower_bound(k);
        self.adjust_basic_stats(v, u32::from(n));
        self.bins.push(Bin { k, n });
    }

    pub fn quantile(&self, q: f64) -> Option<f64> {
        if self.count == 0 {
            return None;
        }

        if q <= 0.0 {
            return Some(self.min);
        }

        if q >= 1.0 {
            return Some(self.max);
        }

        let mut n = 0.0;
        let mut estimated = None;
        let wanted_rank = rank(self.count, q);

        for (i, bin) in self.bins.iter().enumerate() {
            n += f64::from(bin.n);
            if n <= wanted_rank {
                continue;
            }

            let weight = (n - wanted_rank) / f64::from(bin.n);
            let mut v_low = self.config.bin_lower_bound(bin.k);
            let mut v_high = v_low * self.config.gamma_v;

            if i == self.bins.len() {
                v_high = self.max;
            } else if i == 0 {
                v_low = self.min;
            }

            estimated = Some(v_low * weight + v_high * (1.0 - weight));
            break;
        }

        estimated
            .map(|v| v.clamp(self.min, self.max))
            .or(Some(f64::NAN))
    }

    /// Merges another sketch into this sketch, without a loss of accuracy.
    ///
    /// All samples present in the other sketch will be correctly represented in this sketch, and
    /// summary statistics such as the sum, average, count, min, and max, will represent the sum of
    /// samples from both sketches.
    ///
    /// ## Errors
    ///
    /// If there is an error while merging the two sketches together, an error variant will be
    /// returned that describes the issue.
    pub fn merge(&mut self, other: &AgentDDSketch) -> Result<(), MergeError> {
        if self.config != other.config {
            return Err(MergeError::MismatchedConfigs);
        }

        // Merge the basic statistics together.
        self.count += other.count;
        if other.max > self.max {
            self.max = other.max;
        }
        if other.min < self.min {
            self.min = other.min;
        }
        self.sum += other.sum;
        self.avg =
            self.avg + (other.avg - self.avg) * f64::from(other.count) / f64::from(self.count);

        // Now merge the bins.
        let mut temp = Vec::new();

        let mut bins_idx = 0;
        for other_bin in &other.bins {
            let start = bins_idx;
            while bins_idx < self.bins.len() && self.bins[bins_idx].k < other_bin.k {
                bins_idx += 1;
            }

            temp.extend_from_slice(&self.bins[start..bins_idx]);

            if bins_idx >= self.bins.len() || self.bins[bins_idx].k > other_bin.k {
                temp.push(*other_bin);
            } else if self.bins[bins_idx].k == other_bin.k {
                generate_bins(
                    &mut temp,
                    other_bin.k,
                    u32::from(other_bin.n) + u32::from(self.bins[bins_idx].n),
                );
                bins_idx += 1;
            }
        }

        temp.extend_from_slice(&self.bins[bins_idx..]);
        trim_left(&mut temp, self.config.bin_limit);

        self.bins = temp;

        Ok(())
    }

    /// Converts a `Metric` to a sketch representation, if possible, using `AgentDDSketch`.
    ///
    /// For certain types of metric values, such as distributions or aggregated histograms, we can
    /// easily convert them to a sketch-based representation.  Rather than push the logic of how to
    /// do that up to callers that wish to use a sketch-based representation, we bundle it here as a
    /// free function on `AgentDDSketch` itself.
    ///
    /// If the metric value cannot be represented as a sketch -- essentially, everything that isn't
    /// a distribution or aggregated histogram -- then the metric is passed back unmodified.  All
    /// existing metadata -- series name, tags, timestamp, etc -- is left unmodified, even if the
    /// metric is converted to a sketch internally.
    pub fn transform_to_sketch(mut metric: Metric) -> Metric {
        let sketch = match metric.data_mut().value_mut() {
            MetricValue::Distribution { samples, .. } => {
                let mut sketch = AgentDDSketch::with_agent_defaults();
                for sample in samples {
                    sketch.insert_n(sample.value, sample.rate);
                }
                Some(sketch)
            }
            MetricValue::AggregatedHistogram { buckets, .. } => {
                let delta_buckets = mem::take(buckets);
                let mut sketch = AgentDDSketch::with_agent_defaults();
                sketch.insert_interpolate_buckets(delta_buckets);
                Some(sketch)
            }
            // We can't convert from any other metric value.
            _ => None,
        };

        match sketch {
            // Metric was not able to be converted to a sketch, so pass it back.
            None => metric,
            // Metric was able to be converted to a sketch, so adjust the value.
            Some(sketch) => metric.with_value(sketch.into()),
        }
    }
}

impl PartialEq for AgentDDSketch {
    fn eq(&self, other: &Self) -> bool {
        // We skip checking the configuration because we don't allow creating configurations by
        // hand, and it's always locked to the constants used by the Datadog Agent.  We only check
        // the configuration equality manually in `AgentDDSketch::merge`, to protect ourselves in
        // the future if different configurations become allowed.
        //
        // Additionally, we also use floating-point-specific relative comparisons for sum/avg
        // because they can be minimally different between sketches purely due to floating-point
        // behavior, despite being fed the same exact data in terms of recorded samples.
        self.count == other.count
            && self.min == other.min
            && self.max == other.max
            && self.sum.eq_ulps(&other.sum, &1)
            && self.avg.eq_ulps(&other.avg, &1)
            && self.bins == other.bins
    }
}

impl Eq for AgentDDSketch {}

impl ByteSizeOf for AgentDDSketch {
    fn allocated_bytes(&self) -> usize {
        self.bins.len() * mem::size_of::<Bin>()
    }
}

#[derive(Serialize, Deserialize)]
pub struct BinMap {
    #[serde(rename = "k")]
    pub keys: Vec<i16>,
    #[serde(rename = "n")]
    pub counts: Vec<u16>,
}

impl BinMap {
    pub fn from_bins<B>(bins: B) -> BinMap
    where
        B: AsRef<[Bin]>,
    {
        let (keys, counts) =
            bins.as_ref()
                .iter()
                .fold((Vec::new(), Vec::new()), |(mut keys, mut counts), bin| {
                    keys.push(bin.k);
                    counts.push(bin.n);

                    (keys, counts)
                });

        BinMap { keys, counts }
    }

    pub fn into_parts(self) -> (Vec<i16>, Vec<u16>) {
        (self.keys, self.counts)
    }

    pub fn into_bins(self) -> Option<Vec<Bin>> {
        if self.keys.len() == self.counts.len() {
            Some(
                self.keys
                    .into_iter()
                    .zip(self.counts.into_iter())
                    .map(|(k, n)| Bin { k, n })
                    .collect(),
            )
        } else {
            None
        }
    }
}

pub(self) mod bin_serialization {
    use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};

    use super::{Bin, BinMap};

    pub fn serialize<S>(bins: &[Bin], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // We use a custom serializer here because while the summary stat fields are (de)serialized
        // fine using the default derive implementation, we have to split the bins into an array of
        // keys and an array of counts.  This is to keep serializing as close as possible to the
        // Protocol Buffers definition that the Datadog Agent uses.
        let bin_map = BinMap::from_bins(bins);
        bin_map.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Bin>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bin_map: BinMap = Deserialize::deserialize(deserializer)?;
        bin_map
            .into_bins()
            .ok_or("keys and counts must match in length")
            .map_err(D::Error::custom)
    }
}

fn rank(count: u32, q: f64) -> f64 {
    round_to_even(q * f64::from(count - 1))
}

#[allow(clippy::cast_possible_truncation)]
fn buf_count_leading_equal(keys: &[i16], start_idx: usize) -> u32 {
    if start_idx == keys.len() - 1 {
        return 1;
    }

    let mut idx = start_idx;
    while idx < keys.len() && keys[idx] == keys[start_idx] {
        idx += 1;
    }

    // SAFETY: We limit the size of the vector (used to provide the slice given to us here) to be no
    // larger than 2^32, so we can't exceed u32 here.
    (idx - start_idx) as u32
}

fn trim_left(bins: &mut Vec<Bin>, bin_limit: u16) {
    // We won't ever support Vector running on anything other than a 32-bit platform and above, I
    // imagine, so this should always be safe.
    let bin_limit = bin_limit as usize;
    if bin_limit == 0 || bins.len() < bin_limit {
        return;
    }

    let num_to_remove = bins.len() - bin_limit;
    let mut missing = 0;
    let mut overflow = Vec::new();

    for bin in bins.iter().take(num_to_remove) {
        missing += u32::from(bin.n);

        if missing > u32::from(MAX_BIN_WIDTH) {
            overflow.push(Bin {
                k: bin.k,
                n: MAX_BIN_WIDTH,
            });

            missing -= u32::from(MAX_BIN_WIDTH);
        }
    }

    let bin_remove = &mut bins[num_to_remove];
    missing = bin_remove.increment(missing);
    if missing > 0 {
        generate_bins(&mut overflow, bin_remove.k, missing);
    }

    let overflow_len = overflow.len();
    let (_, bins_end) = bins.split_at(num_to_remove);
    overflow.extend_from_slice(bins_end);

    // I still don't yet understand how this works, since you'd think bin limit should be the
    // overall limit of the number of bins, but we're allowing more than that.. :thinkies:
    overflow.truncate(bin_limit + overflow_len);

    mem::swap(bins, &mut overflow);
}

#[allow(clippy::cast_possible_truncation)]
fn generate_bins(bins: &mut Vec<Bin>, k: i16, n: u32) {
    if n < u32::from(MAX_BIN_WIDTH) {
        // SAFETY: Cannot truncate `n`, as it's less than a u16 value.
        bins.push(Bin { k, n: n as u16 });
    } else {
        let overflow = n % u32::from(MAX_BIN_WIDTH);
        if overflow != 0 {
            bins.push(Bin {
                k,
                // SAFETY: Cannot truncate `overflow`, as it's modulo'd by a u16 value.
                n: overflow as u16,
            });
        }

        for _ in 0..(n / u32::from(MAX_BIN_WIDTH)) {
            bins.push(Bin {
                k,
                n: MAX_BIN_WIDTH,
            });
        }
    }
}

#[allow(clippy::cast_possible_truncation)]
#[inline]
fn capped_u64_shift(shift: u64) -> u32 {
    if shift >= 64 {
        u32::MAX
    } else {
        // SAFETY: There's no way that we end up truncating `shift`, since we cap it to 64 above.
        shift as u32
    }
}

fn round_to_even(v: f64) -> f64 {
    // Taken from Go: src/math/floor.go
    //
    // Copyright (c) 2009 The Go Authors. All rights reserved.
    //
    // Redistribution and use in source and binary forms, with or without
    // modification, are permitted provided that the following conditions are
    // met:
    //
    //    * Redistributions of source code must retain the above copyright
    // notice, this list of conditions and the following disclaimer.
    //    * Redistributions in binary form must reproduce the above
    // copyright notice, this list of conditions and the following disclaimer
    // in the documentation and/or other materials provided with the
    // distribution.
    //    * Neither the name of Google Inc. nor the names of its
    // contributors may be used to endorse or promote products derived from
    // this software without specific prior written permission.
    //
    // THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
    // "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
    // LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
    // A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
    // OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
    // SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
    // LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
    // DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
    // THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
    // (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
    // OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

    // HEAR YE: There's a non-zero chance that we could rewrite this function in a way that is far
    // more Rust like, rather than porting over the particulars of how Go works, but we're
    // aiming for compatibility with a Go implementation, and so we've ported this over as
    // faithfully as possible.  With that said, here are the specifics we're dealing with:
    // - in Go, subtraction of unsigned numbers implicitly wraps around i.e. 1u64 - u64::MAX == 2
    // - in Go, right shifts are capped at the bitwidth of the left operand, which means that if
    //   you shift a 64-bit unsigned integers by 65 or above (some_u64 >> 65), instead of being told that
    //   you're doing something wrong, Go just caps the shift amount, and you end up with 0,
    //   whether you shift by 64 or by 9 million
    // - in Rust, it's not possible to directly do `some_u64 >> 64` even, as this would be
    //   considered an overflow
    // - in Rust, there are methods on unsigned integer primitives that allow for safely
    //   shifting by amounts greater than the bitwidth of the primitive type, although they mask off
    //   the bits in the shift amount that are higher than the bitwidth i.e. shift values above 64,
    //   for shifting a u64, are masked such that the resulting shift amount is 0, effectively
    //   not shifting the operand at all
    //
    // With all of this in mind, the code below compensates for that by doing wrapped
    // subtraction and shifting, which is straightforward, but also by utilizing
    // `capped_u64_shift` to approximate the behavior Go has when shifting by amounts larger
    // than the bitwidth of the left operand.
    //
    // I'm really proud of myself for reverse engineering this, but I sincerely hope we can
    // flush it down the toilet in the near future for something vastly simpler.
    const MASK: u64 = 0x7ff;
    const BIAS: u64 = 1023;
    const SHIFT: u64 = 64 - 11 - 1;
    const SIGN_MASK: u64 = 1 << 63;
    const FRAC_MASK: u64 = (1 << SHIFT) - 1;
    #[allow(clippy::unreadable_literal)]
    const UV_ONE: u64 = 0x3FF0000000000000;
    const HALF_MINUS_ULP: u64 = (1 << (SHIFT - 1)) - 1;

    let mut bits = v.to_bits();
    let mut e = (bits >> SHIFT) & MASK;
    if e >= BIAS {
        e = e.wrapping_sub(BIAS);
        let shift_amount = SHIFT.wrapping_sub(e);
        let shifted = bits.wrapping_shr(capped_u64_shift(shift_amount));
        let plus_ulp = HALF_MINUS_ULP + (shifted & 1);
        bits += plus_ulp.wrapping_shr(capped_u64_shift(e));
        bits &= !(FRAC_MASK.wrapping_shr(capped_u64_shift(e)));
    } else if e == BIAS - 1 && bits & FRAC_MASK != 0 {
        // Round 0.5 < abs(x) < 1.
        bits = bits & SIGN_MASK | UV_ONE; // +-1
    } else {
        // Round abs(x) <= 0.5 including denormals.
        bits &= SIGN_MASK; // +-0
    }
    f64::from_bits(bits)
}

#[cfg(test)]
mod tests {
    use super::{round_to_even, AgentDDSketch, Config, AGENT_DEFAULT_EPS, MAX_KEY};

    const FLOATING_POINT_ACCEPTABLE_ERROR: f64 = 1.0e-10;

    #[cfg(ddsketch_extended)]
    fn generate_pareto_distribution() -> Vec<OrderedFloat<f64>> {
        use ordered_float::OrderedFloat;
        use rand::thread_rng;
        use rand_distr::{Distribution, Pareto};

        // Generate a set of samples that roughly correspond to the latency of a typical web
        // service, in microseconds, with a gamma distribution: big hump at the beginning with a
        // long tail.  We limit this so the samples represent latencies that bottom out at 15
        // milliseconds and tail off all the way up to 10 seconds.
        //let distribution = Gamma::new(1.2, 100.0).unwrap();
        let distribution = Pareto::new(1.0, 1.0).expect("pareto distribution should be valid");
        let mut samples = distribution
            .sample_iter(thread_rng())
            // Scale by 10,000 to get microseconds.
            .map(|n| n * 10_000.0)
            .filter(|n| *n > 15_000.0 && *n < 10_000_000.0)
            .map(OrderedFloat)
            .take(1000)
            .collect::<Vec<_>>();

        // Sort smallest to largest.
        samples.sort();

        samples
    }

    #[test]
    fn test_ddsketch_config_key_lower_bound_identity() {
        let config = Config::default();
        for k in (-MAX_KEY + 1)..MAX_KEY {
            assert_eq!(k, config.key(config.bin_lower_bound(k)));
        }
    }

    #[test]
    fn test_ddsketch_basic() {
        let mut sketch = AgentDDSketch::with_agent_defaults();
        assert!(sketch.is_empty());
        assert_eq!(sketch.count(), 0);
        assert_eq!(sketch.min(), None);
        assert_eq!(sketch.max(), None);
        assert_eq!(sketch.sum(), None);
        assert_eq!(sketch.avg(), None);

        sketch.insert(3.15);
        assert!(!sketch.is_empty());
        assert_eq!(sketch.count(), 1);
        assert_eq!(sketch.min(), Some(3.15));
        assert_eq!(sketch.max(), Some(3.15));
        assert_eq!(sketch.sum(), Some(3.15));
        assert_eq!(sketch.avg(), Some(3.15));

        sketch.insert(2.28);
        assert!(!sketch.is_empty());
        assert_eq!(sketch.count(), 2);
        assert_eq!(sketch.min(), Some(2.28));
        assert_eq!(sketch.max(), Some(3.15));
        assert_eq!(sketch.sum(), Some(5.43));
        assert_eq!(sketch.avg(), Some(2.715));
    }

    #[test]
    fn test_ddsketch_clear() {
        let sketch1 = AgentDDSketch::with_agent_defaults();
        let mut sketch2 = AgentDDSketch::with_agent_defaults();

        assert_eq!(sketch1, sketch2);
        assert!(sketch1.is_empty());
        assert!(sketch2.is_empty());

        sketch2.insert(3.15);
        assert_ne!(sketch1, sketch2);
        assert!(!sketch2.is_empty());

        sketch2.clear();
        assert_eq!(sketch1, sketch2);
        assert!(sketch2.is_empty());
    }

    #[test]
    fn test_ddsketch_neg_to_pos() {
        // This gives us 10k values because otherwise this test runs really slow in debug mode.
        let start = -1.0;
        let end = 1.0;
        let delta = 0.0002;

        let mut sketch = AgentDDSketch::with_agent_defaults();

        let mut v = start;
        while v <= end {
            sketch.insert(v);

            v += delta;
        }

        let min = sketch.quantile(0.0).expect("should have value");
        let median = sketch.quantile(0.5).expect("should have value");
        let max = sketch.quantile(1.0).expect("should have value");

        assert_eq!(start, min);
        assert!(median.abs() < FLOATING_POINT_ACCEPTABLE_ERROR);
        assert!((end - max).abs() < FLOATING_POINT_ACCEPTABLE_ERROR);
    }

    #[test]
    fn test_merge() {
        let mut all_values = AgentDDSketch::with_agent_defaults();
        let mut odd_values = AgentDDSketch::with_agent_defaults();
        let mut even_values = AgentDDSketch::with_agent_defaults();
        let mut all_values_many = AgentDDSketch::with_agent_defaults();

        let mut values = Vec::new();
        for i in -50..=50 {
            let v = f64::from(i);

            all_values.insert(v);

            if i & 1 == 0 {
                odd_values.insert(v);
            } else {
                even_values.insert(v);
            }

            values.push(v);
        }

        all_values_many.insert_many(&values);

        assert!(odd_values.merge(&even_values).is_ok());
        let merged_values = odd_values;

        // Number of bins should be equal to the number of values we inserted.
        assert_eq!(all_values.bin_count(), values.len());

        // Values at both ends of the quantile range should be equal.
        let low_end = all_values
            .quantile(0.01)
            .expect("should have estimated value");
        let high_end = all_values
            .quantile(0.99)
            .expect("should have estimated value");
        assert_eq!(high_end, -low_end);

        let target_bin_count = all_values.bin_count();
        for sketch in &[all_values, all_values_many, merged_values] {
            assert_eq!(sketch.quantile(0.5), Some(0.0));
            assert_eq!(sketch.quantile(0.0), Some(-50.0));
            assert_eq!(sketch.quantile(1.0), Some(50.0));

            for p in 0..50 {
                let q = f64::from(p) / 100.0;
                let positive = sketch
                    .quantile(q + 0.5)
                    .expect("should have estimated value");
                let negative = -sketch
                    .quantile(0.5 - q)
                    .expect("should have estimated value");

                assert!(
                    (positive - negative).abs() <= 1.0e-6,
                    "positive vs negative difference too great ({} vs {})",
                    positive,
                    negative
                );
            }

            assert_eq!(target_bin_count, sketch.bin_count());
        }
    }

    #[test]
    fn test_merge_different_configs() {
        let mut first = AgentDDSketch::with_agent_defaults();
        let mut second = AgentDDSketch::with_agent_defaults();

        // Subtly tweak the config of the second sketch to ensure that merging fails.
        second.config.norm_bias += 1;

        assert!(first.merge(&second).is_err());
    }

    #[test]
    #[cfg(ddsketch_extended)]
    fn test_ddsketch_pareto_distribution() {
        use ndarray::{Array, Axis};
        use ndarray_stats::{interpolate::Midpoint, QuantileExt};
        use noisy_float::prelude::N64;

        // NOTE: This test unexpectedly fails to meet the relative accuracy guarantees when checking
        // the samples against quantiles pulled via `ndarray_stats`.  When feeding the same samples
        // to the actual DDSketch implementation in datadog-agent, we get identical results at each
        // quantile. This doesn't make a huge amount of sense to me, since we have a unit test that
        // verifies the relative accuracy of the configuration itself, which should only fail to be
        // met if we hit the bin limit and bins have to be collapsed.
        //
        // We're keeping it here as a reminder of the seemingly practical difference in accuracy
        // vs deriving the quantiles of the sample sets directly.

        // We generate a straightforward Pareto distribution to simulate web request latencies.
        let samples = generate_pareto_distribution();

        // Prepare our data for querying.
        let mut sketch = AgentDDSketch::with_agent_defaults();

        let relative_accuracy = AGENT_DEFAULT_EPS;
        for sample in &samples {
            sketch.insert(sample.into_inner());
        }

        let mut array = Array::from_iter(samples);

        // Now check the estimated quantile via `AgentDDSketch` vs the true quantile via `ndarray`.
        //
        // TODO: what's a reasonable quantile to start from? from testing the actual agent code, it
        // seems like <p50 is gonna be rough no matter what, which I think is expected but also not great?
        for p in 1..=100 {
            let q = p as f64 / 100.0;
            let x = sketch.quantile(q);
            assert!(x.is_some());

            let estimated = x.unwrap();
            let actual = array
                .quantile_axis_mut(Axis(0), N64::unchecked_new(q), &Midpoint)
                .expect("quantile should be in range")
                .get(())
                .expect("quantile value should be present")
                .clone()
                .into_inner();

            let _err = (estimated - actual).abs() / actual;
            assert!(err <= relative_accuracy,
				"relative accuracy out of bounds: q={}, estimate={}, actual={}, target-rel-acc={}, actual-rel-acc={}, bin-count={}",
				q, estimated, actual, relative_accuracy, err, sketch.bin_count());
        }
    }

    #[test]
    fn test_relative_accuracy_fast() {
        // These values are based on the agent's unit tests for asserting relative accuracy of the
        // DDSketch implementation.  Notably, it does not seem to test the full extent of values
        // that the open-source implementations do, but then again... all we care about is parity
        // with the agent version so we can pass them through.
        //
        // Another noteworthy thing: it seems that they don't test from the actual targeted minimum
        // value, which is 1.0e-9, which would give nanosecond granularity vs just microsecond
        // granularity.
        let config = Config::default();
        let min_value = 1.0;
        // We don't care about precision loss, just consistency.
        #[allow(clippy::cast_possible_truncation)]
        let max_value = config.gamma_v.powf(5.0) as f32;

        test_relative_accuracy(config, AGENT_DEFAULT_EPS, min_value, max_value);
    }

    #[test]
    #[cfg(ddsketch_extended)]
    fn test_relative_accuracy_slow() {
        // These values are based on the agent's unit tests for asserting relative accuracy of the
        // DDSketch implementation.  Notably, it does not seem to test the full extent of values
        // that the open-source implementations do, but then again... all we care about is parity
        // with the agent version so we can pass them through.
        //
        // Another noteworthy thing: it seems that they don't test from the actual targeted minimum
        // value, which is 1.0e-9, which would give nanosecond granularity vs just microsecond
        // granularity.
        //
        // This test uses a far larger range of values, and takes 60-70 seconds, hence why we've
        // guared it here behind a cfg flag.
        let config = Config::default();
        let min_value = 1.0e-6;
        let max_value = i64::MAX as f32;

        test_relative_accuracy(config, AGENT_DEFAULT_EPS, min_value, max_value)
    }

    fn parse_sketch_from_string_bins(layout: &str) -> AgentDDSketch {
        layout
            .split(' ')
            .map(|pair| pair.split(':').map(ToOwned::to_owned).collect::<Vec<_>>())
            .fold(
                AgentDDSketch::with_agent_defaults(),
                |mut sketch, mut kn| {
                    let k = kn.remove(0).parse::<i16>().unwrap();
                    let n = kn.remove(0).parse::<u16>().unwrap();

                    sketch.insert_raw_bin(k, n);
                    sketch
                },
            )
    }

    fn compare_sketches(actual: &AgentDDSketch, expected: &AgentDDSketch, allowed_err: f64) {
        let actual_sum = actual.sum().unwrap();
        let expected_sum = expected.sum().unwrap();
        let actual_avg = actual.avg().unwrap();
        let expected_avg = expected.avg().unwrap();
        let sum_delta = (actual_sum - expected_sum).abs();
        let avg_delta = (actual_avg - expected_avg).abs();
        assert!(sum_delta <= allowed_err);
        assert!(avg_delta <= allowed_err);
        assert_eq!(actual.min(), expected.min());
        assert_eq!(actual.max(), expected.max());
        assert_eq!(actual.count(), expected.count());
        assert_eq!(actual.bins(), expected.bins());
    }

    #[test]
    fn test_histogram_interpolation_agent_similarity() {
        #[derive(Clone)]
        struct Case {
            lower: f64,
            upper: f64,
            count: u32,
            allowed_err: f64,
            expected: &'static str,
        }

        let check_result = |actual: &AgentDDSketch, case: &Case| {
            let expected = parse_sketch_from_string_bins(case.expected);

            assert_eq!(expected.count(), case.count);
            assert_eq!(actual.count(), case.count);
            assert_eq!(actual.bins(), expected.bins());
            compare_sketches(actual, &expected, case.allowed_err);

            let actual_count: u32 = actual.bins.iter().map(|b| u32::from(b.n)).sum();
            assert_eq!(actual_count, case.count);
        };

        let cases = &[
            Case { lower: 0.0, upper: 10.0, count: 2, allowed_err: 0.0, expected: "0:1 1442:1" },
            Case { lower: 10.0, upper: 20.0, count: 4,  allowed_err: 0.0, expected: "1487:1 1502:1 1514:1 1524:1" },
		    Case { lower: -10.0, upper: 10.0, count: 4, allowed_err: 0.0, expected: "-1487:1 -1442:1 -1067:1 1442:1"},
            Case { lower: 0.0, upper: 10.0, count: 100, allowed_err: 0.0, expected: "0:1 1190:1 1235:1 1261:1 1280:1 1295:1 1307:1 1317:1 1326:1 1334:1 1341:1 1347:1 1353:1 1358:1 1363:1 1368:1 1372:2 1376:1 1380:1 1384:1 1388:1 1391:1 1394:1 1397:2 1400:1 1403:1 1406:2 1409:1 1412:1 1415:2 1417:1 1419:1 1421:1 1423:1 1425:1 1427:1 1429:2 1431:1 1433:1 1435:2 1437:1 1439:2 1441:1 1443:2 1445:2 1447:1 1449:2 1451:2 1453:2 1455:2 1457:2 1459:1 1460:1 1461:1 1462:1 1463:1 1464:1 1465:1 1466:1 1467:2 1468:1 1469:1 1470:1 1471:1 1472:2 1473:1 1474:1 1475:1 1476:2 1477:1 1478:2 1479:1 1480:1 1481:2 1482:1 1483:2 1484:1 1485:2 1486:1" },
		    Case { lower: 1_000.0, upper: 100_000.0, count: 1_000_000 - 1, allowed_err: 0.0, expected: "1784:158 1785:162 1786:164 1787:166 1788:170 1789:171 1790:175 1791:177 1792:180 1793:183 1794:185 1795:189 1796:191 1797:195 1798:197 1799:201 1800:203 1801:207 1802:210 1803:214 1804:217 1805:220 1806:223 1807:227 1808:231 1809:234 1810:238 1811:242 1812:245 1813:249 1814:253 1815:257 1816:261 1817:265 1818:270 1819:273 1820:278 1821:282 1822:287 1823:291 1824:295 1825:300 1826:305 1827:310 1828:314 1829:320 1830:324 1831:329 1832:335 1833:340 1834:345 1835:350 1836:356 1837:362 1838:367 1839:373 1840:379 1841:384 1842:391 1843:397 1844:403 1845:409 1846:416 1847:422 1848:429 1849:435 1850:442 1851:449 1852:457 1853:463 1854:470 1855:478 1856:486 1857:493 1858:500 1859:509 1860:516 1861:525 1862:532 1863:541 1864:550 1865:558 1866:567 1867:575 1868:585 1869:594 1870:603 1871:612 1872:622 1873:632 1874:642 1875:651 1876:662 1877:672 1878:683 1879:693 1880:704 1881:716 1882:726 1883:738 1884:749 1885:761 1886:773 1887:785 1888:797 1889:809 1890:823 1891:835 1892:848 1893:861 1894:875 1895:889 1896:902 1897:917 1898:931 1899:945 1900:960 1901:975 1902:991 1903:1006 1904:1021 1905:1038 1906:1053 1907:1071 1908:1087 1909:1104 1910:1121 1911:1138 1912:1157 1913:1175 1914:1192 1915:1212 1916:1231 1917:1249 1918:1269 1919:1290 1920:1309 1921:1329 1922:1351 1923:1371 1924:1393 1925:1415 1926:1437 1927:1459 1928:1482 1929:1506 1930:1529 1931:1552 1932:1577 1933:1602 1934:1626 1935:1652 1936:1678 1937:1704 1938:1731 1939:1758 1940:1785 1941:1813 1942:1841 1943:1870 1944:1900 1945:1929 1946:1959 1947:1990 1948:2021 1949:2052 1950:2085 1951:2117 1952:2150 1953:2184 1954:2218 1955:2253 1956:2287 1957:2324 1958:2360 1959:2396 1960:2435 1961:2472 1962:2511 1963:2550 1964:2589 1965:2631 1966:2671 1967:2714 1968:2755 1969:2799 1970:2842 1971:2887 1972:2932 1973:2978 1974:3024 1975:3071 1976:3120 1977:3168 1978:3218 1979:3268 1980:3319 1981:3371 1982:3423 1983:3477 1984:3532 1985:3586 1986:3643 1987:3700 1988:3757 1989:3816 1990:3876 1991:3936 1992:3998 1993:4060 1994:4124 1995:4188 1996:4253 1997:4320 1998:4388 1999:4456 2000:4526 2001:4596 2002:4668 2003:4741 2004:4816 2005:4890 2006:4967 2007:5044 2008:5124 2009:5203 2010:5285 2011:5367 2012:5451 2013:5536 2014:5623 2015:5711 2016:5800 2017:5890 2018:5983 2019:6076 2020:6171 2021:6267 2022:6365 2023:6465 2024:6566 2025:6668 2026:6773 2027:6878 2028:6986 2029:7095 2030:7206 2031:7318 2032:7433 2033:7549 2034:7667 2035:7786 2036:7909 2037:8032 2038:8157 2039:8285 2040:8414 2041:8546 2042:8679 2043:8815 2044:8953 2045:9092 2046:9235 2047:9379 2048:9525 2049:9675 2050:9825 2051:9979 2052:10135 2053:10293 2054:10454 2055:10618 2056:10783 2057:10952 2058:11123 2059:11297 2060:11473 2061:11653 2062:11834 2063:12020 2064:12207 2065:12398 2066:12592 2067:12788 2068:12989 2069:13191 2070:13397 2071:13607 2072:13819 2073:14036 2074:14254 2075:14478 2076:14703 2077:14933 2078:15167 2079:15403 2080:8942" },
		    Case { lower: 1_000.0, upper: 10_000.0, count: 10_000_000 - 1, allowed_err: 0.00001, expected: "1784:17485 1785:17758 1786:18035 1787:18318 1788:18604 1789:18894 1790:19190 1791:19489 1792:19794 1793:20103 1794:20418 1795:20736 1796:21061 1797:21389 1798:21724 1799:22063 1800:22408 1801:22758 1802:23113 1803:23475 1804:23841 1805:24215 1806:24592 1807:24977 1808:25366 1809:25764 1810:26165 1811:26575 1812:26990 1813:27412 1814:27839 1815:28275 1816:28717 1817:29165 1818:29622 1819:30083 1820:30554 1821:31032 1822:31516 1823:32009 1824:32509 1825:33016 1826:33533 1827:34057 1828:34589 1829:35129 1830:35678 1831:36235 1832:36802 1833:37377 1834:37961 1835:38554 1836:39156 1837:39768 1838:40390 1839:41020 1840:41662 1841:42312 1842:42974 1843:43645 1844:44327 1845:45020 1846:45723 1847:46438 1848:47163 1849:47900 1850:48648 1851:49409 1852:50181 1853:50964 1854:51761 1855:52570 1856:53391 1857:54226 1858:55072 1859:55934 1860:56807 1861:57695 1862:58596 1863:59512 1864:60441 1865:61387 1866:62345 1867:63319 1868:64309 1869:65314 1870:799 1870:65535 1871:1835 1871:65535 1872:2889 1872:65535 1873:3957 1873:65535 1874:5043 1874:65535 1875:6146 1875:65535 1876:7266 1876:65535 1877:8404 1877:65535 1878:9559 1878:65535 1879:10732 1879:65535 1880:11923 1880:65535 1881:13135 1881:65535 1882:14363 1882:65535 1883:15612 1883:65535 1884:16879 1884:65535 1885:18168 1885:65535 1886:19475 1886:65535 1887:20803 1887:65535 1888:22153 1888:65535 1889:23523 1889:65535 1890:24914 1890:65535 1891:26327 1891:65535 1892:27763 1892:65535 1893:29221 1893:65535 1894:30701 1894:65535 1895:32205 1895:65535 1896:33732 1896:65535 1897:35283 1897:65535 1898:36858 1898:65535 1899:38458 1899:65535 1900:40084 1900:65535 1901:41733 1901:65535 1902:43409 1902:65535 1903:45112 1903:65535 1904:46841 1904:65535 1905:48596 1905:65535 1906:50380 1906:65535 1907:52191 1907:65535 1908:54030 1908:65535 1909:55899 1909:65535 1910:57796 1910:65535 1911:59723 1911:65535 1912:61680 1912:65535 1913:63668 1913:65535 1914:152 1914:65535 1914:65535 1915:2202 1915:65535 1915:65535 1916:4285 1916:65535 1916:65535 1917:6399 1917:65535 1917:65535 1918:8547 1918:65535 1918:65535 1919:10729 1919:65535 1919:65535 1920:12945 1920:65535 1920:65535 1921:15195 1921:65535 1921:65535 1922:17480 1922:65535 1922:65535 1923:19801 1923:65535 1923:65535 1924:22158 1924:65535 1924:65535 1925:24553 1925:65535 1925:65535 1926:26985 1926:65535 1926:65535 1927:29453 1927:65535 1927:65535 1928:31963 1928:65535 1928:65535 1929:34509 1929:65535 1929:65535 1930:37097 1930:65535 1930:65535 1931:39724 1931:65535 1931:65535 1932:17411"},
        ];

        let double_insert_cases = &[
            Case { lower: 1_000.0, upper: 10_000.0, count: 10_000_000 - 1, allowed_err: 0.0002, expected: "1784:34970 1785:35516 1786:36070 1787:36636 1788:37208 1789:37788 1790:38380 1791:38978 1792:39588 1793:40206 1794:40836 1795:41472 1796:42122 1797:42778 1798:43448 1799:44126 1800:44816 1801:45516 1802:46226 1803:46950 1804:47682 1805:48430 1806:49184 1807:49954 1808:50732 1809:51528 1810:52330 1811:53150 1812:53980 1813:54824 1814:55678 1815:56550 1816:57434 1817:58330 1818:59244 1819:60166 1820:61108 1821:62064 1822:63032 1823:64018 1824:65018 1825:497 1825:65535 1826:1531 1826:65535 1827:2579 1827:65535 1828:3643 1828:65535 1829:4723 1829:65535 1830:5821 1830:65535 1831:6935 1831:65535 1832:8069 1832:65535 1833:9219 1833:65535 1834:10387 1834:65535 1835:11573 1835:65535 1836:12777 1836:65535 1837:14001 1837:65535 1838:15245 1838:65535 1839:16505 1839:65535 1840:17789 1840:65535 1841:19089 1841:65535 1842:20413 1842:65535 1843:21755 1843:65535 1844:23119 1844:65535 1845:24505 1845:65535 1846:25911 1846:65535 1847:27341 1847:65535 1848:28791 1848:65535 1849:30265 1849:65535 1850:31761 1850:65535 1851:33283 1851:65535 1852:34827 1852:65535 1853:36393 1853:65535 1854:37987 1854:65535 1855:39605 1855:65535 1856:41247 1856:65535 1857:42917 1857:65535 1858:44609 1858:65535 1859:46333 1859:65535 1860:48079 1860:65535 1861:49855 1861:65535 1862:51657 1862:65535 1863:53489 1863:65535 1864:55347 1864:65535 1865:57239 1865:65535 1866:59155 1866:65535 1867:61103 1867:65535 1868:63083 1868:65535 1869:65093 1869:65535 1870:1598 1870:65535 1870:65535 1871:3670 1871:65535 1871:65535 1872:5778 1872:65535 1872:65535 1873:7914 1873:65535 1873:65535 1874:10086 1874:65535 1874:65535 1875:12292 1875:65535 1875:65535 1876:14532 1876:65535 1876:65535 1877:16808 1877:65535 1877:65535 1878:19118 1878:65535 1878:65535 1879:21464 1879:65535 1879:65535 1880:23846 1880:65535 1880:65535 1881:26270 1881:65535 1881:65535 1882:28726 1882:65535 1882:65535 1883:31224 1883:65535 1883:65535 1884:33758 1884:65535 1884:65535 1885:36336 1885:65535 1885:65535 1886:38950 1886:65535 1886:65535 1887:41606 1887:65535 1887:65535 1888:44306 1888:65535 1888:65535 1889:47046 1889:65535 1889:65535 1890:49828 1890:65535 1890:65535 1891:52654 1891:65535 1891:65535 1892:55526 1892:65535 1892:65535 1893:58442 1893:65535 1893:65535 1894:61402 1894:65535 1894:65535 1895:64410 1895:65535 1895:65535 1896:1929 1896:65535 1896:65535 1896:65535 1897:5031 1897:65535 1897:65535 1897:65535 1898:8181 1898:65535 1898:65535 1898:65535 1899:11381 1899:65535 1899:65535 1899:65535 1900:14633 1900:65535 1900:65535 1900:65535 1901:17931 1901:65535 1901:65535 1901:65535 1902:21283 1902:65535 1902:65535 1902:65535 1903:24689 1903:65535 1903:65535 1903:65535 1904:28147 1904:65535 1904:65535 1904:65535 1905:31657 1905:65535 1905:65535 1905:65535 1906:35225 1906:65535 1906:65535 1906:65535 1907:38847 1907:65535 1907:65535 1907:65535 1908:42525 1908:65535 1908:65535 1908:65535 1909:46263 1909:65535 1909:65535 1909:65535 1910:50057 1910:65535 1910:65535 1910:65535 1911:53911 1911:65535 1911:65535 1911:65535 1912:57825 1912:65535 1912:65535 1912:65535 1913:61801 1913:65535 1913:65535 1913:65535 1914:304 1914:65535 1914:65535 1914:65535 1914:65535 1915:4404 1915:65535 1915:65535 1915:65535 1915:65535 1916:8570 1916:65535 1916:65535 1916:65535 1916:65535 1917:12798 1917:65535 1917:65535 1917:65535 1917:65535 1918:17094 1918:65535 1918:65535 1918:65535 1918:65535 1919:21458 1919:65535 1919:65535 1919:65535 1919:65535 1920:25890 1920:65535 1920:65535 1920:65535 1920:65535 1921:30390 1921:65535 1921:65535 1921:65535 1921:65535 1922:34960 1922:65535 1922:65535 1922:65535 1922:65535 1923:39602 1923:65535 1923:65535 1923:65535 1923:65535 1924:44316 1924:65535 1924:65535 1924:65535 1924:65535 1925:49106 1925:65535 1925:65535 1925:65535 1925:65535 1926:53970 1926:65535 1926:65535 1926:65535 1926:65535 1927:58906 1927:65535 1927:65535 1927:65535 1927:65535 1928:63926 1928:65535 1928:65535 1928:65535 1928:65535 1929:3483 1929:65535 1929:65535 1929:65535 1929:65535 1929:65535 1930:8659 1930:65535 1930:65535 1930:65535 1930:65535 1930:65535 1931:13913 1931:65535 1931:65535 1931:65535 1931:65535 1931:65535 1932:34822" },
        ];

        for case in cases {
            let mut sketch = AgentDDSketch::with_agent_defaults();
            assert!(sketch.is_empty());

            sketch.insert_interpolate_bucket(case.lower, case.upper, case.count);
            check_result(&sketch, case);
        }

        for case in double_insert_cases {
            let mut sketch = AgentDDSketch::with_agent_defaults();
            assert!(sketch.is_empty());

            sketch.insert_interpolate_bucket(case.lower, case.upper, case.count);
            sketch.insert_interpolate_bucket(case.lower, case.upper, case.count);

            let mut case = case.clone();
            case.count *= 2;
            check_result(&sketch, &case);
        }
    }

    fn test_relative_accuracy(config: Config, rel_acc: f64, min_value: f32, max_value: f32) {
        let max_observed_rel_acc = check_max_relative_accuracy(config, min_value, max_value);
        assert!(
            max_observed_rel_acc <= rel_acc + FLOATING_POINT_ACCEPTABLE_ERROR,
            "observed out of bound max relative acc: {}, target rel acc={}",
            max_observed_rel_acc,
            rel_acc
        );
    }

    fn compute_relative_accuracy(target: f64, actual: f64) -> f64 {
        assert!(
            !(target < 0.0 || actual < 0.0),
            "expected/actual values must be greater than 0.0; target={}, actual={}",
            target,
            actual
        );

        if target == actual {
            0.0
        } else if target == 0.0 {
            if actual == 0.0 {
                0.0
            } else {
                f64::INFINITY
            }
        } else if actual < target {
            (target - actual) / target
        } else {
            (actual - target) / target
        }
    }

    fn check_max_relative_accuracy(config: Config, min_value: f32, max_value: f32) -> f64 {
        assert!(
            min_value < max_value,
            "min_value must be less than max_value"
        );

        let mut v = min_value;
        let mut max_relative_acc = 0.0;
        while v < max_value {
            let target = f64::from(v);
            let actual = config.bin_lower_bound(config.key(target));

            let relative_acc = compute_relative_accuracy(target, actual);
            if relative_acc > max_relative_acc {
                max_relative_acc = relative_acc;
            }

            v = f32::from_bits(v.to_bits() + 1);
        }

        // Final iteration to make sure we hit the highest value.
        let actual = config.bin_lower_bound(config.key(f64::from(max_value)));
        let relative_acc = compute_relative_accuracy(f64::from(max_value), actual);
        if relative_acc > max_relative_acc {
            max_relative_acc = relative_acc;
        }

        max_relative_acc
    }

    #[test]
    fn test_round_to_even() {
        let alike = |a: f64, b: f64| -> bool {
            if a.is_nan() && b.is_nan() {
                true
            } else if a == b {
                a.is_sign_positive() == b.is_sign_positive()
            } else {
                false
            }
        };

        #[allow(clippy::unreadable_literal)]
        let test_cases = &[
            (f64::NAN, f64::NAN),
            (0.5000000000000001, 1.0), // 0.5+epsilon
            (0.0, 0.0),
            (1.390671161567e-309, 0.0), // denormal
            (0.49999999999999994, 0.0), // 0.5-epsilon
            (0.5, 0.0),
            (-1.5, -2.0),
            (-2.5, -2.0),
            (f64::INFINITY, f64::INFINITY),
            (2251799813685249.5, 2251799813685250.0), // 1 bit fraction
            (2251799813685250.5, 2251799813685250.0),
            (4503599627370495.5, 4503599627370496.0), // 1 bit fraction, rounding to 0 bit fraction
            (4503599627370497.0, 4503599627370497.0), // large integer
        ];

        for (input, expected) in test_cases {
            let actual = round_to_even(*input);
            assert!(
                alike(actual, *expected),
                "input -> {}, expected {}, got {}",
                *input,
                *expected,
                actual
            );
        }
    }
}
