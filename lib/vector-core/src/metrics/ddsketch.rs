use std::{cmp, mem};

const AGENT_DEFAULT_BIN_LIMIT: u16 = 4096;
const AGENT_DEFAULT_EPS: f64 = 1.0 / 128.0;
const AGENT_DEFAULT_MIN_VALUE: f64 = 1.0e-9;

const UV_INF: i16 = i16::MAX;
const POS_INF_KEY: i16 = UV_INF;

const INITIAL_BINS: u16 = 128;
const MAX_BIN_WIDTH: u32 = u16::MAX as u32;

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

    if k == i16::max_value() {
        return f64::INFINITY;
    }

    if k == 0 {
        return 0.0;
    }

    pow_gamma(gamma_v, (k as i32 - bias) as f64)
}

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
    pub(self) fn new(mut eps: f64, min_value: f64, bin_limit: u16) -> Self {
        assert!(eps > 0.0 && eps < 1.0, "eps must be between 0.0 and 1.0");
        assert!(min_value > 0.0, "min value must be greater than 0.0");
        assert!(bin_limit > 0, "bin limit must be greater than 0");

        eps *= 2.0;
        let gamma_v = 1.0 + eps;
        let gamma_ln = eps.ln_1p();

        let norm_emin = log_gamma(gamma_ln, min_value).floor() as i32;
        let norm_bias = -norm_emin + 1;

        let norm_min = lower_bound(gamma_v, norm_bias, 1);

        assert!(
            norm_min <= min_value,
            "norm min should not exceed min_value"
        );

        Self {
            bin_limit,
            gamma_v,
            gamma_ln,
            norm_bias,
            norm_min,
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

        self.pow_gamma((k as i32 - self.norm_bias) as f64)
    }

    /// Gets the key for the given value.
    ///
    /// The key correponds to the bin where this value would be represented. The value returned here
    /// is such that: γ^k <= v < γ^(k+1).
    pub fn key(&self, v: f64) -> i16 {
        if v < 0.0 {
            return -self.key(-v);
        }

        if v == 0.0 || (v > 0.0 && v < self.norm_min) || (v < 0.0 && v > -self.norm_min) {
            return 0;
        }

        let rounded = round_to_even(self.log_gamma(v)) as i32;
        let key = rounded.wrapping_add(self.norm_bias);

        key.clamp(1, POS_INF_KEY as i32) as i16
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

#[derive(Clone, Copy)]
struct Bin {
    k: i16,
    n: u16,
}

impl Bin {
    fn increment(&mut self, n: u32) -> u32 {
        let next = n + self.n as u32;
        if next > MAX_BIN_WIDTH {
            self.n = MAX_BIN_WIDTH as u16;
            return next - MAX_BIN_WIDTH;
        }

        self.n = next as u16;
        0
    }
}

pub struct AgentDDSketch {
    config: Config,
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

    fn config(&self) -> &Config {
        &self.config
    }

    fn bin_count(&self) -> usize {
        self.bins.len()
    }

    /// Whether or not this sketch is empty.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Number of samples currently represented by this sketch.
    pub fn count(&self) -> u32 {
        self.count
    }

    fn adjust_basic_stats(&mut self, v: f64, n: u32) {
        if v < self.min {
            self.min = v;
        }

        if v > self.max {
            self.max = v;
        }

        self.count += n;
        self.sum += v * n as f64;

        if n == 1 {
            self.avg += (v - self.avg) / self.count as f64;
        } else {
            // TODO: From the Agent source code, this method apparently loses precision when the
            // two averages -- v and self.avg -- are close.  Is there a better approach?
            self.avg = self.avg + (v - self.avg) * n as f64 / self.count as f64;
        }
    }

    fn insert_key_counts(&mut self, mut counts: Vec<(i16, u32)>) {
        // Counts need to be sorted by key.
        counts.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));

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

            if bin.k < vk {
                temp.push(bin);
                bins_idx += 1;
            } else if bin.k > vk {
                generate_bins(&mut temp, vk, kn);
                key_idx += 1;
            } else {
                generate_bins(&mut temp, bin.k, bin.n as u32 + kn);
                bins_idx += 1;
                key_idx += 1;
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

        // PERF TOOD: This is where we might do a mem::swap instead so that we could shove the
        // bin vector into an object pool but I'm not sure this actually matters at the moment.
        self.bins = temp;
    }

    fn insert_keys(&mut self, mut keys: Vec<i16>) {
        keys.sort();

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

            if bin.k < vk {
                temp.push(bin);
                bins_idx += 1;
            } else if bin.k > vk {
                let kn = buf_count_leading_equal(&keys, key_idx);
                generate_bins(&mut temp, vk, kn);
                key_idx += kn as usize;
            } else {
                let kn = buf_count_leading_equal(&keys, key_idx);
                generate_bins(&mut temp, bin.k, bin.n as u32 + kn);
                bins_idx += 1;
                key_idx += kn as usize;
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

        // PERF TOOD: This is where we might do a mem::swap instead so that we could shove the
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

    pub fn insert_n(&mut self, v: f64, n: u32) {
        // TODO: this should return a result that makes sure we have enough room to actually add N
        // more samples without hitting `self.config.max_count()`
        self.adjust_basic_stats(v, n);

        let key = self.config.key(v);
        self.insert_key_counts(vec![(key, n)]);
    }

    pub fn insert_interpolate(&mut self, lower: f64, upper: f64, count: u32) {
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
            let fkn = ((upper_bound - lower_bound) / distance) * count as f64;
            if fkn > 1.0 {
                remainder += fkn - fkn.trunc();
            }

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
            let lower_bound = self.config.bin_lower_bound(last_key);
            self.adjust_basic_stats(lower_bound, remaining_count);
            key_counts.push((last_key, remaining_count));
        }

        self.insert_key_counts(key_counts);
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
            n += bin.n as f64;
            if n <= wanted_rank {
                continue;
            }

            let weight = (n - wanted_rank) / bin.n as f64;
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
                    other_bin.n as u32 + self.bins[bins_idx].n as u32,
                );
                bins_idx += 1;
            }
        }

        temp.extend_from_slice(&self.bins[bins_idx..]);
        trim_left(&mut temp, self.config.bin_limit);

        self.bins = temp;
    }
}

