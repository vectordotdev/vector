use std::{borrow::Cow, fmt, str::FromStr};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Raw,
    Bytes,
}

pub fn default_unit() -> Unit {
    Unit::Raw
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Variant {
    Baseline,
    Comparison,
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
    pub experiment: Cow<'a, str>,
    pub variant: Variant,
    #[serde(borrow)]
    pub vector_id: Cow<'a, str>,
    pub time: f64,
    pub fetch_index: u64,
    #[serde(borrow)]
    pub query: Cow<'a, str>,
    #[serde(borrow)]
    pub query_id: Cow<'a, str>,
    pub value: f64,
    #[serde(default = "default_unit")]
    pub unit: Unit,
}
