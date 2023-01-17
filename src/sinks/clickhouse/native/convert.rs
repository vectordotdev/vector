use crate::event::LogEvent;
use chrono::TimeZone;
use chrono_tz::Tz;
use clickhouse_rs::types::{Block, DateTimeType, SqlType, Value as CHValue};
use either::Either;
use snafu::{ResultExt, Snafu};
use std::{
    collections::HashMap,
    net::{Ipv4Addr, Ipv6Addr},
    sync::Arc,
};
use value::Value;
use vector_core::event::Event;

pub(super) fn build_block(
    schema: Vec<(String, SqlType)>,
    events: Vec<Event>,
) -> crate::Result<Block> {
    let mut b = Block::new();
    for ev in &events {
        b.push(get_row_from_events(&schema, ev.clone().into_log())?)
            .map_err(Box::new)?;
    }
    Ok(b)
}

fn get_row_from_events(
    schema: &Vec<(String, SqlType)>,
    event: LogEvent,
) -> crate::Result<Vec<(String, CHValue)>> {
    let mut row = Vec::with_capacity(schema.len());
    for (col_name, ty) in schema {
        let event_field = event.get(col_name.as_str());
        let column = into_clickhouse_value(event_field, ty)?;
        row.push((col_name.clone(), column))
    }
    Ok(row)
}

#[derive(Debug, Snafu)]
enum ConvertError {
    #[snafu(display("cannot get data from column {col}"))]
    LackOfColumn { col: String },
    #[snafu(display("cannot convert {from} to {to}"))]
    TypeMisMatch { from: Value, to: String },
    #[snafu(context(false))]
    InvalidUTF8 { source: std::str::Utf8Error },
    #[snafu(display("invalid ip addr: {val}"))]
    InvalidIPAddr {
        source: std::net::AddrParseError,
        val: String,
    },
    #[snafu(display("invalid time format, val:{val}, format:{format}"))]
    InvalidTimeFormat {
        source: chrono::ParseError,
        val: String,
        format: &'static str,
    },
    #[snafu(display("only nullable type can be empty"))]
    NoValue,
    #[snafu(display("type {to} isn't supported, please file an issue"))]
    UnsupportedType { to: SqlType },
}

type CResult = std::result::Result<CHValue, ConvertError>;

fn into_integer(v: Option<&Value>, conv: impl Fn(i64) -> CHValue) -> CResult {
    match v {
        Some(Value::Integer(i)) => Ok(conv(*i)),
        Some(Value::Float(f)) => Ok(conv(f.into_inner() as i64)),
        Some(inner) => Err(ConvertError::TypeMisMatch {
            from: inner.clone(),
            to: stringify!($ty).to_string(),
        }),
        None => Err(ConvertError::NoValue),
    }
}

fn into_float(v: Option<&Value>, to_f32: bool) -> CResult {
    match v {
        None => Err(ConvertError::NoValue),
        Some(Value::Float(v)) => {
            if to_f32 {
                return Ok(CHValue::Float32(v.as_f32().into()));
            }
            Ok(CHValue::Float64(v.into_inner()))
        }
        Some(inner) => {
            let target_type = if to_f32 { "f32" } else { "f64" };
            Err(ConvertError::TypeMisMatch {
                from: inner.clone(),
                to: target_type.to_string(),
            })
        }
    }
}

fn into_ip(v: Option<&Value>, conv: impl Fn(&str) -> CResult) -> CResult {
    match v {
        None => Err(ConvertError::NoValue),
        Some(Value::Bytes(bs)) => {
            let w = &bs.to_vec()[..];
            let s = std::str::from_utf8(w)?;
            conv(s)
        }
        Some(inner) => Err(ConvertError::TypeMisMatch {
            from: inner.clone(),
            to: stringify!($chtype).to_string(),
        }),
    }
}

fn into_clickhouse_value(v: Option<&Value>, target_type: &SqlType) -> CResult {
    match target_type {
        SqlType::UInt8 => into_integer(v, |t| CHValue::UInt8(t as u8)),
        SqlType::UInt16 => into_integer(v, |t| CHValue::UInt16(t as u16)),
        SqlType::UInt32 => into_integer(v, |t| CHValue::UInt32(t as u32)),
        SqlType::UInt64 => into_integer(v, |t| CHValue::UInt64(t as u64)),
        SqlType::Int8 => into_integer(v, |t| CHValue::Int8(t as i8)),
        SqlType::Int16 => into_integer(v, |t| CHValue::Int16(t as i16)),
        SqlType::Int32 => into_integer(v, |t| CHValue::Int32(t as i32)),
        SqlType::Int64 => into_integer(v, |t| CHValue::Int64(t)),
        SqlType::String => into_string(v),
        SqlType::FixedString(len) => into_fixed_string(v, *len),
        SqlType::Float32 => into_float(v, true),
        SqlType::Float64 => into_float(v, false),
        SqlType::Date => into_date(v),
        SqlType::DateTime(ty) => match ty {
            DateTimeType::DateTime32 => into_datetime(v),
            DateTimeType::Chrono => Err(ConvertError::UnsupportedType {
                to: SqlType::DateTime(DateTimeType::Chrono),
            }),
            DateTimeType::DateTime64(p, t) => Err(ConvertError::UnsupportedType {
                to: SqlType::DateTime(DateTimeType::DateTime64(*p, *t)),
            }),
        },
        SqlType::Ipv4 => into_ip(v, |s| {
            let w: Ipv4Addr = s
                .parse()
                .context(InvalidIPAddrSnafu { val: s.to_string() })?;
            Ok(CHValue::Ipv4(w.octets()))
        }),
        SqlType::Ipv6 => into_ip(v, |s| {
            let w: Ipv6Addr = s
                .parse()
                .context(InvalidIPAddrSnafu { val: s.to_string() })?;
            Ok(CHValue::Ipv6(w.octets()))
        }),
        SqlType::Nullable(ty) => into_nullable(v, ty),
        SqlType::Array(ty) => into_array(v, ty),
        SqlType::Map(_, ty) => into_map(v, ty),
        _ => Err(ConvertError::UnsupportedType {
            to: target_type.clone(),
        }),
    }
}

