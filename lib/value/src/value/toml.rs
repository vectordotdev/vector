use crate::Value;
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;
use toml::Value as TomlValue;

pub type TomlError = Box<dyn std::error::Error + Send + Sync + 'static>;

impl TryFrom<TomlValue> for Value {
    type Error = TomlError;

    fn try_from(toml: TomlValue) -> Result<Self, TomlError> {
        Ok(match toml {
            TomlValue::String(s) => Self::from(s),
            TomlValue::Integer(i) => Self::from(i),
            TomlValue::Array(a) => Self::from(
                a.into_iter()
                    .map(Value::try_from)
                    .collect::<Result<Vec<_>, TomlError>>()?,
            ),
            TomlValue::Table(t) => Self::from(
                t.into_iter()
                    .map(|(k, v)| Value::try_from(v).map(|v| (k, v)))
                    .collect::<Result<BTreeMap<_, _>, TomlError>>()?,
            ),
            TomlValue::Datetime(dt) => Self::from(dt.to_string().parse::<DateTime<Utc>>()?),
            TomlValue::Boolean(b) => Self::from(b),
            TomlValue::Float(f) => Self::from(f),
        })
    }
}
