use crate::{
    ast::{Function, FunctionArgument},
    date,
    parse_grok::Error as GrokRuntimeError,
    parse_grok_rules::Error as GrokStaticError,
};

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::{Tz, UTC};
use ordered_float::NotNan;
use regex::Regex;
use std::{convert::TryFrom, string::ToString};
use strum_macros::Display;
use vrl_compiler::Value;

#[derive(Debug, Display, Clone)]
pub enum GrokFilter {
    Date(String, Option<Regex>, Option<String>, bool),
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
                    if let FunctionArgument::Arg(ref null_value) = args[0] {
                        Some(GrokFilter::NullIf(null_value.to_string()))
                    } else {
                        None
                    }
                })
                .ok_or_else(|| GrokStaticError::InvalidFunctionArguments(f.name.clone())),
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
                .map(|f| (f as i64).into())
                .map_err(|_e| {
                    GrokRuntimeError::FailedToApplyFilter(filter.to_string(), value.to_string())
                })?),
            _ => Err(GrokRuntimeError::FailedToApplyFilter(
                filter.to_string(),
                value.to_string(),
            )),
        },
        GrokFilter::Number | GrokFilter::NumberExt => match value {
            Value::Bytes(v) => Ok(String::from_utf8_lossy(v)
                .parse::<f64>()
                .map_err(|_e| {
                    GrokRuntimeError::FailedToApplyFilter(filter.to_string(), value.to_string())
                })?
                .into()),
            _ => Err(GrokRuntimeError::FailedToApplyFilter(
                filter.to_string(),
                value.to_string(),
            )),
        },
        GrokFilter::Scale(scale_factor) => match value {
            Value::Integer(v) => Ok(Value::Float(
                NotNan::new((*v as f64) * scale_factor).expect("NaN"),
            )),
            Value::Float(v) => Ok(Value::Float(*v * scale_factor)),
            _ => Err(GrokRuntimeError::FailedToApplyFilter(
                filter.to_string(),
                value.to_string(),
            )),
        },
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
            Value::Bytes(_) => {
                if value.to_string() == *null_value {
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
        GrokFilter::Date(format, tz_regex_opt, target_tz, with_tz) => match value {
            Value::Bytes(bytes) => {
                let value = String::from_utf8_lossy(bytes);
                match tz_regex_opt {
                    Some(re) => {
                        let tz = re
                            .captures(&value)
                            .ok_or_else(|| {
                                GrokRuntimeError::FailedToApplyFilter(
                                    filter.to_string(),
                                    value.to_string(),
                                )
                            })?
                            .name("tz")
                            .unwrap()
                            .as_str();
                        let tz: Tz = tz.parse().map_err(|error| {
                            error!(message = "Error parsing tz", tz = %tz,  %error);
                            GrokRuntimeError::FailedToApplyFilter(
                                filter.to_string(),
                                value.to_string(),
                            )
                        })?;
                        let naive_date = NaiveDateTime::parse_from_str(&value, format).map_err(|error|
                            {
                                error!(message = "Error parsing date", value = %value, format = %format, %error);
                                GrokRuntimeError::FailedToApplyFilter(
                                    filter.to_string(),
                                    value.to_string(),
                                )
                            })?;
                        let dt = tz
                            .from_local_datetime(&naive_date)
                            .single()
                            .ok_or_else(|| {
                                GrokRuntimeError::FailedToApplyFilter(
                                    filter.to_string(),
                                    value.to_string(),
                                )
                            })?;
                        Ok(Utc
                            .from_utc_datetime(&dt.naive_utc())
                            .timestamp_millis()
                            .into())
                    }
                    None => {
                        if *with_tz {
                            Ok(DateTime::parse_from_str(&value, format)
                                .map_err(|error| {
                                    error!(message = "Error parsing date", date = %value, %error);
                                    GrokRuntimeError::FailedToApplyFilter(
                                        filter.to_string(),
                                        value.to_string(),
                                    )
                                })?
                                .timestamp_millis()
                                .into())
                        } else if let Ok(dt) = NaiveDateTime::parse_from_str(&value, format) {
                            if let Some(tz) = target_tz {
                                let tzs = date::parse_timezone(tz).map_err(|error| {
                                    error!(message = "Error parsing tz", tz = %tz,  %error);
                                    GrokRuntimeError::FailedToApplyFilter(
                                        filter.to_string(),
                                        value.to_string(),
                                    )
                                })?;
                                let dt =
                                    tzs.from_local_datetime(&dt).single().ok_or_else(|| {
                                        GrokRuntimeError::FailedToApplyFilter(
                                            filter.to_string(),
                                            value.to_string(),
                                        )
                                    })?;
                                Ok(Utc
                                    .from_utc_datetime(&dt.naive_utc())
                                    .timestamp_millis()
                                    .into())
                            } else {
                                Ok(dt.timestamp_millis().into())
                            }
                        } else if let Ok(nt) = NaiveTime::parse_from_str(&value, format) {
                            // try parsing as naive time
                            Ok(NaiveDateTime::new(NaiveDate::from_ymd(1970, 1, 1), nt)
                                .timestamp_millis()
                                .into())
                        } else {
                            // try parsing as naive date
                            let nd =
                                NaiveDate::parse_from_str(&value, format).map_err(|error| {
                                    error!(message = "Error parsing date", date = %value, %error);
                                    GrokRuntimeError::FailedToApplyFilter(
                                        filter.to_string(),
                                        value.to_string(),
                                    )
                                })?;
                            Ok(UTC
                                .from_local_datetime(&NaiveDateTime::new(
                                    nd,
                                    NaiveTime::from_hms(0, 0, 0),
                                ))
                                .single()
                                .ok_or_else(|| {
                                    GrokRuntimeError::FailedToApplyFilter(
                                        filter.to_string(),
                                        value.to_string(),
                                    )
                                })?
                                .timestamp_millis()
                                .into())
                        }
                    }
                }
            }
            _ => Err(GrokRuntimeError::FailedToApplyFilter(
                filter.to_string(),
                value.to_string(),
            )),
        },
    }
}
