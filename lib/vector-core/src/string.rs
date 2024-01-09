use std::{borrow::Borrow, cell::RefCell, fmt};

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
    Shared(Chars),
    Static(&'static str),
}

impl VectorString {
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Owned(s) => s.as_bytes(),
            Self::Shared(s) => s.as_bytes(),
            Self::Static(s) => s.as_bytes(),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Owned(s) => s.as_str(),
            Self::Shared(s) => s,
            Self::Static(s) => s,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Owned(s) => s.len(),
            Self::Shared(s) => s.len(),
            Self::Static(s) => s.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::Owned(s) => s.is_empty(),
            Self::Shared(s) => s.is_empty(),
            Self::Static(s) => s.is_empty(),
        }
    }

    pub fn into_string(self) -> String {
        match self {
            Self::Owned(s) => s,
            Self::Shared(s) => s.to_string(),
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
        Self::Shared(value)
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
            VectorString::Shared(s) => unsafe {
                // SAFETY: `Chars::into_bytes` is unsafe because it means giving up the invariant
                // that the string data is known, valid UTF-8. Since we're _immediately_ sticking it
                // into `Value::Bytes`, we're not actually handing over an invalid UTF-8 string...
                // and VRL is going to do UTF-8 validity checks anytime it needs to do string-y
                // things to `Values::Bytes` anyways.
                s.into_bytes().into()
            },
            VectorString::Static(s) => bytes::Bytes::from_static(s.as_bytes()).into(),
        }
    }
}

impl From<VectorString> for vrl::value::KeyString {
    fn from(value: VectorString) -> Self {
        match value {
            VectorString::Owned(s) => s.into(),
            VectorString::Shared(s) => {
                let s: &str = &s;
                s.into()
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

impl<'a> mlua::prelude::IntoLua<'a> for VectorString {
    fn into_lua(
        self,
        lua: &'a mlua::prelude::Lua,
    ) -> mlua::prelude::LuaResult<mlua::prelude::LuaValue<'a>> {
        lua.create_string(self.as_bytes())
            .map(mlua::prelude::LuaValue::String)
    }
}

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
