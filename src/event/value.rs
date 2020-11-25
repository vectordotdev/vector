use crate::event::Segment;
use crate::{
    event::{timestamp_to_string, Lookup},
    Result,
};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use derive_is_enum_variant::is_enum_variant;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, HashMap};
use std::convert::{TryFrom, TryInto};
use std::iter::FromIterator;
use toml::value::Value as TomlValue;

#[derive(PartialEq, Debug, Clone, is_enum_variant)]
pub enum Value {
    Bytes(Bytes),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Timestamp(DateTime<Utc>),
    Map(BTreeMap<String, Value>),
    Array(Vec<Value>),
    Null,
}

impl Value {
    #[instrument(level = "trace")]
    pub fn as_integer(&self) -> &i64 {
        match self {
            Value::Integer(ref i) => i,
            _ => panic!("Tried to call `Value::as_integer` on a non-integer value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_integer_mut(&mut self) -> &mut i64 {
        match self {
            Value::Integer(ref mut i) => i,
            _ => panic!("Tried to call `Value::as_integer` on a non-integer value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_float(&self) -> &f64 {
        match self {
            Value::Float(ref f) => f,
            _ => panic!("Tried to call `Value::as_float` on a non-float value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_float_mut(&mut self) -> &mut f64 {
        match self {
            Value::Float(ref mut f) => f,
            _ => panic!("Tried to call `Value::as_float` on a non-float value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_bool(&self) -> &bool {
        match self {
            Value::Boolean(ref b) => b,
            _ => panic!("Tried to call `Value::as_bool` on a non-bool value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_bool_mut(&mut self) -> &mut bool {
        match self {
            Value::Boolean(ref mut b) => b,
            _ => panic!("Tried to call `Value::as_bool` on a non-bool value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_timestamp(&self) -> &DateTime<Utc> {
        match self {
            Value::Timestamp(ref t) => t,
            _ => panic!("Tried to call `Value::as_timestamp` on a non-timestamp value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_timestamp_mut(&mut self) -> &mut DateTime<Utc> {
        match self {
            Value::Timestamp(ref mut t) => t,
            _ => panic!("Tried to call `Value::as_timestamp` on a non-timestamp value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_bytes(&self) -> &Bytes {
        match self {
            Value::Bytes(ref b) => b,
            _ => panic!("Tried to call `Value::as_bytes` on a non-bytes value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_bytes_mut(&mut self) -> &mut Bytes {
        match self {
            Value::Bytes(ref mut b) => b,
            _ => panic!("Tried to call `Value::as_bytes` on a non-bytes value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_map(&self) -> &BTreeMap<String, Value> {
        match self {
            Value::Map(ref m) => m,
            _ => panic!("Tried to call `Value::as_map` on a non-map value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_map_mut(&mut self) -> &mut BTreeMap<String, Value> {
        match self {
            Value::Map(ref mut m) => m,
            _ => panic!("Tried to call `Value::as_map` on a non-map value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_array(&self) -> &Vec<Value> {
        match self {
            Value::Array(ref a) => a,
            _ => panic!("Tried to call `Value::as_array` on a non-array value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_array_mut(&mut self) -> &mut Vec<Value> {
        match self {
            Value::Array(ref mut a) => a,
            _ => panic!("Tried to call `Value::as_array` on a non-array value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn lookups<'a>(
        &'a self,
        prefix: Option<Lookup<'a>>,
    ) -> Box<dyn Iterator<Item = Lookup<'a>> + 'a> {
        match &self {
            Value::Boolean(_)
            | Value::Bytes(_)
            | Value::Timestamp(_)
            | Value::Float(_)
            | Value::Integer(_)
            | Value::Null => Box::new(Vec::<Lookup<'_>>::with_capacity(0).into_iter()),
            Value::Map(m) => Box::new(
                m.iter()
                    .map(move |(k, v)| {
                        let lookup = prefix.clone().map_or_else(
                            || Lookup::from(k),
                            |mut l| {
                                l.push(Segment::field(k));
                                l
                            },
                        );
                        let iter = Some(lookup.clone()).into_iter();
                        let chain = v.lookups(Some(lookup));
                        iter.chain(chain)
                    })
                    .flatten(),
            ),
            Value::Array(a) => Box::new(
                a.iter()
                    .enumerate()
                    .map(move |(k, v)| {
                        let lookup = prefix.clone().map_or_else(
                            || Lookup::from(k),
                            |mut l| {
                                l.push(Segment::index(k));
                                l
                            },
                        );
                        let iter = Some(lookup.clone()).into_iter();
                        let chain = v.lookups(Some(lookup));
                        iter.chain(chain)
                    })
                    .flatten(),
            ),
        }
    }

    #[instrument(level = "trace")]
    pub fn pairs<'a>(
        &'a self,
        prefix: Option<Lookup<'a>>,
    ) -> Box<dyn Iterator<Item = (Lookup<'a>, &'a Value)> + 'a> {
        match &self {
            Value::Boolean(_)
            | Value::Bytes(_)
            | Value::Timestamp(_)
            | Value::Float(_)
            | Value::Integer(_)
            | Value::Null => Box::new(std::iter::empty()),
            Value::Map(m) => Box::new(
                m.iter()
                    .map(move |(k, v)| {
                        let lookup = prefix.clone().map_or_else(
                            || Lookup::from(k),
                            |mut l| {
                                l.push(Segment::field(k));
                                l
                            },
                        );
                        let iter = Some((lookup.clone(), v)).into_iter();
                        let chain = v.pairs(Some(lookup));
                        iter.chain(chain)
                    })
                    .flatten(),
            ),
            Value::Array(a) => Box::new(
                a.iter()
                    .enumerate()
                    .map(move |(k, v)| {
                        let lookup = prefix.clone().map_or_else(
                            || Lookup::from(k),
                            |mut l| {
                                l.push(Segment::index(k));
                                l
                            },
                        );
                        let iter = Some((lookup.clone(), v)).into_iter();
                        let chain = v.pairs(Some(lookup));
                        iter.chain(chain)
                    })
                    .flatten(),
            ),
        }
    }
}

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self {
            Value::Integer(i) => serializer.serialize_i64(*i),
            Value::Float(f) => serializer.serialize_f64(*f),
            Value::Boolean(b) => serializer.serialize_bool(*b),
            Value::Bytes(_) | Value::Timestamp(_) => {
                serializer.serialize_str(&self.to_string_lossy())
            }
            Value::Map(m) => serializer.collect_map(m),
            Value::Array(a) => serializer.collect_seq(a),
            Value::Null => serializer.serialize_none(),
        }
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(crate::event::util::ValueVisitor)
    }
}

impl From<Bytes> for Value {
    fn from(bytes: Bytes) -> Self {
        Value::Bytes(bytes)
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(set: Vec<T>) -> Self {
        Value::from_iter(set.into_iter().map(|v| v.into()))
    }
}

impl From<String> for Value {
    fn from(string: String) -> Self {
        Value::Bytes(string.into())
    }
}

impl TryFrom<TomlValue> for Value {
    type Error = crate::Error;

    fn try_from(toml: TomlValue) -> crate::Result<Self> {
        Ok(match toml {
            TomlValue::String(s) => Self::from(s),
            TomlValue::Integer(i) => Self::from(i),
            TomlValue::Array(a) => Self::from(
                a.into_iter()
                    .map(Value::try_from)
                    .collect::<Result<Vec<_>>>()?,
            ),
            TomlValue::Table(t) => Self::from(
                t.into_iter()
                    .map(|(k, v)| Value::try_from(v).map(|v| (k, v)))
                    .collect::<Result<BTreeMap<_, _>>>()?,
            ),
            TomlValue::Datetime(dt) => Self::from(dt.to_string().parse::<DateTime<Utc>>()?),
            TomlValue::Boolean(b) => Self::from(b),
            TomlValue::Float(f) => Self::from(f),
        })
    }
}

// We only enable this in testing for convenience, since `"foo"` is a `&str`.
// In normal operation, it's better to let the caller decide where to clone and when, rather than
// hiding this from them.
#[cfg(test)]
impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::Bytes(Vec::from(s.as_bytes()).into())
    }
}

impl From<DateTime<Utc>> for Value {
    fn from(timestamp: DateTime<Utc>) -> Self {
        Value::Timestamp(timestamp)
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(value: Option<T>) -> Self {
        match value {
            None => Value::Null,
            Some(v) => v.into(),
        }
    }
}

impl From<f32> for Value {
    fn from(value: f32) -> Self {
        Value::Float(f64::from(value))
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Value::Float(value)
    }
}

impl From<BTreeMap<String, Value>> for Value {
    fn from(value: BTreeMap<String, Value>) -> Self {
        Value::Map(value)
    }
}

impl From<HashMap<String, Value>> for Value {
    fn from(value: HashMap<String, Value>) -> Self {
        Value::from_iter(value.into_iter())
    }
}

impl FromIterator<Value> for Value {
    fn from_iter<I: IntoIterator<Item = Value>>(iter: I) -> Self {
        Value::Array(iter.into_iter().collect::<Vec<Value>>())
    }
}

impl FromIterator<(String, Value)> for Value {
    fn from_iter<I: IntoIterator<Item = (String, Value)>>(iter: I) -> Self {
        Value::Map(iter.into_iter().collect::<BTreeMap<String, Value>>())
    }
}

macro_rules! impl_valuekind_from_integer {
    ($t:ty) => {
        impl From<$t> for Value {
            fn from(value: $t) -> Self {
                Value::Integer(value as i64)
            }
        }
    };
}

impl_valuekind_from_integer!(i64);
impl_valuekind_from_integer!(i32);
impl_valuekind_from_integer!(i16);
impl_valuekind_from_integer!(i8);
impl_valuekind_from_integer!(isize);

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Value::Boolean(value)
    }
}

impl From<serde_json::Value> for Value {
    fn from(json_value: serde_json::Value) -> Self {
        match json_value {
            serde_json::Value::Bool(b) => Value::Boolean(b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Value::Integer(i)
                } else if let Some(f) = n.as_f64() {
                    Value::Float(f)
                } else {
                    Value::Bytes(n.to_string().into())
                }
            }
            serde_json::Value::String(s) => Value::Bytes(Bytes::from(s)),
            serde_json::Value::Object(obj) => Value::Map(
                obj.into_iter()
                    .map(|(key, value)| (key, Value::from(value)))
                    .collect(),
            ),
            serde_json::Value::Array(arr) => {
                Value::Array(arr.into_iter().map(Value::from).collect())
            }
            serde_json::Value::Null => Value::Null,
        }
    }
}

impl TryInto<serde_json::Value> for Value {
    type Error = crate::Error;

    fn try_into(self) -> std::result::Result<serde_json::Value, Self::Error> {
        match self {
            Value::Boolean(v) => Ok(serde_json::Value::from(v)),
            Value::Integer(v) => Ok(serde_json::Value::from(v)),
            Value::Float(v) => Ok(serde_json::Value::from(v)),
            Value::Bytes(v) => Ok(serde_json::Value::from(String::from_utf8(v.to_vec())?)),
            Value::Map(v) => Ok(serde_json::to_value(v)?),
            Value::Array(v) => Ok(serde_json::to_value(v)?),
            Value::Null => Ok(serde_json::Value::Null),
            Value::Timestamp(v) => Ok(serde_json::Value::from(timestamp_to_string(&v))),
        }
    }
}

impl TryInto<bool> for Value {
    type Error = crate::Error;

    fn try_into(self) -> std::result::Result<bool, Self::Error> {
        match self {
            Value::Boolean(v) => Ok(v),
            _ => Err(
                "Tried to call `Value::try_into` to get a bool from a type that was not a bool."
                    .into(),
            ),
        }
    }
}

impl TryInto<Bytes> for Value {
    type Error = crate::Error;

    fn try_into(self) -> std::result::Result<Bytes, Self::Error> {
        match self {
            Value::Bytes(v) => Ok(v),
            _ => Err(
                "Tried to call `Value::try_into` to get a Bytes from a type that was not a Bytes."
                    .into(),
            ),
        }
    }
}

impl TryInto<f64> for Value {
    type Error = crate::Error;

    fn try_into(self) -> std::result::Result<f64, Self::Error> {
        match self {
            Value::Float(v) => Ok(v),
            _ => Err(
                "Tried to call `Value::try_into` to get a f64 from a type that was not a f64."
                    .into(),
            ),
        }
    }
}

impl TryInto<i64> for Value {
    type Error = crate::Error;

    fn try_into(self) -> std::result::Result<i64, Self::Error> {
        match self {
            Value::Integer(v) => Ok(v),
            _ => Err(
                "Tried to call `Value::try_into` to get a i64 from a type that was not a i64."
                    .into(),
            ),
        }
    }
}

impl TryInto<BTreeMap<String, Value>> for Value {
    type Error = crate::Error;

    fn try_into(self) -> std::result::Result<BTreeMap<String, Value>, Self::Error> {
        match self {
            Value::Map(v) => Ok(v),
            _ => Err("Tried to call `Value::try_into` to get a BTreeMap<String, Value> from a type that was not a BTreeMap<String, Value>.".into())
        }
    }
}

impl TryInto<Vec<Value>> for Value {
    type Error = crate::Error;

    fn try_into(self) -> std::result::Result<Vec<Value>, Self::Error> {
        match self {
            Value::Array(v) => Ok(v),
            _ => Err("Tried to call `Value::try_into` to get a Vec<Value> from a type that was not a Vec<Value>.".into())
        }
    }
}

impl TryInto<DateTime<Utc>> for Value {
    type Error = crate::Error;

    fn try_into(self) -> std::result::Result<DateTime<Utc>, Self::Error> {
        match self {
            Value::Timestamp(v) => Ok(v),
            _ => Err("Tried to call `Value::try_into` to get a DateTime from a type that was not a DateTime.".into())
        }
    }
}

impl From<remap::Value> for Value {
    fn from(v: remap::Value) -> Self {
        use remap::Value::*;

        match v {
            Bytes(v) => Value::Bytes(v),
            Integer(v) => Value::Integer(v),
            Float(v) => Value::Float(v),
            Boolean(v) => Value::Boolean(v),
            Map(v) => Value::Map(v.into_iter().map(|(k, v)| (k, v.into())).collect()),
            Array(v) => Value::Array(v.into_iter().map(Into::into).collect()),
            Timestamp(v) => Value::Timestamp(v),
            Null => Value::Null,
        }
    }
}

impl From<Value> for remap::Value {
    fn from(v: Value) -> Self {
        use remap::Value::*;

        match v {
            Value::Bytes(v) => Bytes(v),
            Value::Integer(v) => Integer(v),
            Value::Float(v) => Float(v),
            Value::Boolean(v) => Boolean(v),
            Value::Map(v) => Map(v.into_iter().map(|(k, v)| (k, v.into())).collect()),
            Value::Array(v) => Array(v.into_iter().map(Into::into).collect()),
            Value::Timestamp(v) => Timestamp(v),
            Value::Null => Null,
        }
    }
}

impl Value {
    // TODO: return Cow
    pub fn to_string_lossy(&self) -> String {
        match self {
            Value::Bytes(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Value::Timestamp(timestamp) => timestamp_to_string(timestamp),
            Value::Integer(num) => format!("{}", num),
            Value::Float(num) => format!("{}", num),
            Value::Boolean(b) => format!("{}", b),
            Value::Map(map) => serde_json::to_string(map).expect("Cannot serialize map"),
            Value::Array(arr) => serde_json::to_string(arr).expect("Cannot serialize array"),
            Value::Null => "<null>".to_string(),
        }
    }

    pub fn clone_into_bytes(&self) -> Bytes {
        match self {
            Value::Bytes(bytes) => bytes.clone(), // cloning a Bytes is cheap
            Value::Timestamp(timestamp) => Bytes::from(timestamp_to_string(timestamp)),
            Value::Integer(num) => Bytes::from(format!("{}", num)),
            Value::Float(num) => Bytes::from(format!("{}", num)),
            Value::Boolean(b) => Bytes::from(format!("{}", b)),
            Value::Map(map) => Bytes::from(serde_json::to_vec(map).expect("Cannot serialize map")),
            Value::Array(arr) => {
                Bytes::from(serde_json::to_vec(arr).expect("Cannot serialize array"))
            }
            Value::Null => Bytes::from("<null>"),
        }
    }

    pub fn kind(&self) -> &str {
        match self {
            Value::Bytes(_) => "string",
            Value::Timestamp(_) => "timestamp",
            Value::Integer(_) => "integer",
            Value::Float(_) => "float",
            Value::Boolean(_) => "boolean",
            Value::Map(_) => "map",
            Value::Array(_) => "array",
            Value::Null => "null",
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::{fs, io::Read, path::Path};

    fn parse_artifact(path: impl AsRef<Path>) -> std::io::Result<Vec<u8>> {
        let mut test_file = match fs::File::open(path) {
            Ok(file) => file,
            Err(e) => return Err(e),
        };

        let mut buf = Vec::new();
        test_file.read_to_end(&mut buf)?;

        Ok(buf)
    }

    // This test iterates over the `tests/data/fixtures/value` folder and:
    //   * Ensures the parsed folder name matches the parsed type of the `Value`.
    //   * Ensures the `serde_json::Value` to `vector::Value` conversions are harmless. (Think UTF-8 errors)
    //
    // Basically: This test makes sure we aren't mutilating any content users might be sending.
    #[test]
    fn json_value_to_vector_value_to_json_value() {
        crate::test_util::trace_init();
        const FIXTURE_ROOT: &str = "tests/data/fixtures/value";

        tracing::trace!(?FIXTURE_ROOT, "Opening");
        std::fs::read_dir(FIXTURE_ROOT).unwrap().for_each(|type_dir| match type_dir {
            Ok(type_name) => {
                let path = type_name.path();
                tracing::trace!(?path, "Opening");
                std::fs::read_dir(path).unwrap().for_each(|fixture_file| match fixture_file {
                    Ok(fixture_file) => {
                        let path = fixture_file.path();
                        let buf = parse_artifact(&path).unwrap();

                        let serde_value: serde_json::Value = serde_json::from_slice(&*buf).unwrap();
                        let vector_value = Value::from(serde_value.clone());

                        // Validate type
                        let expected_type = type_name.path().file_name().unwrap().to_string_lossy().to_string();
                        assert!(match &*expected_type {
                            "boolean" => vector_value.is_boolean(),
                            "integer" => vector_value.is_integer(),
                            "bytes" => vector_value.is_bytes(),
                            "array" => vector_value.is_array(),
                            "map" => vector_value.is_map(),
                            "null" => vector_value.is_null(),
                            _ => unreachable!("You need to add a new type handler here."),
                        }, "Typecheck failure. Wanted {}, got {:?}.", expected_type, vector_value);

                        let serde_value_again: serde_json::Value = vector_value.clone().try_into().unwrap();

                        tracing::trace!(?path, ?serde_value, ?vector_value, ?serde_value_again, "Asserting equal.");
                        assert_eq!(
                            serde_value,
                            serde_value_again
                        );
                    },
                    _ => panic!("This test should never read Err'ing test fixtures."),
                });
            },
            _ => panic!("This test should never read Err'ing type folders."),
        })
    }
}
