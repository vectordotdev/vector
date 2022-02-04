use std::{convert::TryFrom, string::ToString};

use ordered_float::NotNan;
use strum_macros::Display;
use vrl_compiler::Value;

use crate::{
    ast::{Function, FunctionArgument},
    filters::{array, keyvalue, keyvalue::KeyValueFilter},
    matchers::date::{apply_date_filter, DateFilter},
    parse_grok::Error as GrokRuntimeError,
    parse_grok_rules::Error as GrokStaticError,
};

#[derive(Debug, Display, Clone)]
pub enum GrokFilter {
    Date(DateFilter),
    Integer,
    IntegerExt,
    // with scientific notation support, e.g. 1e10
    Number,
    NumberExt,
    // with scientific notation support, e.g. 1.52e10
    NullIf(String),
    Scale(f64),
    Lowercase,
    Uppercase,
    Json,
    Array(
        Option<(String, String)>,
        Option<String>,
        Box<Option<GrokFilter>>,
    ),
    KeyValue(KeyValueFilter),
}

impl TryFrom<&Function> for GrokFilter {
    type Error = GrokStaticError;

    fn try_from(f: &Function) -> Result<Self, Self::Error> {
        match f.name.as_str() {
            "scale" => match f.args.as_ref() {
                Some(args) if !args.is_empty() => {
                    let scale_factor = match args[0] {
                        FunctionArgument::Arg(Value::Integer(scale_factor)) => scale_factor as f64,
                        FunctionArgument::Arg(Value::Float(scale_factor)) => {
                            scale_factor.into_inner()
                        }
                        _ => return Err(GrokStaticError::InvalidFunctionArguments(f.name.clone())),
                    };
                    Ok(GrokFilter::Scale(scale_factor))
                }
                _ => Err(GrokStaticError::InvalidFunctionArguments(f.name.clone())),
            },
            "integer" => Ok(GrokFilter::Integer),
            "integerExt" => Ok(GrokFilter::IntegerExt),
            "number" => Ok(GrokFilter::Number),
            "numberExt" => Ok(GrokFilter::NumberExt),
            "lowercase" => Ok(GrokFilter::Lowercase),
            "uppercase" => Ok(GrokFilter::Uppercase),
            "json" => Ok(GrokFilter::Json),
            "nullIf" => f
                .args
                .as_ref()
                .and_then(|args| {
                    if let FunctionArgument::Arg(Value::Bytes(null_value)) = &args[0] {
                        Some(GrokFilter::NullIf(
                            String::from_utf8_lossy(null_value).to_string(),
                        ))
                    } else {
                        None
                    }
                })
                .ok_or_else(|| GrokStaticError::InvalidFunctionArguments(f.name.clone())),
            "array" => array::filter_from_function(f),
            "keyvalue" => keyvalue::filter_from_function(f),
            _ => Err(GrokStaticError::UnknownFilter(f.name.clone())),
        }
    }
}

