//! `analyzer` is a program that reads the capture file written by `observer`
//! and reports on the findings therein.
use argh::FromArgs;
use bytesize::ByteSize;
use ndarray::{ArrayBase, Axis};
use ndarray_stats::interpolate::Nearest;
use ndarray_stats::{QuantileExt, SummaryStatisticsExt};
use noisy_float::types::n64;
use std::collections::HashMap;
use std::io::{self, BufRead};
use std::path::PathBuf;
use std::{cmp, fmt, ops};
use tabled::{Style, Table, Tabled};

#[derive(FromArgs)]
/// vector soak `analyzer` options
struct Opts {
    /// path on disk to captures file to analyze
    #[argh(option)]
    captures: PathBuf,
    /// samples to skip
    #[argh(option)]
    skip_past: u64,
    /// maximum samples to consider
    #[argh(option)]
    maximum_samples: u64,
}

#[derive(Debug)]
struct Capture {
    experiments: HashMap<String, Experiment>, // experiment_name -> Experiment
}

#[derive(Debug, Default)]
struct Experiment {
    samples: HashMap<(String, String), Vec<Sample>>, // (variant, query) -> Samples
}

#[derive(Debug)]
struct Sample {
    time: f64,
    fetch_index: u64,
    value: f64,
    unit: soak::Unit,
}

#[derive(Debug, Clone, Copy)]
enum StatValue {
    Raw { original: f64, converted: u64 },
    Byte { original: f64, converted: ByteSize },
}

impl cmp::PartialOrd for StatValue {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        match (self, other) {
            (StatValue::Raw { original: l, .. }, StatValue::Raw { original: r, .. }) => {
                l.partial_cmp(r)
            }
            (StatValue::Byte { original: l, .. }, StatValue::Byte { original: r, .. }) => {
                l.partial_cmp(r)
            }
            _ => unreachable!(),
        }
    }
}

impl cmp::Ord for StatValue {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        match (self, other) {
            (StatValue::Raw { converted: l, .. }, StatValue::Raw { converted: r, .. }) => l.cmp(r),
            (StatValue::Byte { converted: l, .. }, StatValue::Byte { converted: r, .. }) => {
                l.cmp(r)
            }
            _ => unreachable!(),
        }
    }
}

impl cmp::PartialEq for StatValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (StatValue::Raw { original: l, .. }, StatValue::Raw { original: r, .. }) => l.eq(r),
            (StatValue::Byte { original: l, .. }, StatValue::Byte { original: r, .. }) => l.eq(r),
            _ => false,
        }
    }
}
impl cmp::Eq for StatValue {}

impl ops::Sub for StatValue {
    type Output = Self;

    fn sub(self, other: Self) -> Self::Output {
        match (self, other) {
            (
                StatValue::Raw {
                    original: lo,
                    converted: lc,
                },
                StatValue::Raw {
                    original: ro,
                    converted: rc,
                },
            ) => StatValue::Raw {
                original: lo - ro,
                converted: lc - rc,
            },
            (StatValue::Byte { original: lo, .. }, StatValue::Byte { original: ro, .. }) => {
                StatValue::Byte {
                    original: lo - ro,
                    converted: ByteSize::b((lo - ro) as u64),
                }
            }
            _ => unreachable!(),
        }
    }
}

impl ops::Add for StatValue {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        match (self, other) {
            (
                StatValue::Raw {
                    original: lo,
                    converted: lc,
                },
                StatValue::Raw {
                    original: ro,
                    converted: rc,
                },
            ) => StatValue::Raw {
                original: lo + ro,
                converted: lc + rc,
            },
            (StatValue::Byte { original: lo, .. }, StatValue::Byte { original: ro, .. }) => {
                StatValue::Byte {
                    original: lo + ro,
                    converted: ByteSize::b((lo + ro) as u64),
                }
            }
            _ => unreachable!(),
        }
    }
}

impl ops::Mul<f64> for StatValue {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self::Output {
        match self {
            StatValue::Raw { original: lo, .. } => StatValue::new(lo * rhs, soak::Unit::Raw),
            StatValue::Byte { original: lo, .. } => StatValue::new(lo * rhs, soak::Unit::Bytes),
        }
    }
}

