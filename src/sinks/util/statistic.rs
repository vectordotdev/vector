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

        let mut samples = Vec::new();
        for (v, c) in values.iter().zip(counts.iter()) {
            for _ in 0..*c {
                samples.push(*v);
            }
        }

        if samples.is_empty() {
            return None;
        }

        if samples.len() == 1 {
            let val = samples[0];
            return Some(Self {
                min: val,
                max: val,
                median: val,
                avg: val,
                sum: val,
                count: 1,
                quantiles: quantiles.iter().map(|&p| (p, val)).collect(),
            });
        }

        samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));

        let length = samples.len() as f64;
        let min = *samples.first().unwrap();
        let max = *samples.last().unwrap();

        let median = samples[(0.50 * length - 1.0).round() as usize];
        let quantiles = quantiles
            .iter()
            .map(|&p| {
                let sample = samples[(p * length - 1.0).round() as usize];
                (p, sample)
            })
            .collect();

        let sum = samples.iter().sum();
        let avg = sum / length;

        Some(Self {
            min,
            max,
            median,
            avg,
            sum,
            count: samples.len() as u64,
            quantiles,
        })
    }
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