fn rank(count: u32, q: f64) -> f64 {
    round_to_even(q * (count - 1) as f64)
}

fn buf_count_leading_equal(keys: &[i16], start_idx: usize) -> u32 {
    if start_idx == keys.len() - 1 {
        return 1;
    }

    let mut idx = start_idx;
    while idx < keys.len() && keys[idx] == keys[start_idx] {
        idx += 1;
    }

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

    for i in 0..num_to_remove {
        let bin = bins[i];
        missing += bin.n as u32;

        if missing > MAX_BIN_WIDTH {
            overflow.push(Bin {
                k: bin.k,
                n: MAX_BIN_WIDTH as u16,
            });

            missing -= MAX_BIN_WIDTH;
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

fn generate_bins(bins: &mut Vec<Bin>, k: i16, n: u32) {
    if n < MAX_BIN_WIDTH {
        bins.push(Bin { k, n: n as u16 });
    } else {
        let overflow = n % MAX_BIN_WIDTH;
        if overflow != 0 {
            bins.push(Bin {
                k,
                n: overflow as u16,
            });
        }

        for _ in 0..(n / MAX_BIN_WIDTH) {
            bins.push(Bin {
                k,
                n: MAX_BIN_WIDTH as u16,
            });
        }
    }
}

#[inline]
fn capped_u64_shift(shift: u64) -> u32 {
    if shift >= 64 {
        u32::MAX
    } else {
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
    use super::{round_to_even, AgentDDSketch, Config, AGENT_DEFAULT_EPS};

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
        let max_value = config.gamma_v.powf(5.0) as f32;

        test_relative_accuracy(config, AGENT_DEFAULT_EPS, min_value, max_value)
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
            let target = v as f64;
            let actual = config.bin_lower_bound(config.key(target));

            let relative_acc = compute_relative_accuracy(target, actual);
            if relative_acc > max_relative_acc {
                max_relative_acc = relative_acc;
            }

            v = f32::from_bits(v.to_bits() + 1);
        }

        // Final iteration to make sure we hit the highest value.
        let actual = config.bin_lower_bound(config.key(max_value as f64));
        let relative_acc = compute_relative_accuracy(max_value as f64, actual);
        if relative_acc > max_relative_acc {
            max_relative_acc = relative_acc;
        }

        max_relative_acc
    }

    #[test]
    fn test_sketch_merge() {
        todo!()
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
