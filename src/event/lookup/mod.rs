mod segment;
#[cfg(test)]
mod test;

use crate::{
    event::Value,
    mapping::parser::{MappingParser, Rule},
};
use pest::{iterators::Pair, Parser};
use std::{
    convert::TryFrom,
    ops::{RangeFrom, RangeFull, RangeTo, RangeToInclusive},
    slice::Iter,
    str,
};
use toml::Value as TomlValue;

use core::fmt;
use indexmap::map::IndexMap;
pub use segment::Segment;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{Display, Formatter};
use std::ops::{Index, IndexMut};
use std::str::FromStr;

/// Lookups are pre-validated event lookup paths.
///
/// These are owned, ordered sets of segments. Segments represent parts of a path such as
/// `pies.banana.slices[0]`. The segments would be `["pies", "banana", "slices", 0]`. You can "walk"
/// a lookup with an `iter()` call.
///
/// # Building
///
/// You build `Lookup`s from `String`s and other string-like objects with a `from()` or `try_from()`
/// call. **These do not parse the buffer.**
///
/// From there, you can `push` and `pop` onto the `Lookup`.
///
/// # Parsing
///
/// To parse buffer into a `Lookup`, use the `std::str::FromStr` implementation. If you're working
/// something that's not able to be a `str`, you should consult `std::str::from_utf8` and handle the
/// possible error.
///
// # TODO
//
// It would be very useful to add an unowned variant and have this structure use it in a
// self-referencing manner. This would allow for unowned Lookups which would enable more API
// flexibility. Consider `Path`/`PathBuf` or `String`/`Str`.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct Lookup {
    segments: Vec<Segment>,
}

impl<'a> TryFrom<Pair<'a, Rule>> for Lookup {
    type Error = crate::Error;

    fn try_from(pair: Pair<'a, Rule>) -> Result<Self, Self::Error> {
        let retval = Self {
            segments: Segment::from_lookup(pair)?,
        };
        retval.is_valid()?;
        Ok(retval)
    }
}

impl Display for Lookup {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let mut peeker = self.segments.iter().peekable();
        let mut next = peeker.next();
        let mut maybe_next = peeker.peek();
        while let Some(segment) = next {
            match segment {
                Segment::Field(_) => match maybe_next {
                    Some(next) if next.is_field() => write!(f, r#"{}."#, segment)?,
                    None | Some(_) => write!(f, "{}", segment)?,
                },
                Segment::Index(_) => match maybe_next {
                    Some(next) if next.is_field() => write!(f, r#"[{}]."#, segment)?,
                    None | Some(_) => write!(f, "[{}]", segment)?,
                },
            }
            next = peeker.next();
            maybe_next = peeker.peek();
        }
        Ok(())
    }
}

impl Lookup {
    /// Push onto the internal list of segments.
    #[instrument(level = "trace")]
    pub fn push(&mut self, segment: Segment) {
        trace!(length = %self.segments.len(), "Pushing.");
        self.segments.push(segment)
    }

    #[instrument(level = "trace")]
    pub fn pop(&mut self) -> Option<Segment> {
        trace!(length = %self.segments.len(), "Popping.");
        self.segments.pop()
    }

    #[instrument(level = "trace")]
    pub fn iter(&self) -> Iter<'_, Segment> {
        self.segments.iter()
    }

    #[instrument(level = "trace")]
    pub fn from_indexmap(
        values: IndexMap<String, TomlValue>,
    ) -> crate::Result<IndexMap<Lookup, Value>> {
        let mut discoveries = IndexMap::new();
        for (key, value) in values {
            Self::recursive_step(Lookup::try_from(key)?, value, &mut discoveries)?;
        }
        Ok(discoveries)
    }

    #[instrument(level = "trace")]
    pub fn from_toml_table(value: TomlValue) -> crate::Result<IndexMap<Lookup, Value>> {
        let mut discoveries = IndexMap::new();
        match value {
            TomlValue::Table(map) => {
                for (key, value) in map {
                    Self::recursive_step(Lookup::try_from(key)?, value, &mut discoveries)?;
                }
                Ok(discoveries)
            }
            _ => Err(format!(
                "A TOML table must be passed to the `from_toml_table` function. Passed: {:?}",
                value
            )
            .into()),
        }
    }

