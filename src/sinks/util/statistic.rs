use crate::event::metric::Sample;
use snafu::Snafu;
use std::cmp::Ordering;

#[derive(Debug, Snafu)]
pub enum ValidationError {
    #[snafu(display("Quantiles must be in range [0.0,1.0]"))]
    QuantileOutOfRange,
}

pub struct DistributionStatistic {
    pub min: f64,
    pub max: f64,
    pub median: f64,
    pub avg: f64,
    pub sum: f64,
    pub count: u64,
    /// (quantile, value)
    pub quantiles: Vec<(f64, f64)>,
}

impl DistributionStatistic {
    pub fn from_samples(source: &[Sample], quantiles: &[f64]) -> Option<Self> {
        let mut bins = source
            .iter()
            .filter(|sample| sample.rate > 0)
            .cloned()
            .collect::<Vec<_>>();

        if bins.is_empty() {
            return None;
        }

        if bins.len() == 1 {
            let val = bins[0].value;
            let count = bins[0].rate;
            return Some(Self {
                min: val,
                max: val,
                median: val,
                avg: val,
                sum: val * count as f64,
                count: count as u64,
                quantiles: quantiles.iter().map(|&p| (p, val)).collect(),
            });
        }

        bins.sort_unstable_by(|a, b| a.value.partial_cmp(&b.value).unwrap_or(Ordering::Equal));

        let min = bins.first().unwrap().value;
        let max = bins.last().unwrap().value;
        let sum = bins
            .iter()
            .map(|sample| sample.value * sample.rate as f64)
            .sum::<f64>();
        let count = bins.iter().map(|sample| sample.rate).sum::<u32>();
        let avg = sum / count as f64;

        for i in 1..bins.len() {
            bins[i].rate += bins[i - 1].rate;
        }

        let median = find_sample(&bins, (0.50 * count as f64 - 1.0).round() as u32);
        let quantiles = quantiles
            .iter()
            .map(|&p| {
                let idx = (p * count as f64 - 1.0).round() as u32;
                (p, find_sample(&bins, idx))
            })
            .collect();

        Some(Self {
            min,
            max,
            median,
            avg,
            sum,
            count: count as u64,
            quantiles,
        })
    }
}

/// `bins` is a cumulative histogram
fn find_sample(bins: &[Sample], index: u32) -> f64 {
    let i = match bins.binary_search_by_key(&index, |sample| sample.rate) {
        Ok(i) => i,
        Err(i) => i,
    };
    bins[i].value
}

pub fn validate_quantiles(quantiles: &[f64]) -> Result<(), ValidationError> {
    if quantiles
        .iter()
        .all(|&quantile| (0.0..=1.0).contains(&quantile))
    {
        Ok(())
    } else {
        Err(ValidationError::QuantileOutOfRange)
    }
}
