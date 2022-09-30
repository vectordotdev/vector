#![allow(unused_imports)]
use std::{sync::Arc, io::Read, net::{Ipv4Addr, Ipv6Addr}};
use chrono::{TimeZone, Date, Utc, DateTime};
use chrono_tz::Tz;
use thiserror::Error;
use bytes::{BufMut, Buf};
use futures::{FutureExt, SinkExt, stream::BoxStream, StreamExt, Bo};
use clickhouse_rs::{
    Block, Pool, 
    types::{Value as CHValue, SqlType, DateTimeType, Decimal, Enum8, Enum16},
};
use uuid::Uuid;
use vector_core::{
    stream::{BatcherSettings},
};
use async_trait::async_trait;
use value::Value;


use super::{ClickhouseConfig};
use crate::{
    config::SinkContext,
    event::{Event, LogEvent},
    sinks::{
        util::{
            retries::{RetryAction, RetryLogic},
            Buffer, TowerRequestConfig, StreamSink, SinkBuilderExt,
        },
        Healthcheck, HealthcheckError, UriParseSnafu, VectorSink,
    },
    tls::TlsSettings,
};

async fn build_native_sink(cfg: &ClickhouseConfig, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
    let batch = cfg.batch.into_batch_settings()?;
    let tls_settings = TlsSettings::from_options(&cfg.tls)?;
    let pool = Pool::new("");
    unimplemented!()
}

async fn healthcheck(pool: Pool) -> crate::Result<()> {
    let mut client = pool.get_handle().await?;
    client.ping().await.map_err(|e| e.into())
}

struct NativeClickhouseSink {
    pool: Pool,
    batch: BatcherSettings,
    columns: Vec<String>,
    table_schema: TableSchema,
}

struct TableSchema {
    schema: Vec<(String, SqlType)>
}

impl NativeClickhouseSink {
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input.map(|e| e.into_log())
            .batched(self.batch.into_byte_size_config())
            .into_driver(unimplemented!())
            .run().await
    }
    fn build_block(&self, events: Vec<LogEvent>) -> crate::Result<Block> {
        let mut b = Block::new();
        for ev in &events {
            b.push(get_column_from_events(&self.table_schema.schema, ev)?);
        }
        Ok(b)
    }
}

fn get_column_from_events(schema: &Vec<(String, SqlType)>, event: &LogEvent) -> crate::Result<Vec<(String, CHValue)>> {
    let mut row = vec![];
    for (col_name, ty) in schema {
        let event_field = event.get(col_name.as_str());
        row.push((col_name.clone(), into_clickhouse_value(v, ty, true)?))
    } 
    Ok(row)
}

#[async_trait]
impl StreamSink<Event> for NativeClickhouseSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}



#[derive(Error, Debug)]
pub enum ConvertError {
    #[error("cannot get column data {0}")]
    LackOfColumn(String),
    #[error("cannot convert {0} to {1}")]
    TypeMisMatch(Value, String),
    #[error("enum mismatch")]
    EnumMismatch,
    #[error("only nullable type can have no value")]
    NoValue,
}

