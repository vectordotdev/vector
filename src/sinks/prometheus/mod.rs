use vector_core::event::metric::{Bucket, Sample};

#[cfg(test)]
use vector_core::{
    event::{metric::MetricSketch, Metric, MetricValue},
    metrics::AgentDDSketch,
};

mod collector;
pub(crate) mod exporter;
pub(crate) mod remote_write;

fn default_histogram_buckets() -> Vec<f64> {
    vec![
        0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
    ]
}

fn default_summary_quantiles() -> Vec<f64> {
    vec![0.5, 0.75, 0.9, 0.95, 0.99]
}

fn samples_to_buckets<'a, I>(samples: I, buckets: &[f64]) -> (Vec<Bucket>, u32, f64)
where
    I: Iterator<Item = &'a Sample> + 'a,
{
    let mut counts = vec![0; buckets.len()];
    let mut sum = 0.0;
    let mut count = 0;
    for sample in samples {
        buckets
            .iter()
            .enumerate()
            .skip_while(|&(_, b)| *b < sample.value)
            .for_each(|(i, _)| {
                counts[i] += sample.rate;
            });

        sum += sample.value * (sample.rate as f64);
        count += sample.rate;
    }

    let buckets = buckets
        .iter()
        .zip(counts.iter())
        .map(|(b, c)| Bucket {
            upper_limit: *b,
            count: *c,
        })
        .collect();

    (buckets, count, sum)
}

// TODO: These could be useful generic helper functions for metrics-related conversions.  We should
// consider moving them into `vector_core`.

#[cfg(test)]
fn distribution_to_agg_histogram(metric: Metric, buckets: &[f64]) -> Option<Metric> {
    let new_value = match metric.value() {
        MetricValue::Distribution { samples, .. } => {
            let (buckets, count, sum) = samples_to_buckets(samples.iter(), buckets);
            Some(MetricValue::AggregatedHistogram {
                buckets,
                count,
                sum,
            })
        }
        _ => None,
    };

    new_value.map(move |value| metric.with_value(value))
}

#[cfg(test)]
fn distribution_to_ddsketch(metric: Metric) -> Option<Metric> {
    let new_value = match metric.value() {
        MetricValue::Distribution { samples, .. } => {
            let mut sketch = AgentDDSketch::with_agent_defaults();
            for sample in samples {
                sketch.insert_n(sample.value, sample.rate);
            }

            Some(MetricValue::Sketch {
                sketch: MetricSketch::AgentDDSketch(sketch),
            })
        }
        _ => None,
    };

    new_value.map(move |value| metric.with_value(value))
}