/// Applies a given Grok filter to the value and returns the result or error.
/// For detailed description and examples of specific filters check out https://docs.datadoghq.com/logs/log_configuration/parsing/?tab=filters
pub fn apply_filter(value: &Value, filter: &GrokFilter) -> Result<Value, GrokRuntimeError> {
    match filter {
        GrokFilter::Integer => match value {
            Value::Bytes(v) => Ok(String::from_utf8_lossy(v)
                .parse::<i64>()
                .map_err(|_e| {
                    GrokRuntimeError::FailedToApplyFilter(filter.to_string(), value.to_string())
                })?
                .into()),
            _ => Err(GrokRuntimeError::FailedToApplyFilter(
                filter.to_string(),
                value.to_string(),
            )),
        },
        GrokFilter::IntegerExt => match value {
            Value::Bytes(v) => Ok(String::from_utf8_lossy(v)
                .parse::<f64>()
                .map_err(|_e| {
                    GrokRuntimeError::FailedToApplyFilter(filter.to_string(), value.to_string())
                })
                .map(|f| (f as i64).into())?),
            _ => Err(GrokRuntimeError::FailedToApplyFilter(
                filter.to_string(),
                value.to_string(),
            )),
        },
        GrokFilter::Number | GrokFilter::NumberExt => match value {
            Value::Bytes(v) => {
                let v = Ok(String::from_utf8_lossy(v)
                    .parse::<f64>()
                    .map_err(|_e| {
                        GrokRuntimeError::FailedToApplyFilter(filter.to_string(), value.to_string())
                    })?
                    .into());
                match v {
                    Ok(Value::Float(v)) if (v.into_inner() as i64) as f64 == v.into_inner() => {
                        Ok(Value::Integer(v.into_inner() as i64))
                    }
                    _ => v,
                }
            }
            _ => Err(GrokRuntimeError::FailedToApplyFilter(
                filter.to_string(),
                value.to_string(),
            )),
        },
        GrokFilter::Scale(scale_factor) => {
            let v = match value {
                Value::Integer(v) => Ok(Value::Float(
                    NotNan::new((*v as f64) * scale_factor).expect("NaN"),
                )),
                Value::Float(v) => Ok(Value::Float(
                    NotNan::new(v.into_inner() * scale_factor).expect("NaN"),
                )),
                Value::Bytes(v) => {
                    let v = String::from_utf8_lossy(v).parse::<f64>().map_err(|_e| {
                        GrokRuntimeError::FailedToApplyFilter(filter.to_string(), value.to_string())
                    })?;
                    Ok(Value::Float(NotNan::new(v * scale_factor).expect("NaN")))
                }
                _ => Err(GrokRuntimeError::FailedToApplyFilter(
                    filter.to_string(),
                    value.to_string(),
                )),
            };
            match v {
                Ok(Value::Float(v)) if (v.into_inner() as i64) as f64 == v.into_inner() => {
                    Ok(Value::Integer(v.into_inner() as i64))
                }
                _ => v,
            }
        }
        GrokFilter::Lowercase => match value {
            Value::Bytes(bytes) => Ok(String::from_utf8_lossy(bytes).to_lowercase().into()),
            _ => Err(GrokRuntimeError::FailedToApplyFilter(
                filter.to_string(),
                value.to_string(),
            )),
        },
        GrokFilter::Uppercase => match value {
            Value::Bytes(bytes) => Ok(String::from_utf8_lossy(bytes).to_uppercase().into()),
            _ => Err(GrokRuntimeError::FailedToApplyFilter(
                filter.to_string(),
                value.to_string(),
            )),
        },
        GrokFilter::Json => match value {
            Value::Bytes(bytes) => serde_json::from_slice::<'_, serde_json::Value>(bytes.as_ref())
                .map_err(|_e| {
                    GrokRuntimeError::FailedToApplyFilter(filter.to_string(), value.to_string())
                })
                .map(|v| v.into()),
            _ => Err(GrokRuntimeError::FailedToApplyFilter(
                filter.to_string(),
                value.to_string(),
            )),
        },
        GrokFilter::NullIf(null_value) => match value {
            Value::Bytes(bytes) => {
                if String::from_utf8_lossy(bytes) == *null_value {
                    Ok(Value::Null)
                } else {
                    Ok(value.to_owned())
                }
            }
            _ => Err(GrokRuntimeError::FailedToApplyFilter(
                filter.to_string(),
                value.to_string(),
            )),
        },
        GrokFilter::Date(date_filter) => apply_date_filter(value, date_filter),
        GrokFilter::KeyValue(keyvalue_filter) => keyvalue::apply_filter(value, keyvalue_filter),
        GrokFilter::Array(brackets, delimiter, value_filter) => match value {
            Value::Bytes(bytes) => array::parse(
                String::from_utf8_lossy(bytes).as_ref(),
                brackets
                    .as_ref()
                    .map(|(start, end)| (start.as_str(), end.as_str())),
                delimiter.as_ref().map(|s| s.as_str()),
            )
            .map_err(|_e| {
                GrokRuntimeError::FailedToApplyFilter(filter.to_string(), value.to_string())
            })
            .and_then(|values| {
                if let Some(value_filter) = value_filter.as_ref() {
                    let result = values
                        .iter()
                        .map(|v| apply_filter(v, value_filter))
                        .collect::<Result<Vec<Value>, _>>()
                        .map(Value::from);
                    return result;
                }
                Ok(values.into())
            }),
            _ => Err(GrokRuntimeError::FailedToApplyFilter(
                filter.to_string(),
                value.to_string(),
            )),
        },
    }
}
