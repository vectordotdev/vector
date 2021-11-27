// //! `analyzer` is a program that reads the capture file written by `observer`
// //! and reports on the findings therein.
// use argh::FromArgs;
// use bytesize::{to_string, ByteSize};
// use ndarray::{ArrayBase, Axis};
// use ndarray_stats::interpolate::Nearest;
// use ndarray_stats::{QuantileExt, SummaryStatisticsExt};
// use noisy_float::types::n64;
// use std::collections::HashMap;
// use std::io::{self, BufRead};
// use std::path::PathBuf;
// use std::{cmp, fmt, ops};
// use tabled::{Style, Table, Tabled};

// #[derive(FromArgs)]
// /// vector soak `analyzer` options
// struct Opts {
//     /// path on disk to captures file to analyze
//     #[argh(option)]
//     captures: PathBuf,
//     /// samples to skip
//     #[argh(option)]
//     skip_past: u64,
//     /// maximum samples to consider
//     #[argh(option)]
//     maximum_samples: u64,
// }

// #[derive(Debug)]
// struct Capture {
//     experiments: HashMap<String, Experiment>, // experiment_name -> Experiment
// }

// #[derive(Debug, Default)]
// struct Experiment {
//     samples: HashMap<(String, String), Vec<Sample>>, // (variant, query) -> Samples
// }

// #[derive(Debug)]
// struct Sample {
//     time: f64,
//     fetch_index: u64,
//     value: f64,
//     unit: soak::Unit,
// }

// #[derive(Debug, Clone, Copy)]
// enum StatValue {
//     Raw { original: f64, converted: u64 },
//     Byte { original: f64, converted: ByteSize },
// }

// impl cmp::PartialOrd for StatValue {
//     fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
//         match (self, other) {
//             (StatValue::Raw { original: l, .. }, StatValue::Raw { original: r, .. }) => {
//                 l.partial_cmp(r)
//             }
//             (StatValue::Byte { original: l, .. }, StatValue::Byte { original: r, .. }) => {
//                 l.partial_cmp(r)
//             }
//             _ => unreachable!(),
//         }
//     }
// }

// impl cmp::Ord for StatValue {
//     fn cmp(&self, other: &Self) -> cmp::Ordering {
//         match (self, other) {
//             (StatValue::Raw { converted: l, .. }, StatValue::Raw { converted: r, .. }) => l.cmp(r),
//             (StatValue::Byte { converted: l, .. }, StatValue::Byte { converted: r, .. }) => {
//                 l.cmp(r)
//             }
//             _ => unreachable!(),
//         }
//     }
// }

// impl cmp::PartialEq for StatValue {
//     fn eq(&self, other: &Self) -> bool {
//         match (self, other) {
//             (StatValue::Raw { original: l, .. }, StatValue::Raw { original: r, .. }) => l.eq(r),
//             (StatValue::Byte { original: l, .. }, StatValue::Byte { original: r, .. }) => l.eq(r),
//             _ => false,
//         }
//     }
// }
// impl cmp::Eq for StatValue {}

// impl ops::Sub for StatValue {
//     type Output = Self;

//     fn sub(self, other: Self) -> Self::Output {
//         match (self, other) {
//             (
//                 StatValue::Raw {
//                     original: lo,
//                     converted: lc,
//                 },
//                 StatValue::Raw {
//                     original: ro,
//                     converted: rc,
//                 },
//             ) => StatValue::Raw {
//                 original: lo - ro,
//                 converted: lc - rc,
//             },
//             (StatValue::Byte { original: lo, .. }, StatValue::Byte { original: ro, .. }) => {
//                 StatValue::Byte {
//                     original: lo - ro,
//                     converted: ByteSize::b((lo - ro) as u64),
//                 }
//             }
//             _ => unreachable!(),
//         }
//     }
// }

// impl ops::Add for StatValue {
//     type Output = Self;

//     fn add(self, other: Self) -> Self::Output {
//         match (self, other) {
//             (
//                 StatValue::Raw {
//                     original: lo,
//                     converted: lc,
//                 },
//                 StatValue::Raw {
//                     original: ro,
//                     converted: rc,
//                 },
//             ) => StatValue::Raw {
//                 original: lo + ro,
//                 converted: lc + rc,
//             },
//             (StatValue::Byte { original: lo, .. }, StatValue::Byte { original: ro, .. }) => {
//                 StatValue::Byte {
//                     original: lo + ro,
//                     converted: ByteSize::b((lo + ro) as u64),
//                 }
//             }
//             _ => unreachable!(),
//         }
//     }
// }

// impl ops::Mul<f64> for StatValue {
//     type Output = Self;

//     fn mul(self, rhs: f64) -> Self::Output {
//         match self {
//             StatValue::Raw { original: lo, .. } => StatValue::new(lo * rhs, soak::Unit::Raw),
//             StatValue::Byte { original: lo, .. } => StatValue::new(lo * rhs, soak::Unit::Bytes),
//         }
//     }
// }

