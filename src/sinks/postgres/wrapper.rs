use std::fmt::Debug;

use crate::sinks::prelude::*;
use bytes::{Buf, BufMut};
use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio_postgres::types::{accepts, to_sql_checked, IsNull, ToSql};

#[derive(Debug)]
pub(crate) struct Wrapper<'a>(pub &'a Value);

impl ToSql for Wrapper<'_> {
    fn to_sql(
        &self,
        ty: &tokio_postgres::types::Type,
        out: &mut bytes::BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>>
    where
        Self: Sized,
    {
        match self.0 {
            Value::Bytes(bytes) => bytes.chunk().to_sql(ty, out),
            Value::Regex(value_regex) => value_regex.as_str().to_sql(ty, out),
            Value::Integer(i) => i.to_sql(ty, out),
            Value::Float(not_nan) => not_nan.to_sql(ty, out),
            Value::Boolean(b) => b.to_sql(ty, out),
            Value::Timestamp(date_time) => date_time.to_sql(ty, out),
            Value::Object(btree_map) => {
                serde_json::to_writer(out.writer(), btree_map)?;
                Ok(IsNull::No)
            }
            Value::Array(values) => {
                // Taken from postgres-types/lib.rs `impl<T: ToSql> ToSql for &[T]`
                //
                // There is no function that serializes an iterator, only a method on slices,
                // but we should not have to allocate a new `Vec<Wrapper<&Value>>` just to
                // serialize the `Vec<Value>` we already have

                let member_type = match *ty.kind() {
                    tokio_postgres::types::Kind::Array(ref member) => member,
                    _ => {
                        return Err(Box::new(
                            tokio_postgres::types::WrongType::new::<Vec<Value>>(ty.clone()),
                        ))
                    }
                };

                // Arrays are normally one indexed by default but oidvector and int2vector *require* zero indexing
                let lower_bound = match *ty {
                    tokio_postgres::types::Type::OID_VECTOR
                    | tokio_postgres::types::Type::INT2_VECTOR => 0,
                    _ => 1,
                };

                let dimension = postgres_protocol::types::ArrayDimension {
                    len: values.len().try_into()?,
                    lower_bound,
                };

                postgres_protocol::types::array_to_sql(
                    Some(dimension),
                    member_type.oid(),
                    values.iter().map(Wrapper),
                    |e, w| match e.to_sql(member_type, w)? {
                        IsNull::No => Ok(postgres_protocol::IsNull::No),
                        IsNull::Yes => Ok(postgres_protocol::IsNull::Yes),
                    },
                    out,
                )?;
                Ok(IsNull::No)
            }
            Value::Null => Ok(IsNull::Yes),
        }
    }

    fn accepts(ty: &tokio_postgres::types::Type) -> bool
    where
        Self: Sized,
    {
        <&[u8]>::accepts(ty)
            || <&str>::accepts(ty)
            || i64::accepts(ty)
            || f64::accepts(ty)
            || bool::accepts(ty)
            || DateTime::<Utc>::accepts(ty)
            || serde_json::Value::accepts(ty)
            || Option::<u32>::accepts(ty)
            || match *ty.kind() {
                tokio_postgres::types::Kind::Array(ref member) => Self::accepts(member),
                _ => false,
            }
    }

    to_sql_checked!();
}

/// Allows for zero-copy SQL conversion for any struct that is
/// Serializable into a JSON object
#[derive(Debug)]
pub(crate) struct JsonObjWrapper<Inner>(pub Inner);

impl<Inner: Serialize + Debug> ToSql for JsonObjWrapper<Inner> {
    fn to_sql(
        &self,
        _: &tokio_postgres::types::Type,
        out: &mut bytes::BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>>
    where
        Self: Sized,
    {
        serde_json::to_writer(out.writer(), &self.0)?;
        Ok(IsNull::No)
    }

    accepts!(JSON, JSONB);

    to_sql_checked!();
}
