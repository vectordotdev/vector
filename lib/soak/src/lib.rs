use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::{borrow::Cow, fmt, str::FromStr};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Variant {
    Baseline,
    Comparison,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum MetricKind {
    Counter,
    Gauge,
}

pub enum VariantError {
    Unknown,
}

impl fmt::Display for VariantError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            VariantError::Unknown => write!(f, "unknown, must be baseline|comparison"),
        }
    }
}

impl FromStr for Variant {
    type Err = VariantError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq("baseline") {
            return Ok(Self::Baseline);
        }
        if s.eq("comparison") {
            return Ok(Self::Comparison);
        }
        Err(VariantError::Unknown)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Output<'a> {
    #[serde(borrow)]
    /// An id that is mostly unique to this run, allowing us to distinguish
    /// duplications of the same observational setup.
    pub run_id: Cow<'a, Uuid>,
    #[serde(borrow)]
    pub experiment: Cow<'a, str>,
    pub variant: Variant,
    pub target: Cow<'a, str>,
    pub time: u128,
    pub fetch_index: u64,
    pub metric_name: Cow<'a, str>,
    pub metric_kind: MetricKind,
    pub metric_labels: BTreeMap<String, String>,
    pub value: f64,
}