impl StatValue {
    fn new(inner: f64, unit: soak::Unit) -> Self {
        match unit {
            soak::Unit::Bytes => Self::Byte {
                original: inner,
                converted: ByteSize::b(inner as u64),
            },
            soak::Unit::Raw => Self::Raw {
                original: inner,
                converted: inner as u64,
            },
        }
    }
}

impl fmt::Display for StatValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StatValue::Raw { converted, .. } => {
                write!(f, "{}", converted)
            }
            StatValue::Byte { converted, .. } => {
                write!(f, "{}", converted.to_string_as(true))
            }
        }
    }
}

#[derive(Debug, Tabled)]
struct Statistics {
    experiment: String,
    variant: String,
    query: String,
    p50: StatValue,
    p75: StatValue,
    p90: StatValue,
    p99: StatValue,
    max: StatValue,
    skewness: f64,
    kurtosis: f64,
    iqr: StatValue,
    outliers: bool,
}

fn main() {
    tracing_subscriber::fmt().init();
    let ops: Opts = argh::from_env();
    let file: std::fs::File = std::fs::OpenOptions::new()
        .read(true)
        .open(ops.captures)
        .unwrap();
    let mut capture = Capture {
        experiments: HashMap::new(),
    };
    for line in io::BufReader::new(file).lines() {
        let line = line.unwrap();
        let output: soak::Output = serde_json::from_str(&line).unwrap();

        let experiment_id = output.experiment.to_string();
        let experiment = capture
            .experiments
            .entry(experiment_id)
            .or_insert(Experiment::default());
        let query_id = output.query.id.to_string();
        let variant = output.variant.to_string();
        let sample = Sample {
            time: output.time,
            fetch_index: output.fetch_index,
            value: output.query.value,
            unit: output.query.unit,
        };
        experiment
            .samples
            .entry((variant, query_id))
            .or_insert(Vec::default())
            .push(sample);
    }

    let mut statistics = Vec::with_capacity(capture.experiments.len());
    for (experiment_id, exp) in capture.experiments.into_iter() {
        for ((variant, query_id), samples) in exp.samples.into_iter() {
            let unit = samples[0].unit;
            let mut raw_array: ndarray::Array1<f64> =
                ArrayBase::from_iter(samples.iter().map(|s| s.value));
            let mut array: ndarray::Array1<StatValue> =
                ArrayBase::from_iter(samples.iter().map(|s| StatValue::new(s.value, unit)));
            let skewness = raw_array.skewness().unwrap();
            let kurtosis = raw_array.kurtosis().unwrap();
            let min = array[array.argmin().unwrap()];
            let max = array[array.argmax().unwrap()];
            let p25 = *array
                .quantile_axis_mut(Axis(0), n64(0.25), &Nearest)
                .unwrap()
                .first()
                .unwrap();
            let p50 = *array
                .quantile_axis_mut(Axis(0), n64(0.5), &Nearest)
                .unwrap()
                .first()
                .unwrap();
            let p75 = *array
                .quantile_axis_mut(Axis(0), n64(0.75), &Nearest)
                .unwrap()
                .first()
                .unwrap();
            let p90 = *array
                .quantile_axis_mut(Axis(0), n64(0.90), &Nearest)
                .unwrap()
                .first()
                .unwrap();
            let p99 = *array
                .quantile_axis_mut(Axis(0), n64(0.99), &Nearest)
                .unwrap()
                .first()
                .unwrap();
            let iqr = p75 - p25;
            let tukey_bound = iqr * 1.5;
            let lower = p25 - tukey_bound;
            let upper = p75 + tukey_bound;
            println!("{}, {}, {}", tukey_bound, lower, upper);
            let outliers = (min < lower) || (max > upper);
            statistics.push(Statistics {
                experiment: experiment_id.clone(),
                variant,
                query: query_id,
                max,
                p50,
                p75,
                p90,
                p99,
                skewness,
                kurtosis,
                iqr,
                outliers,
            });
        }
    }
    let table = Table::new(statistics)
        .with(Style::github_markdown())
        .to_string();
    println!("{}", table);
}
