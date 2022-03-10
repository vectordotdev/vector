use crate::value::{timestamp_to_string, StdError, Value};
use bytes::Bytes;
use ordered_float::NotNan;
use serde::de::Error as SerdeError;
use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize, Serializer};
use std::collections::BTreeMap;
use std::fmt;

impl Value {
    /// Converts self into a `Bytes`, using JSON for Map/Array.
    pub fn coerce_to_bytes(&self) -> Bytes {
        match self {
            Value::Bytes(bytes) => bytes.clone(), // cloning `Bytes` is cheap
            Value::Regex(regex) => regex.as_bytes(),
            Value::Timestamp(timestamp) => Bytes::from(timestamp_to_string(timestamp)),
            Value::Integer(num) => Bytes::from(num.to_string()),
            Value::Float(num) => Bytes::from(num.to_string()),
            Value::Boolean(b) => Bytes::from(b.to_string()),
            Value::Object(map) => {
                Bytes::from(serde_json::to_vec(map).expect("Cannot serialize map"))
            }
            Value::Array(arr) => {
                Bytes::from(serde_json::to_vec(arr).expect("Cannot serialize array"))
            }
            Value::Null => Bytes::from("<null>"),
        }
    }

    // TODO: return Cow 🐄
    /// Converts self into a `String` representation, using JSON for `Map`/`Array`.
    pub fn to_string_lossy(&self) -> String {
        match self {
            Value::Bytes(bytes) => String::from_utf8_lossy(bytes).into_owned(),
            Value::Regex(regex) => regex.as_str().to_string(),
            Value::Timestamp(timestamp) => timestamp_to_string(timestamp),
            Value::Integer(num) => num.to_string(),
            Value::Float(num) => num.to_string(),
            Value::Boolean(b) => b.to_string(),
            Value::Object(map) => serde_json::to_string(map).expect("Cannot serialize map"),
            Value::Array(arr) => serde_json::to_string(arr).expect("Cannot serialize array"),
            Value::Null => "<null>".to_string(),
        }
    }
}

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self {
            Value::Integer(i) => serializer.serialize_i64(*i),
            Value::Float(f) => serializer.serialize_f64(f.into_inner()),
            Value::Boolean(b) => serializer.serialize_bool(*b),
            Value::Bytes(_) | Value::Timestamp(_) => {
                serializer.serialize_str(&self.to_string_lossy())
            }
            Value::Regex(regex) => serializer.serialize_str(regex.as_str()),
            Value::Object(m) => serializer.collect_map(m),
            Value::Array(a) => serializer.collect_seq(a),
            Value::Null => serializer.serialize_none(),
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

impl TryInto<serde_json::Value> for Value {
    type Error = StdError;

    fn try_into(self) -> Result<serde_json::Value, Self::Error> {
        match self {
            Value::Boolean(v) => Ok(serde_json::Value::from(v)),
            Value::Integer(v) => Ok(serde_json::Value::from(v)),
            Value::Float(v) => Ok(serde_json::Value::from(v.into_inner())),
            Value::Bytes(v) => Ok(serde_json::Value::from(String::from_utf8(v.to_vec())?)),
            Value::Regex(regex) => Ok(serde_json::Value::from(regex.as_str().to_string())),
            Value::Object(v) => Ok(serde_json::to_value(v)?),
            Value::Array(v) => Ok(serde_json::to_value(v)?),
            Value::Null => Ok(serde_json::Value::Null),
            Value::Timestamp(v) => Ok(serde_json::Value::from(timestamp_to_string(&v))),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::value::Value;
    use std::fs;
    use std::io::Read;
    use std::path::Path;

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

        std::fs::read_dir(FIXTURE_ROOT)
            .unwrap()
            .for_each(|type_dir| match type_dir {
                Ok(type_name) => {
                    let path = type_name.path();
                    std::fs::read_dir(path)
                        .unwrap()
                        .for_each(|fixture_file| match fixture_file {
                            Ok(fixture_file) => {
                                let path = fixture_file.path();
                                let buf = parse_artifact(&path).unwrap();

                                let serde_value: serde_json::Value =
                                    serde_json::from_slice(&*buf).unwrap();
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
                                    _ => unreachable!("You need to add a new type handler here."),
                                };
                                assert!(
                                    is_match,
                                    "Typecheck failure. Wanted {}, got {:?}.",
                                    expected_type, vector_value
                                );
                                let _value: serde_json::Value = vector_value.try_into().unwrap();
                            }
                            _ => panic!("This test should never read Err'ing test fixtures."),
                        });
                }
                _ => panic!("This test should never read Err'ing type folders."),
            });
    }
}