macro_rules! gen_into_numeric {
    ($method:ident, $ty:ty, $vtype:ident, $chtype:ident) => {
        fn $method(v: Option<&Value>, nullable: bool) -> crate::Result<CHValue> {
            if v.is_none() {
                if nullable {
                    let v: Option<$ty> = None;
                    return Ok(v.into());
                }
                return Err(ConvertError::NoValue.into());
            }
            let inner = v.unwrap();
            match inner {
                Value::$vtype(val) => Ok(CHValue::$chtype(*val as $ty)),
                _ => Err(ConvertError::TypeMisMatch(inner.clone(), stringify!($ty).to_string()).into())
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

macro_rules! gen_into_enum {
    ($method:ident, $ty:ty, $chtype:ident) => {
        fn $method(v: Option<&Value>, def: &Vec<(String,$ty)>) -> crate::Result<CHValue> {
            if v.is_none() {
                return Err(ConvertError::NoValue.into());
            }
            let inner = v.unwrap();
            match inner {
                Value::Bytes(bs) => {
                    match def.iter().find(|(k,v)| bs.eq(k)) {
                        Some((_, val)) => Ok(CHValue::$chtype(def.to_vec(), $chtype::of(*val))),
                        _ => Err(ConvertError::EnumMismatch.into())
                    }
                },
                _ => Err(ConvertError::TypeMisMatch(inner.clone(), stringify!($ty).to_string()).into())
            }
        }
    }
}

gen_into_enum!(into_enum8, i8, Enum8);
gen_into_enum!(into_enum16, i16, Enum16);


fn into_float(v: Option<&Value>, to_f32: bool) -> crate::Result<CHValue> {
    if v.is_none() {
        return Err(ConvertError::NoValue.into());
    }
    let inner = v.unwrap();
    match inner {
        Value::Float(v) => {
            if to_f32 {
                Ok(CHValue::Float32(v.as_f32().into()))
            } else {
                Ok(CHValue::Float64(v.into_inner()))
            }
        },
        _ => {
            let target_type = if to_f32 {"f32"} else {"f64"};
            Err(ConvertError::TypeMisMatch(inner.clone(), target_type.to_string()).into())
        }
    }   1
}

macro_rules! gen_into_ip {
    ($method:ident, $ty:ident, $chtype:ident) => {
        fn $method(v: Option<&Value>, nullable: bool) -> crate::Result<CHValue> {
            if v.is_none() {
                if nullable {
                    return CHValue::Nullable()
                }
                return Err(ConvertError::NoValue.into());
            }
            let inner = v.unwrap();
            match inner {
                Value::Bytes(bs) => {
                    let addr: $ty = std::str::from_utf8(&bs.to_vec())?.parse()?;
                    Ok(CHValue::$chtype(addr.octets()))
                },
                _ => Err(ConvertError::TypeMisMatch(inner.clone(), stringify!($ty).to_string()).into())
            }
        }
    }
}

gen_into_ip!(into_ipv4, Ipv4Addr, Ipv4);
gen_into_ip!(into_ipv6, Ipv6Addr, Ipv6);

fn into_clickhouse_value(v: Option<&Value>, target_type: &SqlType, nullable: bool) -> crate::Result<CHValue> {
    match target_type {
        SqlType::UInt8 => into_u8(v, nullable),
        SqlType::UInt16 => into_u16(v, nullable),
        SqlType::UInt32 => into_u32(v, nullable),
        SqlType::UInt64 => into_u64(v, nullable),
        SqlType::Int8 => into_i8(v, nullable),
        SqlType::Int16 => into_i16(v, nullable),
        SqlType::Int32 => into_i32(v, nullable),
        SqlType::Int64 => into_i64(v, nullable),
        SqlType::String => into_string(v, nullable),
        SqlType::FixedString(len) => into_fixed_string(v, *len, nullable),
        SqlType::Float32 => into_float(v, true),
        SqlType::Float64 => into_float(v, false),
        SqlType::Date => into_date(v, nullable),
        SqlType::DateTime(ty) => {
            match ty {
                DateTimeType::DateTime32 => into_datetime(v, nullable),
                _ => unimplemented!()
            }
        },
        SqlType::Ipv4 => into_ipv4(v, nullable),
        SqlType::Ipv6 => into_ipv6(v, nullable),
        SqlType::Uuid => into_uuid(v),
        SqlType::Nullable(ty) => into_nullable(v, *ty),
        SqlType::Array(ty) => into_array(v, ty),
        SqlType::Decimal(_, scale) => into_decimal(v, *scale),
        SqlType::Enum8(def) => into_enum8(v, def),
        SqlType::Enum16(def) => into_enum16(v, def),
        SqlType::Map(k, v) => unimplemented!(),
        SqlType::SimpleAggregateFunction(_, _) => unimplemented!(),
    }
}

fn into_nullable(v: Option<&Value>, target_type: &SqlType, nullable: bool) -> crate::Result<CHValue> {
    let inner = into_clickhouse_value(v, target_type, nullable)?;
    todo!()
}

fn into_array(v: Option<&Value>, target_type: &SqlType) -> crate::Result<CHValue> {
    if v.is_none() {
        return Err(ConvertError::NoValue.into());
    }
    let inner = v.unwrap();
    match inner {
        Value::Array(arr) => {
            let mut w = Vec::with_capacity(arr.len());
            for ev in arr {
                w.push(into_clickhouse_value(Some(ev), target_type)?);
            }
            Ok(CHValue::Array(target_type, Arc::new(w)))
        },
        _ => Err(ConvertError::TypeMisMatch(inner.clone(), target_type.to_string().to_string()).into())
    }
}



fn into_string(v: Option<&Value>, nullable: bool) -> crate::Result<CHValue> {
    if v.is_none() {
        if nullable {
            let v: Option<String> = None;
            return Ok(v.into());
        }
        return Err(ConvertError::NoValue.into());
    }
    let inner = v.unwrap();
    match inner {
        Value::Bytes(bs) => {
            Ok(CHValue::String(Arc::new(bs.to_vec())))
        },
        _ => Err(ConvertError::TypeMisMatch(inner.clone(), "string".into()).into())
    }
}

fn into_fixed_string(v: Option<&Value>, len: usize, nullable: bool) -> crate::Result<CHValue> {
    if v.is_none() {
        if nullable {
            let v: Option<String> = None;
            return Ok(v.into());
        }
        return Err(ConvertError::NoValue.into());
    }
    let inner = v.unwrap();
    match inner {
        Value::Bytes(bs) => {
            let w = bs.to_vec();
            w.truncate(len);
            Ok(CHValue::String(Arc::new(w)))
        },
        _ => Err(ConvertError::TypeMisMatch(inner.clone(), format!("fixedstring({})", len)).into())
    }
}

fn into_decimal(v: Option<&Value>, scale: u8) -> crate::Result<CHValue> {
    if v.is_none() {
        return Err(ConvertError::NoValue.into());
    }
    let inner = v.unwrap();
    match inner {
        Value::Float(v) => {
            Ok(CHValue::Decimal(Decimal::of(v.into_inner(), scale)))
        },
        _ => Err(ConvertError::TypeMisMatch(inner.clone(), format!("decimal({})", scale)).into())
    }
}

const TIME_FORMAT: &str = "%d/%m/%Y %H:%M:%S%.9f%z";

fn into_date(v: Option<&Value>, nullable: bool) -> crate::Result<CHValue> {
    if v.is_none() {
        if nullable {
            let w: Option<Date<Tz>> = None;
            return Ok(w.into());
        }
        return Err(ConvertError::NoValue.into());
    }
    let inner = v.unwrap();
    match inner {
        Value::Timestamp(ts) => {
            let s = ts.format(TIME_FORMAT).to_string();
            let t = Tz::UTC.datetime_from_str(s.as_str(), TIME_FORMAT)?.date();
            Ok(t.into())
        },
        Value::Integer(ts_nano) => {
            Ok(Tz::UTC.timestamp_nanos(*ts_nano).date().into())
        }
        _ => Err(ConvertError::TypeMisMatch(inner.clone(), "date".into()).into())
    }
}

fn into_datetime(v: Option<&Value>, nullable: bool) -> crate::Result<CHValue> {
    if v.is_none() {
        if nullable {
            let w: Option<DateTime<Utc>> = None;
            return Ok(w.into());
        }
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
        _ => Err(ConvertError::TypeMisMatch(inner.clone(), "datetime".into()).into())
    }
}

fn into_uuid(v: Option<&Value>) -> crate::Result<CHValue> {
    if v.is_none() {
        return Err(ConvertError::NoValue.into());
    }
    let inner = v.unwrap();
    match inner {
        Value::Bytes(bs) => {
            let u = Uuid::parse_str(std::str::from_utf8(&bs.to_vec())?)?;
            Ok(u.into())
        },
        _ => Err(ConvertError::TypeMisMatch(inner.clone(), "uuid".into()).into())
    }
}