use std::{borrow::Borrow, cell::RefCell, fmt};

use bytes::Bytes;
use protobuf::Chars;
use serde::{de::Visitor, Deserialize, Serialize};
use serde_json::Value;
use vector_common::byte_size_of::ByteSizeOf;
use vector_config::{
    schema::{generate_string_schema, SchemaGenerator, SchemaObject},
    Configurable, ConfigurableString, GenerateError, Metadata, ToValue,
};

#[derive(Clone, Debug, Eq)]
pub enum VectorString {
    Owned(String),
    Shared(Bytes),
    Static(&'static str),
}

impl VectorString {
    pub fn split_once(&self, delimiter: char) -> Option<(Self, Self)> {
        match self {
            Self::Owned(s) => s
                .split_once(delimiter)
                .map(|(s1, s2)| (s1.to_string().into(), s2.to_string().into())),
            Self::Shared(buf) => {
                // SAFETY: We never create this variant unless the source has already validated that
                // the byte buffer contains valid UTF-8.
                let s = unsafe { std::str::from_utf8_unchecked(&buf) };
                s.split_once(delimiter).map(|(s1, s2)| {
                    (
                        Self::Shared(buf.slice_ref(s1.as_bytes())),
                        Self::Shared(buf.slice_ref(s2.as_bytes())),
                    )
                })
            }
            Self::Static(s) => s
                .split_once(delimiter)
                .map(|(s1, s2)| (Self::Static(s1), Self::Static(s2))),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Owned(s) => s.as_bytes(),
            Self::Shared(buf) => &buf,
            Self::Static(s) => s.as_bytes(),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Owned(s) => s.as_str(),
            Self::Shared(buf) => {
                // SAFETY: We never create this variant unless the source has already validated that
                // the byte buffer contains valid UTF-8.
                unsafe { std::str::from_utf8_unchecked(&buf) }
            }
            Self::Static(s) => s,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Owned(s) => s.len(),
            Self::Shared(buf) => buf.len(),
            Self::Static(s) => s.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::Owned(s) => s.is_empty(),
            Self::Shared(buf) => buf.is_empty(),
            Self::Static(s) => s.is_empty(),
        }
    }

    pub fn into_string(self) -> String {
        match self {
            Self::Owned(s) => s,
            Self::Shared(buf) => {
                // SAFETY: We never create this variant unless the source has already validated that
                // the byte buffer contains valid UTF-8.
                let s = unsafe { std::str::from_utf8_unchecked(&buf) };
                s.to_string()
            }
            Self::Static(s) => s.to_string(),
        }
    }
}

impl fmt::Display for VectorString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_str().fmt(f)
    }
}

impl PartialEq<VectorString> for VectorString {
    fn eq(&self, other: &VectorString) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialOrd for VectorString {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.as_str().partial_cmp(other.as_str())
    }
}

impl Ord for VectorString {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl std::hash::Hash for VectorString {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl Default for VectorString {
    fn default() -> Self {
        Self::Owned(String::new())
    }
}

impl Borrow<str> for VectorString {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl From<String> for VectorString {
    fn from(value: String) -> Self {
        Self::Owned(value)
    }
}

impl From<&String> for VectorString {
    fn from(value: &String) -> Self {
        Self::Owned(value.clone())
    }
}

impl From<Chars> for VectorString {
    fn from(value: Chars) -> Self {
        // SAFETY:: We don't do anything with the returned `Bytes` before wrapping it, after which
        // is is held immutably, so the validity of the UTF-8 holds between `Chars` and `VectorString`.
        let buf = unsafe { value.into_bytes() };

        Self::Shared(buf)
    }
}

impl From<&'static str> for VectorString {
    fn from(value: &'static str) -> Self {
        Self::Owned(value.to_string())
    }
}

impl From<VectorString> for vrl::value::Value {
    fn from(value: VectorString) -> Self {
        match value {
            VectorString::Owned(s) => s.into(),
            VectorString::Shared(buf) => buf.into(),
            VectorString::Static(s) => bytes::Bytes::from_static(s.as_bytes()).into(),
        }
    }
}

impl From<VectorString> for vrl::value::KeyString {
    fn from(value: VectorString) -> Self {
        match value {
            VectorString::Owned(s) => s.into(),
            VectorString::Shared(buf) => {
                // SAFETY: We never create this variant unless the source has already validated that
                // the byte buffer contains valid UTF-8.
                let s = unsafe { std::str::from_utf8_unchecked(&buf) };
                s.to_string().into()
            }
            VectorString::Static(s) => s.into(),
        }
    }
}

impl ByteSizeOf for VectorString {
    fn allocated_bytes(&self) -> usize {
        self.as_bytes().len()
    }
}

impl Configurable for VectorString {
    fn metadata() -> Metadata {
        Metadata::with_transparent(true)
    }

    fn generate_schema(_: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        Ok(generate_string_schema())
    }
}

impl ConfigurableString for VectorString {}

impl ToValue for VectorString {
    fn to_value(&self) -> Value {
        Value::String(self.as_str().to_string())
    }
}

#[cfg(feature = "lua")]
impl<'a> mlua::prelude::IntoLua<'a> for VectorString {
    fn into_lua(
        self,
        lua: &'a mlua::prelude::Lua,
    ) -> mlua::prelude::LuaResult<mlua::prelude::LuaValue<'a>> {
        lua.create_string(self.as_bytes())
            .map(mlua::prelude::LuaValue::String)
    }
}

#[cfg(feature = "lua")]
impl<'a> mlua::prelude::FromLua<'a> for VectorString {
    fn from_lua(
        value: mlua::prelude::LuaValue<'a>,
        lua: &'a mlua::prelude::Lua,
    ) -> mlua::prelude::LuaResult<Self> {
        let ty = value.type_name();
        lua.coerce_string(value)?
            .ok_or_else(|| mlua::prelude::LuaError::FromLuaConversionError {
                from: ty,
                to: "string",
                message: Some("expected string or number".to_string()),
            })
            .and_then(|s| s.to_str().map(|s| s.to_owned().into()))
    }
}

impl Serialize for VectorString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

struct VectorStringVisitor;

impl<'de> Visitor<'de> for VectorStringVisitor {
    type Value = VectorString;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "a string")
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(VectorString::from(v))
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(VectorString::from(v.to_owned()))
    }

    fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(VectorString::from(v.to_owned()))
    }
}

impl<'de> Deserialize<'de> for VectorString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_string(VectorStringVisitor)
    }
}

#[cfg(test)]
mod test_support {
    use quickcheck::{Arbitrary, Gen};

    use super::VectorString;

    impl Arbitrary for VectorString {
        fn arbitrary(g: &mut Gen) -> Self {
            Self::from(String::arbitrary(g))
        }

        fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
            Box::new(self.as_str().to_string().shrink().map(Self::Owned))
        }
    }
}
