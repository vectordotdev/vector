pub mod log;

pub(self) use super::{Value, LogEvent};
use serde::de::{Visitor, MapAccess};
use std::{fmt::{self}};
use std::collections::HashMap;


pub struct LogEventVisitor;

impl<'de> Visitor<'de> for LogEventVisitor {
    type Value = LogEvent;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a valid Log")
    }

    fn visit_map<A>(self, mut data: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
    {
        let mut map = HashMap::with_capacity(data.size_hint().unwrap_or(0));

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
        formatter.write_str("a valid Log")
    }

    fn visit_map<A>(self, mut data: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
    {
        let mut map = HashMap::with_capacity(data.size_hint().unwrap_or(0));

        // While there are entries remaining in the input, add them
        // into our map.
        while let Some((key, value)) = data.next_entry()? {
            map.insert(key, value);
        }

        Ok(Value::from(map))
    }
}