// impl ops::Div<f64> for StatValue {
//     type Output = Self;

//     fn div(self, rhs: f64) -> Self::Output {
//         match self {
//             StatValue::Raw { original: lo, .. } => StatValue::new(lo / rhs, soak::Unit::Raw),
//             StatValue::Byte { original: lo, .. } => StatValue::new(lo / rhs, soak::Unit::Bytes),
//         }
//     }
// }

// impl ops::Mul<StatValue> for StatValue {
//     type Output = Self;

//     fn mul(self, rhs: StatValue) -> Self::Output {
//         match (self, rhs) {
//             (StatValue::Raw { original: lo, .. }, StatValue::Raw { original: ro, .. }) => {
//                 StatValue::new(lo * ro, soak::Unit::Raw)
//             }
//             (StatValue::Byte { original: lo, .. }, StatValue::Byte { original: ro, .. }) => {
//                 StatValue::new(lo * ro, soak::Unit::Bytes)
//             }
//             _ => unreachable!(),
//         }
//     }
// }

// impl StatValue {
//     fn new(inner: f64, unit: soak::Unit) -> Self {
//         match unit {
//             soak::Unit::Bytes => Self::Byte {
//                 original: inner,
//                 converted: ByteSize::b(inner as u64),
//             },
//             soak::Unit::Raw => Self::Raw {
//                 original: inner,
//                 converted: inner as u64,
//             },
//         }
//     }

//     fn sqrt(&self) -> Self {
//         match self {
//             StatValue::Raw { original: lo, .. } => StatValue::new(lo.sqrt(), soak::Unit::Raw),
//             StatValue::Byte { original: lo, .. } => StatValue::new(lo.sqrt(), soak::Unit::Bytes),
//         }
//     }

//     fn as_inner(&self) -> f64 {
//         match self {
//             StatValue::Raw { original, .. } => *original,
//             StatValue::Byte { original, .. } => *original,
//         }
//     }
// }

// impl fmt::Display for StatValue {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         match self {
//             StatValue::Raw { converted, .. } => {
//                 write!(f, "{}", converted)
//             }
//             StatValue::Byte { converted, .. } => {
//                 write!(f, "{}", converted.to_string_as(true))
//             }
//         }
//     }
// }

// #[derive(Debug, Tabled, Clone)]
// struct Statistics {
//     experiment: String,
//     variant: String,
//     query: String,
//     mean: StatValue,
//     stdev: StatValue,
//     min: StatValue,
//     max: StatValue,
//     p50: StatValue,
//     p75: StatValue,
//     p90: StatValue,
//     p99: StatValue,
//     skewness: f64,
//     kurtosis: f64,
//     iqr: StatValue,
//     outliers: bool,
//     total_samples: usize,
// }

// // #[derive(Debug, Tabled, Clone, Copy)]
// // enum Change {
// //     /// The baseline is faster than comparison
// //     Regression,
// //     /// There is no statistically interesting change between the means of baseline and comparison
// //     NoChange,
// //     /// The comparison is faster than baseline
// //     Improvement,
// // }

// // #[derive(Debug, Tabled, Clone)]
// // struct SecondOrderStatistics {
// //     experiment: String,
// //     change: Change,
// //     t_value: f64,
// // }

// #[derive(Debug, Default)]
// struct ExperimentalPair {
//     baseline: Option<Statistics>,
//     comparison: Option<Statistics>,
// }

// impl ExperimentalPair {
//     /// Implements Welch's t-test, https://en.wikipedia.org/wiki/Welch%27s_t-test
//     fn t_test(&self) -> (f64, f64) {
//         let baseline = self.baseline.as_ref().unwrap();
//         let comparison = self.comparison.as_ref().unwrap();

//         let baseline_total_samples = baseline.total_samples as f64;
//         let baseline_degrees_freedom = baseline_total_samples - 1.0;
//         let baseline_stdev = baseline.stdev.as_inner();

//         let comparison_total_samples = comparison.total_samples as f64;
//         let comparison_degrees_freedom = comparison_total_samples - 1.0;
//         let comparison_stdev = comparison.stdev.as_inner();

//         let t_statistic = {
//             let diff_mean: f64 = (baseline.mean - comparison.mean).as_inner();
//             let baseline_std_error: f64 = baseline_stdev / baseline_total_samples.sqrt();
//             let comparison_std_error: f64 = comparison_stdev / comparison_total_samples.sqrt();
//             let denominator = (baseline_std_error + comparison_std_error).sqrt();
//             diff_mean / denominator
//         };

//         let degrees_of_freedom = {
//             let numerator = ((baseline_stdev.powf(2.0) / baseline_total_samples)
//                 + (comparison_stdev.powf(2.0) / comparison_total_samples))
//                 .powf(2.0);
//             let denominator = (baseline_stdev.powf(4.0) / baseline_degrees_freedom.powf(2.0))
//                 + (comparison_stdev.powf(4.0) / comparison_degrees_freedom.powf(2.0));

//             numerator / denominator
//         };

