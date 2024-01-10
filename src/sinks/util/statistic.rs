use std::cmp::Ordering;

use snafu::Snafu;

use crate::event::metric::Sample;

#[derive(Debug, Snafu)]
pub enum ValidationError {
    #[snafu(display("Quantiles must be in range [0.0,1.0]"))]
    QuantileOutOfRange,
}

#[derive(Debug)]
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
            .copied()
            .collect::<Vec<_>>();

        match bins.len() {
            0 => None,
            1 => Some({
                let val = bins[0].value;
                let count = bins[0].rate;
                Self {
                    min: val,
                    max: val,
                    median: val,
                    avg: val,
                    sum: val * count as f64,
                    count: count as u64,
                    quantiles: quantiles.iter().map(|&p| (p, val)).collect(),
                }
            }),
            _ => Some({
                bins.sort_unstable_by(|a, b| {
                    a.value.partial_cmp(&b.value).unwrap_or(Ordering::Equal)
                });

                let min = bins.first().unwrap().value;
                let max = bins.last().unwrap().value;
                let sum = bins
                    .iter()
                    .map(|sample| sample.value * sample.rate as f64)
                    .sum::<f64>();

                for i in 1..bins.len() {
                    bins[i].rate += bins[i - 1].rate;
                }

                let count = bins.last().unwrap().rate;
                let avg = sum / count as f64;

                let median = find_quantile(&bins, 0.5);
                let quantiles = quantiles
                    .iter()
                    .map(|&p| (p, find_quantile(&bins, p)))
                    .collect();

                Self {
                    min,
                    max,
                    median,
                    avg,
                    sum,
                    count: count as u64,
                    quantiles,
                }
            }),
        }
    }
}

/// `bins` is a cumulative histogram
/// We are using R-3 (without choosing the even integer in the case of a tie),
/// it might be preferable to use a more common function, such as R-7.
///
/// List of quantile functions:
/// <https://en.wikipedia.org/wiki/Quantile#Estimating_quantiles_from_a_sample>
fn find_quantile(bins: &[Sample], p: f64) -> f64 {
    let count = bins.last().expect("bins is empty").rate;
    find_sample(bins, (p * count as f64).round() as u32)
}

/// `bins` is a cumulative histogram
/// Return the i-th smallest value,
/// i starts from 1 (i == 1 mean the smallest value).
/// i == 0 is equivalent to i == 1.
fn find_sample(bins: &[Sample], i: u32) -> f64 {
    let index = match bins.binary_search_by_key(&i, |sample| sample.rate) {
        Ok(index) => index,
        Err(index) => index,
    };
    bins[index].value
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

#[cfg(test)]
mod test {
    use super::*;

    impl PartialEq<Self> for DistributionStatistic {
        fn eq(&self, other: &Self) -> bool {
            self.min == other.min
                && self.max == other.max
                && self.median == other.median
                && self.avg == other.avg
                && self.sum == other.sum
                && self.count == other.count
                && self
                    .quantiles
                    .iter()
                    .zip(other.quantiles.iter())
                    .all(|(this, other)| this.0 == other.0 && this.1 == other.1)
        }
    }

    impl Eq for DistributionStatistic {}

    fn samples(v: &[(f64, u32)]) -> Vec<Sample> {
        v.iter()
            .map(|&(value, rate)| Sample { value, rate })
            .collect()
    }

    #[test]
    fn test_distribution() {
        // should return None on empty input
        assert_eq!(DistributionStatistic::from_samples(&[], &[0.5]), None);
        assert_eq!(
            DistributionStatistic::from_samples(&samples(&[(0.0, 0)]), &[0.5]),
            None
        );

        // test len == 1 case
        assert_eq!(
            DistributionStatistic::from_samples(&samples(&[(0.9, 100)]), &[0.5],).unwrap(),
            DistributionStatistic {
                min: 0.9,
                max: 0.9,
                median: 0.9,
                avg: 0.9,
                sum: 90.0,
                count: 100,
                quantiles: vec![(0.5, 0.9)],
            }
        );

        assert_eq!(
            DistributionStatistic::from_samples(
                &samples(&[(1.0, 1), (2.0, 1), (3.0, 1), (4.0, 1), (5.0, 1)]),
                &[]
            )
            .unwrap(),
            DistributionStatistic {
                min: 1.0,
                max: 5.0,
                median: 3.0,
                avg: 3.0,
                sum: 15.0,
                count: 5,
                quantiles: Vec::new(),
            }
        );

        assert_eq!(
            DistributionStatistic::from_samples(
                &samples(&[(1.0, 1), (2.0, 1), (4.0, 1), (3.0, 1)]),
                &[0.0, 1.0, 0.9]
            )
            .unwrap(),
            DistributionStatistic {
                min: 1.0,
                max: 4.0,
                median: 2.0,
                avg: 2.5,
                sum: 10.0,
                count: 4,
                quantiles: vec![(0.0, 1.0), (1.0, 4.0), (0.9, 4.0)],
            }
        );

        assert_eq!(
            DistributionStatistic::from_samples(
                &samples(&[(1.0, 2), (2.0, 1), (3.0, 4), (4.0, 3)]),
                &[0.75, 0.3, 0.31, 0.29, 0.24],
            )
            .unwrap(),
            DistributionStatistic {
                min: 1.0,
                max: 4.0,
                median: 3.0,
                avg: 2.8,
                sum: 28.0,
                count: 10,
                quantiles: vec![
                    (0.75, 4.0),
                    (0.3, 2.0),
                    (0.31, 2.0),
                    (0.29, 2.0),
                    (0.24, 1.0)
                ],
            }
        );
    }
}
