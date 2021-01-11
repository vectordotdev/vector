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
    pub fn new(values: &[f64], counts: &[u32], quantiles: &[f64]) -> Option<Self> {
        if values.len() != counts.len() {
            return None;
        }

        let mut bins = values
            .iter()
            .zip(counts.iter())
            .filter(|(_, &c)| c > 0)
            .map(|(v, c)| (*v, *c))
            .collect::<Vec<_>>();

        if bins.is_empty() {
            return None;
        }

        if bins.len() == 1 {
            let val = bins[0].0;
            let count = bins[0].1;
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

        bins.sort_unstable_by(|(a, _), (b, _)| a.partial_cmp(b).unwrap_or(Ordering::Equal));

        let min = bins.first().unwrap().0;
        let max = bins.last().unwrap().0;
        let sum = bins.iter().map(|(v, c)| v * *c as f64).sum::<f64>();
        let count = bins.iter().map(|(_, c)| *c).sum::<u32>();
        let avg = sum / count as f64;

        for i in 1..bins.len() {
            bins[i].1 += bins[i - 1].1;
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
fn find_sample(bins: &[(f64, u32)], index: u32) -> f64 {
    let i = match bins.binary_search_by_key(&index, |(_, c)| *c) {
        Ok(i) => i,
        Err(i) => i,
    };
    bins[i].0
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
