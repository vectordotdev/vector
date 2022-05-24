use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use ordered_float::NotNan;
use toml::value::Value as TomlValue;

use crate::value::{StdError, Value};

impl TryFrom<TomlValue> for Value {
    type Error = StdError;

    fn try_from(toml: TomlValue) -> Result<Self, StdError> {
        Ok(match toml {
            TomlValue::String(s) => Self::from(s),
            TomlValue::Integer(i) => Self::from(i),
            TomlValue::Array(a) => Self::from(
                a.into_iter()
                    .map(Self::try_from)
                    .collect::<Result<Vec<_>, StdError>>()?,
            ),
            TomlValue::Table(t) => Self::from(
                t.into_iter()
                    .map(|(k, v)| Self::try_from(v).map(|v| (k, v)))
                    .collect::<Result<BTreeMap<_, _>, StdError>>()?,
            ),
            TomlValue::Datetime(dt) => Self::from(dt.to_string().parse::<DateTime<Utc>>()?),
            TomlValue::Boolean(b) => Self::from(b),
            TomlValue::Float(f) => {
                Self::Float(NotNan::new(f).map_err(|_| "NaN value not supported")?)
            }
        })
    }
}
