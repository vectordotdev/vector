use std::{borrow::Cow, collections::BTreeMap, fmt};

use bytes::Bytes;
use ordered_float::NotNan;
use serde::de::Error as SerdeError;
use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize, Serializer};

use crate::value::{timestamp_to_string, StdError, Value};

impl Value {
    /// Converts self into a `Bytes`, using JSON for Map/Array.
    pub fn coerce_to_bytes(&self) -> Bytes {
        match self {
            Self::Bytes(bytes) => bytes.clone(), // cloning `Bytes` is cheap
            Self::Regex(regex) => regex.as_bytes(),
            Self::Timestamp(timestamp) => Bytes::from(timestamp_to_string(timestamp)),
            Self::Integer(num) => Bytes::from(num.to_string()),
            Self::Float(num) => Bytes::from(num.to_string()),
            Self::Boolean(b) => Bytes::from(b.to_string()),
            Self::Object(map) => {
                Bytes::from(serde_json::to_vec(map).expect("Cannot serialize map"))
            }
            Self::Array(arr) => {
                Bytes::from(serde_json::to_vec(arr).expect("Cannot serialize array"))
            }
            Self::Null => Bytes::from("<null>"),
        }
    }

    /// Converts self into a `String` representation, using JSON for `Map`/`Array`.
    pub fn to_string_lossy(&self) -> Cow<'_, str> {
        match self {
            Self::Bytes(bytes) => String::from_utf8_lossy(bytes),
            Self::Regex(regex) => regex.as_str().into(),
            Self::Timestamp(timestamp) => timestamp_to_string(timestamp).into(),
            Self::Integer(num) => num.to_string().into(),
            Self::Float(num) => num.to_string().into(),
            Self::Boolean(b) => b.to_string().into(),
            Self::Object(map) => serde_json::to_string(map)
                .expect("Cannot serialize map")
                .into(),
            Self::Array(arr) => serde_json::to_string(arr)
                .expect("Cannot serialize array")
                .into(),
            Self::Null => "<null>".into(),
        }
    }
}

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self {
            Self::Integer(i) => serializer.serialize_i64(*i),
            Self::Float(f) => serializer.serialize_f64(f.into_inner()),
            Self::Boolean(b) => serializer.serialize_bool(*b),
            Self::Bytes(b) => serializer.serialize_str(String::from_utf8_lossy(b).as_ref()),
            Self::Timestamp(ts) => serializer.serialize_str(&timestamp_to_string(ts)),
            Self::Regex(regex) => serializer.serialize_str(regex.as_str()),
            Self::Object(m) => serializer.collect_map(m),
            Self::Array(a) => serializer.collect_seq(a),
            Self::Null => serializer.serialize_none(),
        }
    }
}

impl<'de> Deserialize<'de> for Value {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ValueVisitor;

        impl<'de> Visitor<'de> for ValueVisitor {
            type Value = Value;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("any valid JSON value")
            }

            #[inline]
            fn visit_bool<E>(self, value: bool) -> Result<Value, E> {
                Ok(value.into())
            }

            #[inline]
            fn visit_i64<E>(self, value: i64) -> Result<Value, E> {
                Ok(value.into())
            }

            #[inline]
            fn visit_u64<E>(self, value: u64) -> Result<Value, E> {
                Ok((value as i64).into())
            }

            #[inline]
            fn visit_f64<E>(self, value: f64) -> Result<Value, E>
            where
                E: serde::de::Error,
            {
                let f = NotNan::new(value).map_err(|_| {
                    SerdeError::invalid_value(serde::de::Unexpected::Float(value), &self)
                })?;
                Ok(Value::Float(f))
            }

            #[inline]
            fn visit_str<E>(self, value: &str) -> Result<Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Value::Bytes(Bytes::copy_from_slice(value.as_bytes())))
            }

            #[inline]
            fn visit_string<E>(self, value: String) -> Result<Value, E> {
                Ok(Value::Bytes(value.into()))
            }

            #[inline]
            fn visit_none<E>(self) -> Result<Value, E> {
                Ok(Value::Null)
            }

            #[inline]
            fn visit_some<D>(self, deserializer: D) -> Result<Value, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                Deserialize::deserialize(deserializer)
            }

            #[inline]
            fn visit_unit<E>(self) -> Result<Value, E> {
                Ok(Value::Null)
            }

            #[inline]
            fn visit_seq<V>(self, mut visitor: V) -> Result<Value, V::Error>
            where
                V: SeqAccess<'de>,
            {
                let mut vec = Vec::new();
                while let Some(value) = visitor.next_element()? {
                    vec.push(value);
                }

                Ok(Value::Array(vec))
            }

            fn visit_map<V>(self, mut visitor: V) -> Result<Value, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut map = BTreeMap::new();
                while let Some((key, value)) = visitor.next_entry()? {
                    map.insert(key, value);
                }

                Ok(Value::Object(map))
            }
        }

        deserializer.deserialize_any(ValueVisitor)
    }
}

