use std::{
    cmp::{self, Ordering},
    mem,
};

use core_common::byte_size_of::ByteSizeOf;
use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};

use crate::event::{metric::Bucket, Metric, MetricValue};

const AGENT_DEFAULT_BIN_LIMIT: u16 = 4096;
const AGENT_DEFAULT_EPS: f64 = 1.0 / 128.0;
const AGENT_DEFAULT_MIN_VALUE: f64 = 1.0e-9;

const UV_INF: i16 = i16::MAX;
const POS_INF_KEY: i16 = UV_INF;

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

    if k == i16::MAX {
        return f64::INFINITY;
    }

    if k == 0 {
        return 0.0;
    }

    pow_gamma(gamma_v, f64::from(i32::from(k) - bias))
}

#[derive(Copy, Clone, Debug)]
struct Config {
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

    /// Gets the value lower bound of the bin at the given key.
    pub fn bin_lower_bound(&self, k: i16) -> f64 {
        if k < 0 {
            return -self.bin_lower_bound(-k);
        }

        if k == POS_INF_KEY {
            return f64::INFINITY;
        }

        if k == 0 {
            return 0.0;
        }

        self.pow_gamma(f64::from(i32::from(k) - self.norm_bias))
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
        key.clamp(1, i32::from(POS_INF_KEY)) as i16
    }

    pub fn log_gamma(&self, v: f64) -> f64 {
        log_gamma(self.gamma_ln, v)
    }

    pub fn pow_gamma(&self, y: f64) -> f64 {
        pow_gamma(self.gamma_v, y)
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
        let mut total = 0;

        for bucket in buckets {
            let mut upper = bucket.upper_limit;
            if upper.is_sign_positive() && upper.is_infinite() {
                upper = lower;
            }

            // Each bucket should have a higher count than the last, since the logic of "contains
            // this many samples under this upper limit" implies that once a value fits into a
            // particular bucket, it fits into all subsequent buckets.
            //
            // Thus, we track the last bucket count and subtract it from the current bucket count to
            // get the correct value for "how many items are in this bucket if we exclude the
            // samples we've already accounted for?".
            let bucket_count = bucket.count - total;
            self.insert_interpolate_bucket(lower, upper, bucket_count);
            lower = bucket.upper_limit;
            total = bucket.count;
        }
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

    pub fn merge(&mut self, other: AgentDDSketch) {
        self.count += other.count;

        let mut temp = Vec::new();

        let mut bins_idx = 0;
        for other_bin in other.bins {
            let start = bins_idx;
            while bins_idx < self.bins.len() && self.bins[bins_idx].k < other_bin.k {
                bins_idx += 1;
            }

            temp.extend_from_slice(&self.bins[start..bins_idx]);

            if bins_idx >= self.bins.len() || self.bins[bins_idx].k > other_bin.k {
                temp.push(other_bin);
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
        // hand, and it's always locked to the constants used by the Datadog Agent.
        self.count == other.count
            && self.min == other.min
            && self.max == other.max
            && self.sum == other.sum
            && self.avg == other.avg
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
    use crate::{event::metric::Bucket, metrics::handle::Histogram};

    use super::{round_to_even, AgentDDSketch, Config, AGENT_DEFAULT_EPS};

    const FLOATING_POINT_ACCEPTABLE_ERROR: f64 = 1.0e-10;

    #[allow(clippy::unreadable_literal)]
    static HISTO_VALUES: &[f64] = &[
        104221.0, 10206.0, 32436.0, 121686.0, 92848.0, 83685.0, 23739.0, 15122.0, 50491.0, 88507.0,
        48318.0, 28004.0, 29576.0, 8735.0, 77693.0, 33965.0, 88047.0, 7592.0, 64138.0, 59966.0,
        117956.0, 112525.0, 41743.0, 82790.0, 27084.0, 26967.0, 75008.0, 10752.0, 96636.0, 97150.0,
        60768.0, 33411.0, 24746.0, 91872.0, 59057.0, 48329.0, 16756.0, 100459.0, 117640.0, 59244.0,
        107584.0, 124303.0, 32368.0, 109940.0, 106353.0, 90452.0, 84471.0, 39086.0, 91119.0,
        89680.0, 41339.0, 23329.0, 25629.0, 98156.0, 97002.0, 9538.0, 73671.0, 112586.0, 101616.0,
        70719.0, 117291.0, 90043.0, 10713.0, 49195.0, 60656.0, 60887.0, 47332.0, 113675.0, 8371.0,
        42619.0, 33489.0, 108629.0, 70501.0, 84355.0, 24576.0, 34468.0, 76756.0, 110706.0, 42854.0,
        83841.0, 120751.0, 66494.0, 65210.0, 70244.0, 118529.0, 28021.0, 51603.0, 96315.0, 92364.0,
        59120.0, 118968.0, 5484.0, 91790.0, 45171.0, 102756.0, 29673.0, 85303.0, 108322.0,
        122793.0, 88373.0,
    ];

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

        odd_values.merge(even_values);
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

    #[test]
    fn test_histogram_interpolation() {
        let mut histo_sketch = AgentDDSketch::with_agent_defaults();
        assert!(histo_sketch.is_empty());

        let histo = Histogram::new();
        for num in HISTO_VALUES {
            histo.record(num / 10_000.0);
        }

        let buckets = histo
            .buckets()
            .map(|(ub, n)| Bucket {
                upper_limit: ub,
                count: n,
            })
            .collect::<Vec<_>>();
        histo_sketch.insert_interpolate_buckets(buckets);

        assert!(!histo_sketch.is_empty());
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
        if target < 0.0 || actual < 0.0 {
            panic!(
                "expected/actual values must be greater than 0.0; target={}, actual={}",
                target, actual
            );
        }

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
