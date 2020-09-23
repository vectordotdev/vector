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
use nom::lib::std::fmt::Formatter;
pub use segment::Segment;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::Display;
use std::ops::{Index, IndexMut};

/// Lookups are precomputed event lookup paths.
///
/// They are intended to handle user input from a configuration.
///
/// Generally, these shouldn't be created on the hot path.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct Lookup {
    segments: Vec<Segment>,
}

impl<'a> TryFrom<Pair<'a, Rule>> for Lookup {
    type Error = crate::Error;

    fn try_from(pair: Pair<'a, Rule>) -> Result<Self, Self::Error> {
        Ok(Self {
            segments: Segment::from_lookup(pair)?,
        })
    }
}

impl Display for Lookup {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let mut peeker = self.segments.iter().peekable();
        let mut next = peeker.next();
        let mut maybe_next = peeker.peek();
        while let Some(segment) = next {
            match segment {
                Segment::Field(_) => {
                    match maybe_next {
                        Some(next) if next.is_field() => write!(f, r#"{}."#, segment)?,
                        None | Some(_) => write!(f, "{}", segment)?,
                    };
                }
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
    pub fn iter(&self) -> Iter<'_, Segment> {
        self.segments.iter()
    }

    pub fn from_indexmap(
        values: IndexMap<String, TomlValue>,
    ) -> crate::Result<IndexMap<Lookup, Value>> {
        let mut discoveries = IndexMap::new();
        for (key, value) in values {
            Self::from_recursive_step(Lookup::try_from(key)?, value, &mut discoveries)?;
        }
        Ok(discoveries)
    }
    pub fn from_toml(value: TomlValue) -> crate::Result<IndexMap<Lookup, Value>> {
        let mut discoveries = IndexMap::new();
        match value {
            TomlValue::Table(map) => {
                for (key, value) in map {
                    Self::from_recursive_step(Lookup::try_from(key)?, value, &mut discoveries)?;
                }
                Ok(discoveries)
            }
            _ => Err(format!(
                "A TOML table must be passed to the `from_toml_table` function. Passed: {:?}",
                value
            ))?,
        }
    }

    fn from_recursive_step(
        lookup: Lookup,
        value: TomlValue,
        discoveries: &mut IndexMap<Lookup, Value>,
    ) -> crate::Result<()> {
        match value {
            TomlValue::String(s) => discoveries.insert(Lookup::from(lookup), Value::from(s)),
            TomlValue::Integer(i) => discoveries.insert(Lookup::from(lookup), Value::from(i)),
            TomlValue::Float(f) => discoveries.insert(Lookup::from(lookup), Value::from(f)),
            TomlValue::Boolean(b) => discoveries.insert(Lookup::from(lookup), Value::from(b)),
            TomlValue::Datetime(dt) => {
                let dt = dt.to_string();
                discoveries.insert(Lookup::try_from(lookup)?, Value::from(dt))
            }
            TomlValue::Array(vals) => {
                for (i, val) in vals.into_iter().enumerate() {
                    let key = format!("{}[{}]", lookup, i);
                    Self::from_recursive_step(Lookup::try_from(key)?, val, discoveries)?;
                }
                None
            }
            TomlValue::Table(map) => {
                for (table_key, value) in map {
                    let key = format!("{}.{}", lookup, table_key);
                    Self::from_recursive_step(Lookup::try_from(key)?, value, discoveries)?;
                }
                None
            }
        };
        Ok(())
    }
}

impl IntoIterator for Lookup {
    type Item = Segment;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.segments.into_iter()
    }
}

impl TryFrom<String> for Lookup {
    type Error = crate::Error;

    fn try_from(input: String) -> Result<Self, Self::Error> {
        let mut pairs = MappingParser::parse(Rule::lookup, &input)?;
        let pair = pairs.next().ok_or("No tokens found.")?;
        Self::try_from(pair)
    }
}

impl TryFrom<&str> for Lookup {
    type Error = crate::Error;

    fn try_from(input: &str) -> Result<Self, Self::Error> {
        let mut pairs = MappingParser::parse(Rule::lookup, input)?;
        let pair = pairs.next().ok_or("No tokens found.")?;
        Self::try_from(pair)
    }
}

impl TryFrom<string_cache::DefaultAtom> for Lookup {
    type Error = crate::Error;

    fn try_from(input: string_cache::DefaultAtom) -> Result<Self, Self::Error> {
        let mut pairs = MappingParser::parse(Rule::lookup, &input)?;
        let pair = pairs.next().ok_or("No tokens found.")?;
        Self::try_from(pair)
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
        Lookup::try_from(value.to_owned()).map_err(de::Error::custom)
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Lookup::try_from(value).map_err(de::Error::custom)
    }
}