//         (t_statistic, degrees_of_freedom)
//     }
// }

// fn main() {
//     tracing_subscriber::fmt().init();
//     let ops: Opts = argh::from_env();
//     let file: std::fs::File = std::fs::OpenOptions::new()
//         .read(true)
//         .open(ops.captures)
//         .unwrap();
//     let mut capture = Capture {
//         experiments: HashMap::new(),
//     };
//     for line in io::BufReader::new(file).lines() {
//         let line = line.unwrap();
//         let output: soak::Output = serde_json::from_str(&line).unwrap();

//         let experiment_id = output.experiment.to_string();
//         let experiment = capture
//             .experiments
//             .entry(experiment_id)
//             .or_insert(Experiment::default());
//         let query_id = output.query.id.to_string();
//         let variant = output.variant.to_string();
//         let sample = Sample {
//             time: output.time,
//             fetch_index: output.fetch_index,
//             value: output.query.value,
//             unit: output.query.unit,
//         };
//         experiment
//             .samples
//             .entry((variant, query_id))
//             .or_insert(Vec::default())
//             .push(sample);
//     }

//     // Compute first-order statistics, those that are done per
//     // experiment/variant.
//     let mut experimental_pairs = HashMap::new();
//     let mut statistics = Vec::with_capacity(capture.experiments.len());
//     for (experiment_id, exp) in capture.experiments.into_iter() {
//         let ep = experimental_pairs
//             .entry(experiment_id.clone())
//             .or_insert(ExperimentalPair::default());
//         for ((variant, query_id), samples) in exp.samples.into_iter() {
//             let unit = samples[0].unit;
//             let mut raw_array: ndarray::Array1<f64> =
//                 ArrayBase::from_iter(samples.iter().map(|s| s.value));
//             let mut array: ndarray::Array1<StatValue> =
//                 ArrayBase::from_iter(samples.iter().map(|s| StatValue::new(s.value, unit)));
//             let skewness = raw_array.skewness().unwrap();
//             let kurtosis = raw_array.kurtosis().unwrap();
//             let mean = StatValue::new(raw_array.mean().unwrap(), unit);
//             // standard deviation
//             let mut base = StatValue::new(0.0, unit);
//             for sv in array.iter() {
//                 let distance_from_mean = *sv - mean;
//                 let square_dfm = distance_from_mean * distance_from_mean;
//                 base = base + square_dfm;
//             }
//             let variance = base / (raw_array.len() as f64);
//             let stdev = variance.sqrt();

//             let min = array[array.argmin().unwrap()];
//             let max = array[array.argmax().unwrap()];
//             let p25 = *array
//                 .quantile_axis_mut(Axis(0), n64(0.25), &Nearest)
//                 .unwrap()
//                 .first()
//                 .unwrap();
//             let p50 = *array
//                 .quantile_axis_mut(Axis(0), n64(0.5), &Nearest)
//                 .unwrap()
//                 .first()
//                 .unwrap();
//             let p75 = *array
//                 .quantile_axis_mut(Axis(0), n64(0.75), &Nearest)
//                 .unwrap()
//                 .first()
//                 .unwrap();
//             let p90 = *array
//                 .quantile_axis_mut(Axis(0), n64(0.90), &Nearest)
//                 .unwrap()
//                 .first()
//                 .unwrap();
//             let p99 = *array
//                 .quantile_axis_mut(Axis(0), n64(0.99), &Nearest)
//                 .unwrap()
//                 .first()
//                 .unwrap();
//             let iqr = p75 - p25;
//             let tukey_bound = iqr * 1.5;
//             let lower = p25 - tukey_bound;
//             let upper = p75 + tukey_bound;
//             let outliers = (min < lower) || (max > upper);
//             let stat = Statistics {
//                 experiment: experiment_id.clone(),
//                 variant: variant.clone(),
//                 query: experiment_id.clone(),
//                 mean,
//                 max,
//                 min,
//                 p50,
//                 p75,
//                 p90,
//                 p99,
//                 skewness,
//                 kurtosis,
//                 iqr,
//                 outliers,
//                 stdev,
//                 total_samples: raw_array.len(),
//             };
//             statistics.push(stat.clone());
//             // TODO make this an enum
//             if variant.eq("baseline") {
//                 ep.baseline = Some(stat)
//             } else {
//                 ep.comparison = Some(stat)
//             }
//         }
//     }

//     // Compute second-order statistics, those that are done per experiment
//     // between variants.
//     let mut second_order_statistics = Vec::with_capacity(experimental_pairs.len());
//     for (experiment_id, p) in experimental_pairs.iter() {
//         let (t_statistic, degrees_of_freedom) = p.t_test();
//         // let change = if t_value < 0.05 {
//         // }
//         // SecondOrderStatistics {

//         // }
//         unimplemented!()
//     }
//     println!("{:?}", experimental_pairs);

//     let table = Table::new(statistics)
//         .with(Style::github_markdown())
//         .to_string();
//     println!("{}", table);
// }

fn main() {}
