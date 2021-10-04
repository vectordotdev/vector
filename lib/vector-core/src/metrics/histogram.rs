use std::collections::{hash_map::Entry, HashMap, HashSet};

use ordered_float::OrderedFloat;

use crate::event::metric::Bucket;

use super::ddsketch::AgentDDSketch;

/// Translates cumulative histograms into a DDSketch for generating quantiles or forwarding to Datadog.
pub struct AgentDDSketchHistogram {
    sketch: AgentDDSketch,
    seen: HashMap<OrderedFloat<f64>, u32>,
}

impl AgentDDSketchHistogram {
    pub fn with_ddagent_defaults() -> Self {
        Self {
            sketch: AgentDDSketch::with_agent_defaults(),
            seen: HashMap::new(),
        }
    }

    pub fn sketch(&self) -> &DDSketch {
        &self.sketch
    }

    fn track_bucket_change(&mut self, bucket: &Bucket) -> Option<u32> {
        let k = OrderedFloat(bucket.upper_limit);
        let new = bucket.count;
        match self.seen.entry(k) {
            Entry::Vacant(vacant) => {
                let _ = vacant.insert(new);
                Some(new)
            }
            Entry::Occupied(mut occupied) => {
                let current = occupied.get_mut();
                let delta = if *current > new {
                    Some((u32::MAX - *current) + new)
                } else if new > *current {
                    Some(new - *current)
                } else {
                    None
                };

                *current = new;
                delta
            }
        }
    }

    pub fn insert_buckets(&mut self, buckets: Vec<Bucket>) {
        // If we already have _any_ buckets, and these new buckets don't match, reset our entire
        // state.  It's likely to get messed up if some buckets still exist and we're tracking their
        // count deltas, and new bucketing feels like a reasonable signal to reset ourselves.
        if !self.seen.is_empty() {
            let mut seen_buckets = self.seen.keys().cloned().collect::<HashSet<_>>();
            for bucket in &buckets {
                let _ = seen_buckets.remove(&OrderedFloat(bucket.upper_limit));
            }

            if !seen_buckets.is_empty() {
                self.seen.clear();
                self.sketch = DDSketch::new(self.config.clone());
                println!("clearing existing buckets, new buckets mismatched")
            } else {
                println!("buckets matched, proceeding");
            }
        }

        let mut lower = 0.0;

        for bucket in buckets {
            if let Some(delta) = self.track_bucket_change(&bucket) {
                println!(
                    "bucket had delta, interpolating (ub={},delta={})",
                    bucket.upper_limit, delta
                );
                let mut upper = bucket.upper_limit;
                if upper.is_sign_positive() && upper.is_infinite() {
                    upper = lower;
                }

                self.interpolate_bucket(lower, upper, delta);
            }
            lower = bucket.upper_limit;
        }
    }

    pub fn interpolate_bucket(&mut self, lower: f64, upper: f64, count: u32) {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use sketches_ddsketch::Config;

    use crate::{event::metric::Bucket, metrics::handle::Histogram};

    use super::HistogramSketch;

    static HISTO_VALUES: &[u64] = &[
        104221, 10206, 32436, 121686, 92848, 83685, 23739, 15122, 50491, 88507, 48318, 28004,
        29576, 8735, 77693, 33965, 88047, 7592, 64138, 59966, 117956, 112525, 41743, 82790, 27084,
        26967, 75008, 10752, 96636, 97150, 60768, 33411, 24746, 91872, 59057, 48329, 16756, 100459,
        117640, 59244, 107584, 124303, 32368, 109940, 106353, 90452, 84471, 39086, 91119, 89680,
        41339, 23329, 25629, 98156, 97002, 9538, 73671, 112586, 101616, 70719, 117291, 90043,
        10713, 49195, 60656, 60887, 47332, 113675, 8371, 42619, 33489, 108629, 70501, 84355, 24576,
        34468, 76756, 110706, 42854, 83841, 120751, 66494, 65210, 70244, 118529, 28021, 51603,
        96315, 92364, 59120, 118968, 5484, 91790, 45171, 102756, 29673, 85303, 108322, 122793,
        88373,
    ];

    #[test]
    fn basic_test() {
        let mut histo_sketch = HistogramSketch::with_dd_agent_defaults();
        assert!(histo_sketch.sketch().is_empty());

        let histo = Histogram::new();
        for num in HISTO_VALUES {
            histo.record((*num as f64) / 10_000.0);
        }

        let buckets = histo
            .buckets()
            .map(|(ub, n)| Bucket {
                upper_limit: ub,
                count: n,
            })
            .collect::<Vec<_>>();
        histo_sketch.insert_buckets(buckets);

        assert!(!histo_sketch.sketch().is_empty());
    }

    #[test]
    fn negative_to_positive() {
        let mut histo_sketch = HistogramSketch::with_dd_agent_defaults();
        assert!(histo_sketch.sketch().is_empty());

        let bounds = &[
            -10.0, -1.0, -0.1, -0.01, -0.001, 0.0, 0.001, 0.01, 0.1, 1.0, 10.0,
        ];

        let buckets = bounds
            .iter()
            .map(|b| Bucket {
                upper_limit: *b,
                count: 1,
            })
            .collect::<Vec<_>>();
        histo_sketch.insert_buckets(buckets);

        assert!(!histo_sketch.sketch().is_empty());
    }
}