fn into_nullable(v: Option<&Value>, target_type: &SqlType) -> CResult {
    let rs = into_clickhouse_value(v, target_type);
    match rs {
        Ok(v) => Ok(CHValue::Nullable(Either::Right(Box::new(v)))),
        Err(e) if matches!(e, ConvertError::NoValue) => Ok(CHValue::Nullable(Either::Left(
            (*target_type).clone().into(),
        ))),
        Err(e) => Err(e),
    }
}

fn into_array(v: Option<&Value>, target_type: &SqlType) -> CResult {
    match v {
        None => Err(ConvertError::NoValue),
        Some(Value::Array(arr)) => {
            let mut w = Vec::with_capacity(arr.len());
            for ev in arr {
                w.push(into_clickhouse_value(Some(ev), target_type)?);
            }
            Ok(CHValue::Array((*target_type).clone().into(), Arc::new(w)))
        }
        Some(inner) => Err(ConvertError::TypeMisMatch {
            from: inner.clone(),
            to: target_type.to_string().into_owned(),
        }),
    }
}

// only support Map(String, xxx)
fn into_map(v: Option<&Value>, target_type: &SqlType) -> CResult {
    match v {
        None => Err(ConvertError::NoValue),
        Some(Value::Object(bt)) => {
            let mut hm = HashMap::with_capacity(bt.len());
            for (k, v) in bt {
                hm.insert(
                    CHValue::String(Arc::new(k.clone().into_bytes())),
                    into_clickhouse_value(Some(v), target_type)?,
                );
            }
            Ok(CHValue::Map(
                SqlType::String.into(),
                (*target_type).clone().into(),
                Arc::new(hm),
            ))
        }
        Some(inner) => Err(ConvertError::TypeMisMatch {
            from: inner.clone(),
            to: target_type.to_string().into_owned(),
        }),
    }
}

fn into_string(v: Option<&Value>) -> CResult {
    match v {
        None => Err(ConvertError::NoValue),
        Some(Value::Bytes(bs)) => Ok(CHValue::String(Arc::new(bs.to_vec()))),
        Some(inner) => Err(ConvertError::TypeMisMatch {
            from: inner.clone(),
            to: "string".to_string(),
        }),
    }
}

fn into_fixed_string(v: Option<&Value>, len: usize) -> CResult {
    match v {
        None => Err(ConvertError::NoValue),
        Some(Value::Bytes(bs)) => {
            let mut w = bs.to_vec();
            w.truncate(len);
            Ok(CHValue::String(Arc::new(w)))
        }
        Some(inner) => Err(ConvertError::TypeMisMatch {
            from: inner.clone(),
            to: format!("fixedstring{}", len),
        }),
    }
}

const TIME_FORMAT: &str = "%d/%m/%Y %H:%M:%S%.9f%z";

fn into_date(v: Option<&Value>) -> CResult {
    match v {
        None => Err(ConvertError::NoValue),
        Some(Value::Timestamp(ts)) => {
            let s = ts.format(TIME_FORMAT).to_string();
            let t = Tz::UTC
                .datetime_from_str(s.as_str(), TIME_FORMAT)
                .context(InvalidTimeFormatSnafu {
                    val: s,
                    format: TIME_FORMAT,
                })?
                .date();
            Ok(t.into())
        }
        Some(Value::Integer(ts_nano)) => Ok(Tz::UTC.timestamp_nanos(*ts_nano).date().into()),
        Some(inner) => Err(ConvertError::TypeMisMatch {
            from: inner.clone(),
            to: "date".to_string(),
        }),
    }
}

fn into_datetime(v: Option<&Value>) -> CResult {
    match v {
        None => Err(ConvertError::NoValue),
        Some(Value::Timestamp(ts)) => Ok((*ts).into()),
        Some(Value::Integer(ts_nano)) => Ok(Tz::UTC.timestamp_nanos(*ts_nano).into()),
        Some(inner) => Err(ConvertError::TypeMisMatch {
            from: inner.clone(),
            to: "datetime".to_string(),
        }),
    }
}
