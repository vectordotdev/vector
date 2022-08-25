use std::{
    collections::{BTreeMap, BTreeSet},
    mem,
};

use bytes::{Bytes, BytesMut};
use chrono::{DateTime, Utc};
use serde_json::{value::RawValue, Value as JsonValue};
use smallvec::SmallVec;
use value::Value;

pub trait ByteSizeOf {
    /// Returns the in-memory size of this type
    ///
    /// This function returns the total number of bytes that
    /// [`std::mem::size_of`] does in addition to any interior allocated
    /// bytes. It default implementation is `std::mem::size_of` +
    /// `ByteSizeOf::allocated_bytes`.
    fn size_of(&self) -> usize {
        mem::size_of_val(self) + self.allocated_bytes()
    }

    /// Returns the allocated bytes of this type
    ///
    /// This function returns the total number of bytes that have been allocated
    /// interior to this type instance. It does not include any bytes that are
    /// captured by [`std::mem::size_of`] except for any bytes that are iterior
    /// to this type. For instance, `BTreeMap<String, Vec<u8>>` would count all
    /// bytes for `String` and `Vec<u8>` instances but not the exterior bytes
    /// for `BTreeMap`.
    fn allocated_bytes(&self) -> usize;

    /// Estimated size of this type, represented as a JSON-encoded string.
    ///
    /// This is an *estimation*, and *MUST NOT* be used to calculate the final byte size of a
    /// JSON-encoded payload.
    ///
    /// The implementation *MUST* consider the encoding to be comptactly-formatted (e.g. only
    /// significant whitespace is counted).
    #[must_use]
    fn estimated_json_encoded_size_of(&self) -> usize;
}

impl<'a, T> ByteSizeOf for &'a T
where
    T: ByteSizeOf,
{
    fn allocated_bytes(&self) -> usize {
        (*self).size_of()
    }

    fn estimated_json_encoded_size_of(&self) -> usize {
        (*self).estimated_json_encoded_size_of()
    }
}

impl ByteSizeOf for Bytes {
    fn allocated_bytes(&self) -> usize {
        self.len()
    }

    fn estimated_json_encoded_size_of(&self) -> usize {
        string_like_estimated_json_byte_size(self.len())
    }
}

impl ByteSizeOf for BytesMut {
    fn allocated_bytes(&self) -> usize {
        self.len()
    }

    fn estimated_json_encoded_size_of(&self) -> usize {
        string_like_estimated_json_byte_size(self.len())
    }
}

impl ByteSizeOf for String {
    fn allocated_bytes(&self) -> usize {
        self.len()
    }

    fn estimated_json_encoded_size_of(&self) -> usize {
        string_like_estimated_json_byte_size(self.len())
    }
}

impl<'a> ByteSizeOf for &'a str {
    fn allocated_bytes(&self) -> usize {
        0
    }

    fn estimated_json_encoded_size_of(&self) -> usize {
        string_like_estimated_json_byte_size(self.len())
    }
}

impl ByteSizeOf for str {
    fn allocated_bytes(&self) -> usize {
        self.len()
    }

    fn estimated_json_encoded_size_of(&self) -> usize {
        string_like_estimated_json_byte_size(self.len())
    }
}

impl ByteSizeOf for bool {
    fn allocated_bytes(&self) -> usize {
        0
    }

    fn estimated_json_encoded_size_of(&self) -> usize {
        const TRUE_SIZE: usize = 4;
        const FALSE_SIZE: usize = 5;

        if *self {
            TRUE_SIZE
        } else {
            FALSE_SIZE
        }
    }
}

impl<K, V> ByteSizeOf for BTreeMap<K, V>
where
    K: ByteSizeOf,
    V: ByteSizeOf,
{
    fn allocated_bytes(&self) -> usize {
        self.iter()
            .fold(0, |acc, (k, v)| acc + k.size_of() + v.size_of())
    }

    fn estimated_json_encoded_size_of(&self) -> usize {
        object_like_estimated_json_byte_size(self.iter())
    }
}

impl<T> ByteSizeOf for BTreeSet<T>
where
    T: ByteSizeOf,
{
    fn allocated_bytes(&self) -> usize {
        self.iter().map(ByteSizeOf::size_of).sum()
    }

    fn estimated_json_encoded_size_of(&self) -> usize {
        array_like_estimated_json_byte_size(self.iter())
    }
}

impl<T> ByteSizeOf for Vec<T>
where
    T: ByteSizeOf,
{
    fn allocated_bytes(&self) -> usize {
        self.iter().map(ByteSizeOf::size_of).sum()
    }

    fn estimated_json_encoded_size_of(&self) -> usize {
        array_like_estimated_json_byte_size(self.iter())
    }
}

