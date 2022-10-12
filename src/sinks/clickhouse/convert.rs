use std::{
    collections::HashMap,
    sync::Arc,
    net::{Ipv4Addr, Ipv6Addr}
};
use chrono::TimeZone;
use chrono_tz::Tz;
use either::Either;
use snafu::{Snafu, ResultExt};
use value::Value;
use clickhouse_rs::{
    types::{Block, Value as CHValue, SqlType, DateTimeType},
};
use crate::event::LogEvent;

pub(super) fn build_block(schema: Vec<(String, SqlType)>, events: Vec<LogEvent>) -> crate::Result<Block> {
    let mut b = Block::new();
    for ev in &events {
        b.push(get_row_from_events(&schema, ev)?)
            .map_err(|e| Box::new(e))?;
    }
    Ok(b)
}

fn get_row_from_events(schema: &Vec<(String, SqlType)>, event: &LogEvent) -> crate::Result<Vec<(String, CHValue)>> {
    let mut row = vec![];
    for (col_name, ty) in schema {
        let event_field = event.get(col_name.as_str());
        let column = into_clickhouse_value(event_field, ty).map_err(|e| Box::new(e))?;
        row.push((col_name.clone(), column))
    } 
    Ok(row)
}

#[derive(Debug, Snafu)]
enum ConvertError {
    #[snafu(display("cannot get data from column {col}"))]
    LackOfColumn{col: String},
    #[snafu(display("cannot convert {from} to {to}"))]
    TypeMisMatch{from: Value, to: String},
    #[snafu(context(false))]
    InvalidUTF8{source: std::str::Utf8Error},
    #[snafu(display("invalid ip addr: {val}"))]
    InvalidIPAddr{source: std::net::AddrParseError, val: String},
    #[snafu(display("invalid time format, val:{val}, format:{format}"))]
    InvalidTimeFormat{source: chrono::ParseError, val: String, format: &'static str},
    #[snafu(display("only nullable type can have no value"))]
    NoValue,
}

type CResult = std::result::Result<CHValue, ConvertError>;

macro_rules! gen_into_numeric {
    ($method:ident, $ty:ty, $vtype:ident, $chtype:ident) => {
        fn $method(v: Option<&Value>) -> CResult {
            if v.is_none() {
                return Err(ConvertError::NoValue);
            }
            let inner = v.unwrap();
            match inner {
                Value::$vtype(val) => Ok(CHValue::$chtype(*val as $ty)),
                _ => Err(ConvertError::TypeMisMatch{from: inner.clone(), to: stringify!($ty).to_string()})
            }
        }
    }
}

gen_into_numeric!(into_u8, u8, Integer, UInt8);
gen_into_numeric!(into_u16, u16, Integer, UInt16);
gen_into_numeric!(into_u32, u32, Integer, UInt32);
gen_into_numeric!(into_u64, u64, Integer, UInt64);
gen_into_numeric!(into_i8, i8, Integer, Int8);
gen_into_numeric!(into_i16, i16, Integer, Int16);
gen_into_numeric!(into_i32, i32, Integer, Int32);
gen_into_numeric!(into_i64, i64, Integer, Int64);


fn into_float(v: Option<&Value>, to_f32: bool) -> CResult {
    if v.is_none() {
        return Err(ConvertError::NoValue);
    }
    let inner = v.unwrap();
    match inner {
        Value::Float(v) => {
            if to_f32 {
                return Ok(CHValue::Float32(v.as_f32().into()));
            }
            return Ok(CHValue::Float64(v.into_inner()));
        },
        _ => {
            let target_type = if to_f32 {"f32"} else {"f64"};
            return Err(ConvertError::TypeMisMatch{from: inner.clone(), to: target_type.to_string()});
        }
    }
}

macro_rules! gen_into_ip {
    ($method:ident, $ty:ty, $chtype:ident) => {
        fn $method(v: Option<&Value>) -> CResult {
            if v.is_none() {
                return Err(ConvertError::NoValue);
            }
            let inner = v.unwrap();
            match inner {
                Value::Bytes(bs) => {
                    let w = &bs.to_vec()[..];
                    let s = std::str::from_utf8(w)?;
                    let addr: $ty = s.parse().context(InvalidIPAddrSnafu{val: s.to_string()})?;
                    Ok(CHValue::$chtype(addr.octets()))
                },
                _ => Err(ConvertError::TypeMisMatch{from: inner.clone(), to: stringify!($chtype).to_string()})
            }              
        }
    }
}

gen_into_ip!(into_ipv4, Ipv4Addr, Ipv4);
gen_into_ip!(into_ipv6, Ipv6Addr, Ipv6);


fn into_clickhouse_value(v: Option<&Value>, target_type: &SqlType) -> CResult {
    match target_type {
        SqlType::UInt8 => into_u8(v),
        SqlType::UInt16 => into_u16(v),
        SqlType::UInt32 => into_u32(v),
        SqlType::UInt64 => into_u64(v),
        SqlType::Int8 => into_i8(v),
        SqlType::Int16 => into_i16(v),
        SqlType::Int32 => into_i32(v),
        SqlType::Int64 => into_i64(v),
        SqlType::String => into_string(v),
        SqlType::FixedString(len) => into_fixed_string(v, *len),
        SqlType::Float32 => into_float(v, true),
        SqlType::Float64 => into_float(v, false),
        SqlType::Date => into_date(v),
        SqlType::DateTime(ty) => {
            match ty {
                DateTimeType::DateTime32 => into_datetime(v),
                _ => unimplemented!()
            }
        },
        SqlType::Ipv4 => into_ipv4(v),
        SqlType::Ipv6 => into_ipv6(v),
        SqlType::Nullable(ty) => into_nullable(v, *ty),
        SqlType::Array(ty) => into_array(v, ty),
        SqlType::Map(_, ty) => into_map(v, ty),
        _ => unimplemented!(),
    }
}

fn into_nullable(v: Option<&Value>, target_type: &SqlType) -> CResult {
    let rs = into_clickhouse_value(v, target_type);
    match rs {
        Ok(v) => {
            Ok(CHValue::Nullable(Either::Right(Box::new(v))))
        },
        Err(e) if matches!(e, ConvertError::NoValue) => {
            Ok(CHValue::Nullable(Either::Left((*target_type).clone().into())))
        },
        Err(e) => {
            Err(e)
        }
    }
}

fn into_array(v: Option<&Value>, target_type: &SqlType) -> CResult {
    if v.is_none() {
        return Err(ConvertError::NoValue);
    }
    let inner = v.unwrap();
    match inner {
        Value::Array(arr) => {
            let mut w = Vec::with_capacity(arr.len());
            for ev in arr {
                w.push(into_clickhouse_value(Some(ev), target_type)?);
            }
            Ok(CHValue::Array((*target_type).clone().into(), Arc::new(w)))
        },
        _ => Err(ConvertError::TypeMisMatch{from: inner.clone(), to: target_type.to_string().into_owned()})
    }
}

// only support Map(String, xxx)
fn into_map(v: Option<&Value>, target_type: &SqlType) -> CResult {
    if v.is_none() {
        return Err(ConvertError::NoValue);
    }
    let inner = v.unwrap();
    match inner {
        Value::Object(bt) => {
            let mut hm = HashMap::with_capacity(bt.len());
            for (k,v) in bt {
                hm.insert(CHValue::String(Arc::new(k.clone().into_bytes())), into_clickhouse_value(Some(v), target_type)?);
            }
            Ok(CHValue::Map(SqlType::String.into(), (*target_type).clone().into(), Arc::new(hm)))
        },
        _ => Err(ConvertError::TypeMisMatch{from: inner.clone(), to: target_type.to_string().into_owned()})
    }
}

fn into_string(v: Option<&Value>) -> CResult {
    if v.is_none() {
        return Err(ConvertError::NoValue);
    }
    let inner = v.unwrap();
    match inner {
        Value::Bytes(bs) => {
            Ok(CHValue::String(Arc::new(bs.to_vec())))
        },
        _ => Err(ConvertError::TypeMisMatch{from: inner.clone(), to: "string".to_string()})
    }
}

fn into_fixed_string(v: Option<&Value>, len: usize) -> CResult {
    if v.is_none() {
        return Err(ConvertError::NoValue);
    }
    let inner = v.unwrap();
    match inner {
        Value::Bytes(bs) => {
            let mut w = bs.to_vec();
            w.truncate(len);
            Ok(CHValue::String(Arc::new(w)))
        },
        _ => Err(ConvertError::TypeMisMatch{from: inner.clone(), to: format!("fixedstring{}", len).to_string()})
    }
}

const TIME_FORMAT: &str = "%d/%m/%Y %H:%M:%S%.9f%z";

fn into_date(v: Option<&Value>) -> CResult {
    if v.is_none() {
        return Err(ConvertError::NoValue);
    }
    let inner = v.unwrap();
    match inner {
        Value::Timestamp(ts) => {
            let s = ts.format(TIME_FORMAT).to_string();
            let t = Tz::UTC
                .datetime_from_str(s.as_str(), TIME_FORMAT)
                .context(InvalidTimeFormatSnafu{val: s, format: TIME_FORMAT})?
                .date();
            Ok(t.into())
        },
        Value::Integer(ts_nano) => {
            Ok(Tz::UTC.timestamp_nanos(*ts_nano).date().into())
        }
        _ => Err(ConvertError::TypeMisMatch{from: inner.clone(), to: "date".to_string()})
    }
}

fn into_datetime(v: Option<&Value>) -> CResult {
    if v.is_none() {
        return Err(ConvertError::NoValue.into());
    }
    let inner = v.unwrap();
    match inner {
        Value::Timestamp(ts) => {
            Ok((*ts).into())
        },
        Value::Integer(ts_nano) => {
            Ok(Tz::UTC.timestamp_nanos(*ts_nano).into())
        }
        _ => Err(ConvertError::TypeMisMatch{from: inner.clone(), to: "datetime".to_string()})
    }
}