impl From<serde_json::Value> for Value {
    fn from(json_value: serde_json::Value) -> Self {
        match json_value {
            serde_json::Value::Bool(b) => Self::Boolean(b),
            serde_json::Value::Number(n) if n.is_i64() => n.as_i64().unwrap().into(),
            serde_json::Value::Number(n) if n.is_f64() => {
                // JSON doesn't support NaN values
                NotNan::new(n.as_f64().unwrap()).unwrap().into()
            }
            serde_json::Value::Number(n) => n.to_string().into(),
            serde_json::Value::String(s) => Self::Bytes(Bytes::from(s)),
            serde_json::Value::Object(obj) => Self::Object(
                obj.into_iter()
                    .map(|(key, value)| (key, Self::from(value)))
                    .collect(),
            ),
            serde_json::Value::Array(arr) => Self::Array(arr.into_iter().map(Self::from).collect()),
            serde_json::Value::Null => Self::Null,
        }
    }
}

impl From<&serde_json::Value> for Value {
    fn from(json_value: &serde_json::Value) -> Self {
        json_value.clone().into()
    }
}

impl TryInto<serde_json::Value> for Value {
    type Error = StdError;

    fn try_into(self) -> Result<serde_json::Value, Self::Error> {
        match self {
            Self::Boolean(v) => Ok(serde_json::Value::from(v)),
            Self::Integer(v) => Ok(serde_json::Value::from(v)),
            Self::Float(v) => Ok(serde_json::Value::from(v.into_inner())),
            Self::Bytes(v) => Ok(serde_json::Value::from(String::from_utf8(v.to_vec())?)),
            Self::Regex(regex) => Ok(serde_json::Value::from(regex.as_str().to_string())),
            Self::Object(v) => Ok(serde_json::to_value(v)?),
            Self::Array(v) => Ok(serde_json::to_value(v)?),
            Self::Null => Ok(serde_json::Value::Null),
            Self::Timestamp(v) => Ok(serde_json::Value::from(timestamp_to_string(&v))),
        }
    }
}

#[cfg(test)]
mod test {
    use std::fs;
    use std::io::Read;
    use std::path::Path;

    use crate::value::Value;

    pub fn parse_artifact(path: impl AsRef<Path>) -> std::io::Result<Vec<u8>> {
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
        const FIXTURE_ROOT: &str = "tests/data/fixtures/value";

        for type_dir in std::fs::read_dir(FIXTURE_ROOT).unwrap() {
            type_dir.map_or_else(
                |_| panic!("This test should never read Err'ing type folders."),
                |type_name| {
                    let path = type_name.path();
                    for fixture_file in std::fs::read_dir(path).unwrap() {
                        fixture_file.map_or_else(
                            |_| panic!("This test should never read Err'ing test fixtures."),
                            |fixture_file| {
                                let path = fixture_file.path();
                                let buf = parse_artifact(path).unwrap();

                                let serde_value: serde_json::Value =
                                    serde_json::from_slice(&buf).unwrap();
                                let vector_value = Value::from(serde_value);

                                // Validate type
                                let expected_type = type_name
                                    .path()
                                    .file_name()
                                    .unwrap()
                                    .to_string_lossy()
                                    .to_string();
                                let is_match = match vector_value {
                                    Value::Boolean(_) => expected_type.eq("boolean"),
                                    Value::Integer(_) => expected_type.eq("integer"),
                                    Value::Bytes(_) => expected_type.eq("bytes"),
                                    Value::Array { .. } => expected_type.eq("array"),
                                    Value::Object(_) => expected_type.eq("map"),
                                    Value::Null => expected_type.eq("null"),
                                    _ => {
                                        unreachable!("You need to add a new type handler here.")
                                    }
                                };
                                assert!(
                                    is_match,
                                    "Typecheck failure. Wanted {}, got {:?}.",
                                    expected_type, vector_value
                                );
                                let _value: serde_json::Value = vector_value.try_into().unwrap();
                            },
                        );
                    }
                },
            );
        }
    }
}