impl<A: smallvec::Array> ByteSizeOf for SmallVec<A>
where
    A::Item: ByteSizeOf,
{
    fn allocated_bytes(&self) -> usize {
        self.iter().map(ByteSizeOf::size_of).sum()
    }

    fn estimated_json_encoded_size_of(&self) -> usize {
        array_like_estimated_json_byte_size(self.iter())
    }
}

impl<T> ByteSizeOf for &[T]
where
    T: ByteSizeOf,
{
    fn allocated_bytes(&self) -> usize {
        self.iter().map(ByteSizeOf::size_of).sum()
    }

    fn estimated_json_encoded_size_of(&self) -> usize {
        array_like_estimated_json_byte_size(self.iter())
    }
}

impl<T, const N: usize> ByteSizeOf for [T; N]
where
    T: ByteSizeOf,
{
    fn size_of(&self) -> usize {
        self.allocated_bytes()
    }

    fn allocated_bytes(&self) -> usize {
        self.iter().map(ByteSizeOf::size_of).sum()
    }

    fn estimated_json_encoded_size_of(&self) -> usize {
        array_like_estimated_json_byte_size(self.iter())
    }
}

impl<T> ByteSizeOf for Option<T>
where
    T: ByteSizeOf,
{
    fn allocated_bytes(&self) -> usize {
        self.as_ref().map_or(0, ByteSizeOf::allocated_bytes)
    }

    fn estimated_json_encoded_size_of(&self) -> usize {
        const NULL_SIZE: usize = 4;

        self.as_ref()
            .map_or(NULL_SIZE, ByteSizeOf::estimated_json_encoded_size_of)
    }
}

macro_rules! num {
    ($t:ty) => {
        impl ByteSizeOf for $t {
            fn allocated_bytes(&self) -> usize {
                0
            }

            fn estimated_json_encoded_size_of(&self) -> usize {
                // NOTE: this is converted into a series of if-statements by the compiler: https://godbolt.org/z/GjhqnzqvM
                fn length(n: $t) -> usize {
                    let mut power = 10;
                    let mut count = 1;
                    while n >= power {
                        count += 1;
                        if let Some(new_power) = power.checked_mul(10) {
                            power = new_power;
                        } else {
                            break;
                        }
                    }
                    count
                }

                length(*self)
            }
        }
    };
}

macro_rules! fnum {
    ($t:ty) => {
        impl ByteSizeOf for $t {
            fn allocated_bytes(&self) -> usize {
                0
            }

            fn estimated_json_encoded_size_of(&self) -> usize {
                let mut buffer = ryu::Buffer::new();
                buffer.format(*self).len()
            }
        }
    };
}

num!(u8);
num!(u16);
num!(u32);
num!(u64);
num!(u128);
num!(usize);
num!(i8);
num!(i16);
num!(i32);
num!(i64);
num!(i128);
num!(isize);
fnum!(f32);
fnum!(f64);

impl ByteSizeOf for Box<RawValue> {
    fn allocated_bytes(&self) -> usize {
        self.get().len()
    }

    fn estimated_json_encoded_size_of(&self) -> usize {
        self.allocated_bytes()
    }
}

impl ByteSizeOf for JsonValue {
    fn allocated_bytes(&self) -> usize {
        match self {
            JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) => 0,
            JsonValue::String(s) => s.len(),
            JsonValue::Array(a) => a.size_of(),
            JsonValue::Object(o) => o.iter().map(|(k, v)| k.size_of() + v.size_of()).sum(),
        }
    }

    fn estimated_json_encoded_size_of(&self) -> usize {
        const NULL_SIZE: usize = 4;

        match self {
            JsonValue::Null => NULL_SIZE,
            JsonValue::Bool(v) => (*v).estimated_json_encoded_size_of(),
            JsonValue::Number(v) if v.is_u64() => {
                v.as_u64().unwrap().estimated_json_encoded_size_of()
            }
            JsonValue::Number(v) if v.is_i64() => {
                v.as_i64().unwrap().estimated_json_encoded_size_of()
            }
            JsonValue::Number(v) => v.as_f64().unwrap().estimated_json_encoded_size_of(),
            JsonValue::String(s) => string_like_estimated_json_byte_size(s.len()),
            JsonValue::Array(a) => array_like_estimated_json_byte_size(a.iter()),
            JsonValue::Object(o) => object_like_estimated_json_byte_size(o.iter()),
        }
    }
}

impl ByteSizeOf for Value {
    fn allocated_bytes(&self) -> usize {
        match self {
            Value::Bytes(bytes) => bytes.len(),
            Value::Object(map) => map.size_of(),
            Value::Array(arr) => arr.size_of(),
            _ => 0,
        }
    }

