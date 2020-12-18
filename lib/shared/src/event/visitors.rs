use crate::event::*;
use bytes::Bytes;
use serde::de::{MapAccess, SeqAccess, Visitor};
use std::{
    collections::BTreeMap,
    fmt::{self},
};

pub struct LogEventVisitor;

impl<'de> Visitor<'de> for LogEventVisitor {
    type Value = LogEvent;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a valid Log Event")
    }

    fn visit_map<A>(self, mut data: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut map = BTreeMap::default();

        // While there are entries remaining in the input, add them
        // into our map.
        while let Some((key, value)) = data.next_entry()? {
            map.insert(key, value);
        }

        Ok(LogEvent::from(map))
    }
}

pub struct ValueVisitor;

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a valid Value")
    }

    fn visit_bool<E>(self, data: bool) -> Result<Self::Value, E> {
        Ok(Value::from(data))
    }

    fn visit_i64<E>(self, data: i64) -> Result<Self::Value, E> {
        Ok(Value::from(data))
    }

    fn visit_u64<E>(self, data: u64) -> Result<Self::Value, E> {
        // Our current data model doesn't allow us to handle this without restricting our
        // possible values. :(
        Ok(Value::from(data as i64))
    }

    fn visit_f32<E>(self, data: f32) -> Result<Self::Value, E> {
        Ok(Value::from(data))
    }

    fn visit_f64<E>(self, data: f64) -> Result<Self::Value, E> {
        Ok(Value::from(data))
    }

    fn visit_str<E>(self, data: &str) -> Result<Self::Value, E> {
        Ok(Value::from(data.to_string()))
    }

    fn visit_string<E>(self, data: String) -> Result<Self::Value, E> {
        Ok(Value::from(data))
    }

    fn visit_bytes<E>(self, data: &[u8]) -> Result<Self::Value, E> {
        Ok(Value::from(Bytes::from(data.to_vec())))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A>(self, mut data: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut vec = Vec::with_capacity(data.size_hint().unwrap_or(0));

        // While there are entries remaining in the input, add them
        // into our map.
        while let Some(value) = data.next_element::<Value>()? {
            vec.push(value);
        }

        Ok(Value::from(vec))
    }

    fn visit_map<A>(self, mut data: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut map = BTreeMap::default();

        // While there are entries remaining in the input, add them
        // into our map.
        while let Some((key, value)) = data.next_entry()? {
            map.insert(key, value);
        }

        Ok(Value::from(map))
    }
}
