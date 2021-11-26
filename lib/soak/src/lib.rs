use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Raw,
    Bytes,
}

pub fn default_unit() -> Unit {
    Unit::Raw
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Query<'a> {
    #[serde(borrow)]
    pub query: Cow<'a, str>,
    #[serde(borrow)]
    pub id: Cow<'a, str>,
    pub value: f64,
    #[serde(default = "default_unit")]
    pub unit: Unit,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Output<'a> {
    #[serde(borrow)]
    pub experiment: Cow<'a, str>,
    #[serde(borrow)]
    pub variant: Cow<'a, str>,
    #[serde(borrow)]
    pub vector_id: Cow<'a, str>,
    pub time: f64,
    pub fetch_index: u64,
    pub query: Query<'a>,
}