    fn estimated_json_encoded_size_of(&self) -> usize {
        const NULL_SIZE: usize = 4;

        match self {
            Value::Bytes(bytes) => string_like_estimated_json_byte_size(bytes.len()),
            Value::Object(map) => map.estimated_json_encoded_size_of(),
            Value::Array(arr) => arr.estimated_json_encoded_size_of(),
            Value::Boolean(v) => v.estimated_json_encoded_size_of(),
            Value::Regex(v) => v.to_string().estimated_json_encoded_size_of(),
            Value::Integer(v) => v.estimated_json_encoded_size_of(),
            Value::Float(v) => v.estimated_json_encoded_size_of(),
            Value::Timestamp(v) => v.estimated_json_encoded_size_of(),
            Value::Null => NULL_SIZE,
        }
    }
}

impl ByteSizeOf for DateTime<Utc> {
    fn allocated_bytes(&self) -> usize {
        0
    }

    fn estimated_json_encoded_size_of(&self) -> usize {
        /// This estimation assumes the following:
        ///
        /// ```ignore
        /// self.to_rfc3339_opts(secform: SecondsFormat::Millis, use_z: true).len()
        /// ```
        ///
        /// Our `Value` type uses `SecondsFormat::AutoSi`, which will auto-detect a range between 0
        /// and 9 digits to represent the timestamp at nanosecond precision.
        ///
        /// Representation will also be off by a few bytes if other formatting options are used when
        /// serializing to JSON.
        ///
        /// "2019-10-12T07:20:50.522Z"
        const RFC3339_SIZE: usize = 26;

        RFC3339_SIZE
    }
}

#[must_use]
pub fn string_like_estimated_json_byte_size(len: usize) -> usize {
    const QUOTES_SIZE: usize = 2;

    len + QUOTES_SIZE
}

#[must_use]
pub fn array_like_estimated_json_byte_size<T, V>(iter: T) -> usize
where
    T: Iterator<Item = V>,
    V: ByteSizeOf,
{
    const BRACKETS_SIZE: usize = 2;
    const COMMA_SIZE: usize = 1;

    let mut size = iter.fold(BRACKETS_SIZE, |acc, v| {
        acc + v.estimated_json_encoded_size_of() + COMMA_SIZE
    });

    // no trailing comma
    if size > BRACKETS_SIZE {
        size -= COMMA_SIZE;
    }

    size
}

#[must_use]
pub fn object_like_estimated_json_byte_size<T, K, V>(iter: T) -> usize
where
    T: Iterator<Item = (K, V)>,
    K: ByteSizeOf,
    V: ByteSizeOf,
{
    const BRACES_SIZE: usize = 2;
    const COLON_SIZE: usize = 1;
    const COMMA_SIZE: usize = 1;

    let mut size = iter.fold(BRACES_SIZE, |acc, (k, v)| {
        acc + k.estimated_json_encoded_size_of()
            + COLON_SIZE
            + v.estimated_json_encoded_size_of()
            + COMMA_SIZE
    });

    // no trailing comma
    if size > BRACES_SIZE {
        size -= COMMA_SIZE;
    }

    size
}

pub fn struct_estimated_json_byte_size(fields: &[(&'static str, &dyn ByteSizeOf)]) -> usize {
    const BRACES_SIZE: usize = 2;
    const COLON_SIZE: usize = 1;
    const COMMA_SIZE: usize = 1;

    let mut size = fields.iter().fold(BRACES_SIZE, |acc, (k, v)| {
        acc + string_like_estimated_json_byte_size(k.len())
            + COLON_SIZE
            + v.estimated_json_encoded_size_of()
            + COMMA_SIZE
    });

    // no trailing comma
    if size > BRACES_SIZE {
        size -= COMMA_SIZE;
    }

    size
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "serde")]
    fn test_estimated_json_encoded_size_of() {
        use serde::Serialize;

        fn case<T: ByteSizeOf + Serialize>(value: T, expected_size: usize) {
            let json = serde_json::to_string(&value).unwrap();
            let size = value.estimated_json_encoded_size_of();
            assert_eq!(size, expected_size);
            assert_eq!(size, json.len());
        }

        case("foo", 5);
        case("foo bar", 9);
        case(Value::Bytes("foo".into()), 5);

        case(true, 4);
        case(false, 5);
        case(Value::Boolean(true), 4);
        case(Value::Boolean(false), 5);
        case(
            BTreeMap::from([("foo", Value::from(true)), ("bar", Value::from("baz"))]),
            24,
        );
    }

    #[test]
    #[cfg(feature = "serde")]
    fn test_object_like_estimated_json_byte_size() {
        let data: BTreeMap<_, Value> = [("foo", true.into()), ("bar", "baz".into())].into();
        let json = serde_json::to_string(&data).unwrap();
        let size = object_like_estimated_json_byte_size(data.iter());

        assert_eq!(size, json.len());
    }
}
