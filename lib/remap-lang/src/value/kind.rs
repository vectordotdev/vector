use super::Value;
use std::fmt;
use std::ops::Deref;

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy, Ord, PartialOrd)]
pub enum Kind {
    String,
    Integer,
    Float,
    Boolean,
    Map,
    Array,
    Timestamp,
    Null,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self)
    }
}

impl Kind {
    pub fn all() -> Vec<Self> {
        use Kind::*;

        vec![String, Integer, Float, Boolean, Map, Array, Timestamp, Null]
    }

    pub fn as_str(&self) -> &'static str {
        use Kind::*;

        match self {
            String => "string",
            Integer => "integer",
            Float => "float",
            Boolean => "boolean",
            Map => "map",
            Array => "array",
            Timestamp => "timestamp",
            Null => "null",
        }
    }
}

impl Deref for Kind {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl From<&Value> for Kind {
    fn from(value: &Value) -> Self {
        use Kind::*;

        match value {
            Value::String(_) => String,
            Value::Integer(_) => Integer,
            Value::Float(_) => Float,
            Value::Boolean(_) => Boolean,
            Value::Map(_) => Map,
            Value::Array(_) => Array,
            Value::Timestamp(_) => Timestamp,
            Value::Null => Null,
        }
    }
}