    #[instrument(level = "trace")]
    fn recursive_step(
        lookup: Lookup,
        value: TomlValue,
        discoveries: &mut IndexMap<Lookup, Value>,
    ) -> crate::Result<()> {
        match value {
            TomlValue::String(s) => discoveries.insert(lookup, Value::from(s)),
            TomlValue::Integer(i) => discoveries.insert(lookup, Value::from(i)),
            TomlValue::Float(f) => discoveries.insert(lookup, Value::from(f)),
            TomlValue::Boolean(b) => discoveries.insert(lookup, Value::from(b)),
            TomlValue::Datetime(dt) => {
                let dt = dt.to_string();
                discoveries.insert(lookup, Value::from(dt))
            }
            TomlValue::Array(vals) => {
                for (i, val) in vals.into_iter().enumerate() {
                    let key = format!("{}[{}]", lookup, i);
                    Self::recursive_step(Lookup::try_from(key)?, val, discoveries)?;
                }
                None
            }
            TomlValue::Table(map) => {
                for (table_key, value) in map {
                    let key = format!("{}.{}", lookup, table_key);
                    Self::recursive_step(Lookup::try_from(key)?, value, discoveries)?;
                }
                None
            }
        };
        Ok(())
    }

    /// Raise any errors that might stem from the lookup being invalid.
    #[instrument(level = "trace")]
    fn is_valid(&self) -> crate::Result<()> {
        if self.segments.is_empty() {
            return Err("Lookups must have at least 1 segment to be valid.".into());
        }

        Ok(())
    }
}

impl FromStr for Lookup {
    type Err = crate::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let mut pairs = MappingParser::parse(Rule::lookup, input)?;
        let pair = pairs.next().ok_or("No tokens found.")?;
        Self::try_from(pair)
    }
}

impl IntoIterator for Lookup {
    type Item = Segment;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.segments.into_iter()
    }
}

impl From<String> for Lookup {
    fn from(input: String) -> Self {
        Self {
            segments: vec![Segment::field(input)],
        }
        // We know this must be at least one segment.
    }
}

impl From<&str> for Lookup {
    fn from(input: &str) -> Self {
        Self {
            segments: vec![Segment::field(input.to_owned())],
        }
        // We know this must be at least one segment.
    }
}

impl Index<usize> for Lookup {
    type Output = Segment;

    fn index(&self, index: usize) -> &Self::Output {
        self.segments.index(index)
    }
}

impl Index<RangeFull> for Lookup {
    type Output = [Segment];

    fn index(&self, index: RangeFull) -> &Self::Output {
        self.segments.index(index)
    }
}

impl Index<RangeToInclusive<usize>> for Lookup {
    type Output = [Segment];

    fn index(&self, index: RangeToInclusive<usize>) -> &Self::Output {
        self.segments.index(index)
    }
}

impl Index<RangeTo<usize>> for Lookup {
    type Output = [Segment];

    fn index(&self, index: RangeTo<usize>) -> &Self::Output {
        self.segments.index(index)
    }
}

impl Index<RangeFrom<usize>> for Lookup {
    type Output = [Segment];

    fn index(&self, index: RangeFrom<usize>) -> &Self::Output {
        self.segments.index(index)
    }
}

impl IndexMut<usize> for Lookup {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.segments.index_mut(index)
    }
}

impl Serialize for Lookup {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&*self.to_string())
    }
}

impl<'de> Deserialize<'de> for Lookup {
    fn deserialize<D>(deserializer: D) -> Result<Lookup, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_string(LookupVisitor)
    }
}

struct LookupVisitor;

impl<'de> Visitor<'de> for LookupVisitor {
    type Value = Lookup;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("Expected valid Lookup path.")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Lookup::from_str(value).map_err(de::Error::custom)
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Lookup::from_str(&value).map_err(de::Error::custom)
    }
}
