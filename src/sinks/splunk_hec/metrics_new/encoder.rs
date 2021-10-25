use std::{borrow::Cow, collections::BTreeMap};
use serde::{Serialize};

#[derive(Serialize, Debug, PartialEq)]
#[serde(untagged)]
pub enum FieldValue<'a> {
    Float(f64),
    Str(Cow<'a, str>),
}

impl<'a> From<&'a str> for FieldValue<'a> {
    fn from(s: &'a str) -> Self {
        FieldValue::Str(Cow::Borrowed(s))
    }
}

impl<'a> From<String> for FieldValue<'a> {
    fn from(s: String) -> Self {
        FieldValue::Str(Cow::Owned(s))
    }
}

impl<'a> From<f64> for FieldValue<'a> {
    fn from(f: f64) -> Self {
        FieldValue::Float(f)
    }
}

pub type FieldMap<'a> = BTreeMap<&'a str, FieldValue<'a>>;

pub struct HecMetricsEncoder;
