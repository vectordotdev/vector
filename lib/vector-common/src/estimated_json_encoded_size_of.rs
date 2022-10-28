use chrono::Timelike;
use serde::{ser, Serialize};
use value::Value;

const NULL_SIZE: usize = 4;
const TRUE_SIZE: usize = 4;
const FALSE_SIZE: usize = 5;

const BRACKET_SIZE: usize = 1;
const BRACES_SIZE: usize = 2;
const BRACE_SIZE: usize = 1;

const QUOTES_SIZE: usize = 2;
const COMMA_SIZE: usize = 1;
const COLON_SIZE: usize = 1;

const EPOCH_RFC3339_0: &str = "1970-01-01T00:00:00Z";
const EPOCH_RFC3339_3: &str = "1970-01-01T00:00:00.000Z";
const EPOCH_RFC3339_6: &str = "1970-01-01T00:00:00.000000Z";
const EPOCH_RFC3339_9: &str = "1970-01-01T00:00:00.000000000Z";

/// A wrapper type around the default `Value` type, to implement the `Serialize` trait in an
/// efficient way to count the JSON encoded bytes of a `Value`.
///
/// See the comments in the `Serializer` implementation for more details.
pub struct JsonEncodedByteCountingValue<'a>(pub &'a Value);

impl<'a> Serialize for JsonEncodedByteCountingValue<'a> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        match &self.0 {
            // The timestamp is converted to a static epoch timestamp, to avoid any unnecessary
            // allocations.
            //
            // The following invariants must hold for the size of timestamps to remain correct:
            //
            // - `chrono::SecondsFormat::AutoSi` is used to calculate nanoseconds precision.
            // - `chrono::offset::Utc` is used as the timezone.
            // - `use_z` is `true` for the `chrono::DateTime#to_rfc3339_opts` function call.
            Value::Timestamp(ts) => {
                let ns = ts.nanosecond() % 1_000_000_000;
                let epoch = if ns == 0 {
                    EPOCH_RFC3339_0
                } else if ns % 1_000_000 == 0 {
                    EPOCH_RFC3339_3
                } else if ns % 1_000 == 0 {
                    EPOCH_RFC3339_6
                } else {
                    EPOCH_RFC3339_9
                };

                serializer.serialize_str(epoch)
            }

            // Collection types have their inner `Value`'s wrapped in `JsonEncodedValue`.
            Value::Object(m) => serializer.collect_map(m.iter().map(|(k, v)| (k, Self(v)))),
            Value::Array(a) => serializer.collect_seq(a.iter().map(Self)),

            // The `Value` type serializes `Value::Bytes` using `serialize_str`, but this has two
            // downsides:
            //
            // 1. For invalid UTF-8 encoded bytes, it will replace them with `U+FFFD REPLACEMENT,
            //    requiring allocations before counting the bytes.
            //
            // 2. Even for valid UTF-8 encoded bytes, it will have to validate all individual bytes,
            //    which our soaks have shown to cause a significant drop in throughput.
            //
            // Because of this, we take the assumption that all bytes passed through this serializer
            // are valid UTF-8 encoded bytes, and thus can be counted as-is. If this is not the
            // case, the final byte size will be off slightly, or significantly, depending on how
            // many of the bytes need to be escaped, or replaced.
            Value::Bytes(b) => serializer.serialize_bytes(b),

            // All other `Value` variants are serialized according to the default serialization
            // implementation of that type.
            v => v.serialize(serializer),
        }
    }
}

/// A helper trait that is implemented for any `T` that implements `serde::Serialize`, to get the
/// estimated JSON encoded size of that type.
pub trait EstimatedJsonEncodedSizeOf {
    fn estimated_json_encoded_size_of(&self) -> usize;
}

impl<T> EstimatedJsonEncodedSizeOf for T
where
    T: serde::Serialize,
{
    /// Returns the estimated JSON encoded size of `self`, or `0` if the size cannot be calculated
    /// because `T` errors during serialization.
    #[inline]
    fn estimated_json_encoded_size_of(&self) -> usize {
        estimated_size_of(self)
    }
}

#[derive(Debug)]
pub struct Error;

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error")
    }
}

impl std::error::Error for Error {}
impl ser::Error for Error {
    fn custom<T: std::fmt::Display>(_msg: T) -> Self {
        Self
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// A serializer that counts the number of JSON-encoded bytes in a serializable type.
///
/// See [`estimated_size_of`] for an easy-to-use wrapper function around this serializer.
///
/// This serializer is **optimized for performance**. This means that it is allowed to *approximate*
/// the size of a type, if doing an exact calculation is too expensive.
///
/// Specifically
///
/// Additionally, the serializer assumes the type is serialized to a JSON-encoded string using the
/// `serde_json` crate, which internally uses the `ryu` crate to encode floating point types.
#[derive(Default)]
pub struct Serializer {
    bytes: usize,
    start_collection: bool,
}

/// Return the estimated size of `T` as represented by a JSON-encoded string.
///
/// See [`Serializer`] for more details.
///
/// # Errors
///
/// Returns an error if `T` cannot be serialized.
pub fn estimated_size_of<T>(value: &T) -> usize
where
    T: Serialize,
{
    let mut serializer = Serializer::default();
    _ = value.serialize(&mut serializer);
    serializer.bytes
}

impl<'a> ser::Serializer for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn serialize_bool(self, v: bool) -> Result<()> {
        self.bytes += if v { TRUE_SIZE } else { FALSE_SIZE };
        Ok(())
    }

    #[rustfmt::skip]
    fn serialize_i8(self, v: i8) -> Result<()> {
        // -128 ..= 127
        if        v < -99 { self.bytes += 4;
        } else if v <  -9 { self.bytes += 3;
        } else if v <   0 { self.bytes += 2;
        } else if v <  10 { self.bytes += 1;
        } else if v < 100 { self.bytes += 2;
        } else            { self.bytes += 3; }

        Ok(())
    }

    #[rustfmt::skip]
    fn serialize_i16(self, v: i16) -> Result<()> {
        // -32_768 ..= 32_767
        if        v < -9_999 { self.bytes += 6;
        } else if v <   -999 { self.bytes += 5;
        } else if v <    -99 { self.bytes += 4;
        } else if v <     -9 { self.bytes += 3;
        } else if v <      0 { self.bytes += 2;
        } else if v <     10 { self.bytes += 1;
        } else if v <    100 { self.bytes += 2;
        } else if v <  1_000 { self.bytes += 3;
        } else if v < 10_000 { self.bytes += 4;
        } else               { self.bytes += 5; }

        Ok(())
    }

    #[rustfmt::skip]
    fn serialize_i32(self, v: i32) -> Result<()> {
        // -2_147_483_648 ..= 2_147_483_647
        if        v <  -999_999_999 { self.bytes += 11;
        } else if v <   -99_999_999 { self.bytes += 10;
        } else if v <    -9_999_999 { self.bytes +=  9;
        } else if v <      -999_999 { self.bytes +=  8;
        } else if v <       -99_999 { self.bytes +=  7;
        } else if v <        -9_999 { self.bytes +=  6;
        } else if v <          -999 { self.bytes +=  5;
        } else if v <           -99 { self.bytes +=  4;
        } else if v <            -9 { self.bytes +=  3;
        } else if v <             0 { self.bytes +=  2;
        } else if v <            10 { self.bytes +=  1;
        } else if v <           100 { self.bytes +=  2;
        } else if v <         1_000 { self.bytes +=  3;
        } else if v <        10_000 { self.bytes +=  4;
        } else if v <       100_000 { self.bytes +=  5;
        } else if v <     1_000_000 { self.bytes +=  6;
        } else if v <    10_000_000 { self.bytes +=  7;
        } else if v <   100_000_000 { self.bytes +=  8;
        } else if v < 1_000_000_000 { self.bytes +=  9;
        } else                      { self.bytes += 10; }

        Ok(())
    }

    #[rustfmt::skip]
    fn serialize_i64(self, v: i64) -> Result<()> {
        // -9_223_372_036_854_775_808 ..= 9_223_372_036_854_775_807
        if        v <  -999_999_999_999_999_999 { self.bytes += 20;
        } else if v <   -99_999_999_999_999_999 { self.bytes += 19;
        } else if v <    -9_999_999_999_999_999 { self.bytes += 18;
        } else if v <      -999_999_999_999_999 { self.bytes += 17;
        } else if v <       -99_999_999_999_999 { self.bytes += 16;
        } else if v <        -9_999_999_999_999 { self.bytes += 15;
        } else if v <          -999_999_999_999 { self.bytes += 14;
        } else if v <           -99_999_999_999 { self.bytes += 13;
        } else if v <            -9_999_999_999 { self.bytes += 12;
        } else if v <              -999_999_999 { self.bytes += 11;
        } else if v <               -99_999_999 { self.bytes += 10;
        } else if v <                -9_999_999 { self.bytes +=  9;
        } else if v <                  -999_999 { self.bytes +=  8;
        } else if v <                   -99_999 { self.bytes +=  7;
        } else if v <                    -9_999 { self.bytes +=  6;
        } else if v <                      -999 { self.bytes +=  5;
        } else if v <                       -99 { self.bytes +=  4;
        } else if v <                        -9 { self.bytes +=  3;
        } else if v <                         0 { self.bytes +=  2;
        } else if v <                        10 { self.bytes +=  1;
        } else if v <                       100 { self.bytes +=  2;
        } else if v <                     1_000 { self.bytes +=  3;
        } else if v <                    10_000 { self.bytes +=  4;
        } else if v <                   100_000 { self.bytes +=  5;
        } else if v <                 1_000_000 { self.bytes +=  6;
        } else if v <                10_000_000 { self.bytes +=  7;
        } else if v <               100_000_000 { self.bytes +=  8;
        } else if v <             1_000_000_000 { self.bytes +=  9;
        } else if v <            10_000_000_000 { self.bytes += 10;
        } else if v <           100_000_000_000 { self.bytes += 11;
        } else if v <         1_000_000_000_000 { self.bytes += 12;
        } else if v <        10_000_000_000_000 { self.bytes += 13;
        } else if v <       100_000_000_000_000 { self.bytes += 14;
        } else if v <     1_000_000_000_000_000 { self.bytes += 15;
        } else if v <    10_000_000_000_000_000 { self.bytes += 16;
        } else if v <   100_000_000_000_000_000 { self.bytes += 17;
        } else if v < 1_000_000_000_000_000_000 { self.bytes += 18;
        } else                                  { self.bytes += 19; }

        Ok(())
    }

    #[rustfmt::skip]
    fn serialize_u8(self, v: u8) -> Result<()> {
        // 0 ..= 255
        if        v <  10 { self.bytes += 1;
        } else if v < 100 { self.bytes += 2;
        } else            { self.bytes += 3; }

        Ok(())
    }

    #[rustfmt::skip]
    fn serialize_u16(self, v: u16) -> Result<()> {
        // 0 ..= 65_535
        if        v <     10 { self.bytes += 1;
        } else if v <    100 { self.bytes += 2;
        } else if v <  1_000 { self.bytes += 3;
        } else if v < 10_000 { self.bytes += 4;
        } else               { self.bytes += 5; }

        Ok(())
    }

    #[rustfmt::skip]
    fn serialize_u32(self, v: u32) -> Result<()> {
        // 0 ..= 4_294_967_295
        if        v <            10 { self.bytes += 1;
        } else if v <           100 { self.bytes += 2;
        } else if v <         1_000 { self.bytes += 3;
        } else if v <        10_000 { self.bytes += 4;
        } else if v <       100_000 { self.bytes += 5;
        } else if v <     1_000_000 { self.bytes += 6;
        } else if v <    10_000_000 { self.bytes += 7;
        } else if v <   100_000_000 { self.bytes += 8;
        } else if v < 1_000_000_000 { self.bytes += 9;
        } else                      { self.bytes += 10; }

        Ok(())
    }

    #[rustfmt::skip]
    fn serialize_u64(self, v: u64) -> Result<()> {
        // 0 ..= 18_446_744_073_709_551_615
        if        v <                         10 { self.bytes +=  1;
        } else if v <                        100 { self.bytes +=  2;
        } else if v <                      1_000 { self.bytes +=  3;
        } else if v <                     10_000 { self.bytes +=  4;
        } else if v <                    100_000 { self.bytes +=  5;
        } else if v <                  1_000_000 { self.bytes +=  6;
        } else if v <                 10_000_000 { self.bytes +=  7;
        } else if v <                100_000_000 { self.bytes +=  8;
        } else if v <              1_000_000_000 { self.bytes +=  9;
        } else if v <             10_000_000_000 { self.bytes += 10;
        } else if v <            100_000_000_000 { self.bytes += 11;
        } else if v <          1_000_000_000_000 { self.bytes += 12;
        } else if v <         10_000_000_000_000 { self.bytes += 13;
        } else if v <        100_000_000_000_000 { self.bytes += 14;
        } else if v <      1_000_000_000_000_000 { self.bytes += 15;
        } else if v <     10_000_000_000_000_000 { self.bytes += 16;
        } else if v <    100_000_000_000_000_000 { self.bytes += 17;
        } else if v <  1_000_000_000_000_000_000 { self.bytes += 18;
        } else if v < 10_000_000_000_000_000_000 { self.bytes += 19;
        } else                                   { self.bytes += 20; }

        Ok(())
    }

    /// This method delegates to `serialize_f64`, as that is what `serde_json` does as well. Not
    /// doing so would result in a small difference in the reported byte size of the serialized
    /// value.
    fn serialize_f32(self, v: f32) -> Result<()> {
        self.serialize_f64(f64::from(v))
    }

    /// This method assumes the float is finite (not NaN or infinite), which holds true for our
    /// `Value` type, but might not hold true in other cases.
    ///
    /// If the float _is_ of one of those classifications, this method won't panic, but the reported
    /// byte size won't be accurate.
    fn serialize_f64(self, v: f64) -> Result<()> {
        let mut buffer = ryu::Buffer::new();
        self.bytes += buffer.format_finite(v).len();

        Ok(())
    }

    #[inline]
    fn serialize_char(self, v: char) -> Result<()> {
        self.bytes += QUOTES_SIZE + v.len_utf8();
        Ok(())
    }

    fn serialize_str(self, v: &str) -> Result<()> {
        self.serialize_bytes(v.as_bytes())
    }

    /// Consider `bytes` as being a valid `str`.
    ///
    /// This is a special-case that allows a fast path of counting a string as a slice of bytes,
    /// without having to check for invalid UTF-8 characters, or characters that need to be escaped.
    ///
    /// This means that any of those cases are ignored, and thus the final byte count **WILL**
    /// differ from the JSON serialized form of the type.
    ///
    /// This is known, and accepted, as the overhead of checking for valid UTF-8 + escaped character
    /// sequences is too expensive for our use-case of this serializer.
    fn serialize_bytes(self, v: &[u8]) -> Result<()> {
        self.bytes += v.len() + QUOTES_SIZE;
        Ok(())
    }

    #[inline]
    fn serialize_none(self) -> Result<()> {
        self.serialize_unit()
    }

    #[inline]
    fn serialize_some<T>(self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    #[inline]
    fn serialize_unit(self) -> Result<()> {
        self.bytes += NULL_SIZE;
        Ok(())
    }

    #[inline]
    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
        self.serialize_unit()
    }

    #[inline]
    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<()> {
        self.serialize_str(variant)
    }

    #[inline]
    fn serialize_newtype_struct<T>(self, _name: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.bytes += BRACES_SIZE + COLON_SIZE;
        self.serialize_str(variant)?;
        value.serialize(self)?;

        Ok(())
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq> {
        self.start_collection = true;
        self.bytes += BRACKET_SIZE;
        Ok(self)
    }

    #[inline]
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple> {
        self.serialize_seq(Some(len))
    }

    #[inline]
    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        self.bytes += BRACE_SIZE + COLON_SIZE + BRACKET_SIZE;
        self.serialize_str(variant)?;
        self.start_collection = true;
        Ok(self)
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        self.start_collection = true;
        self.bytes += BRACE_SIZE;
        Ok(self)
    }

    #[inline]
    fn serialize_struct(self, _name: &'static str, len: usize) -> Result<Self::SerializeStruct> {
        self.serialize_map(Some(len))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        // { "variant": { ...
        self.bytes += BRACE_SIZE + COLON_SIZE + BRACE_SIZE;
        self.serialize_str(variant)?;
        self.start_collection = true;
        Ok(self)
    }
}

impl<'a> ser::SerializeSeq for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    #[inline]
    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        if !self.start_collection {
            self.bytes += COMMA_SIZE;
        }
        self.start_collection = false;

        value.serialize(&mut **self)
    }

    #[inline]
    fn end(self) -> Result<()> {
        self.bytes += BRACKET_SIZE;
        self.start_collection = false;
        Ok(())
    }
}

impl<'a> ser::SerializeTuple for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    #[inline]
    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        if !self.start_collection {
            self.bytes += COMMA_SIZE;
        }
        self.start_collection = false;

        value.serialize(&mut **self)
    }

    #[inline]
    fn end(self) -> Result<()> {
        self.bytes += BRACKET_SIZE;
        self.start_collection = false;
        Ok(())
    }
}

impl<'a> ser::SerializeTupleStruct for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    #[inline]
    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        if !self.start_collection {
            self.bytes += COMMA_SIZE;
        }
        self.start_collection = false;

        value.serialize(&mut **self)
    }

    #[inline]
    fn end(self) -> Result<()> {
        self.bytes += BRACKET_SIZE;
        self.start_collection = false;
        Ok(())
    }
}

impl<'a> ser::SerializeTupleVariant for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    #[inline]
    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        if !self.start_collection {
            self.bytes += COMMA_SIZE;
        }
        self.start_collection = false;

        value.serialize(&mut **self)
    }

    #[inline]
    fn end(self) -> Result<()> {
        self.bytes += BRACKET_SIZE + BRACE_SIZE;
        self.start_collection = false;
        Ok(())
    }
}

impl<'a> ser::SerializeMap for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    // TODO:
    //
    // A real JSON serializer would need to validate that map keys are strings.
    // This can be done by using a different Serializer to serialize the key
    // (instead of `&mut **self`) and having that other serializer only
    // implement `serialize_str` and return an error on any other data type.
    #[inline]
    fn serialize_key<T>(&mut self, key: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        if !self.start_collection {
            self.bytes += COMMA_SIZE;
        }
        self.start_collection = false;

        key.serialize(&mut **self)
    }

    #[inline]
    fn serialize_value<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.bytes += COLON_SIZE;
        value.serialize(&mut **self)
    }

    #[inline]
    fn end(self) -> Result<()> {
        self.start_collection = false;
        self.bytes += BRACE_SIZE;
        Ok(())
    }
}

impl<'a> ser::SerializeStruct for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    #[inline]
    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        if !self.start_collection {
            self.bytes += COMMA_SIZE;
        }
        self.start_collection = false;

        key.serialize(&mut **self)?;
        self.bytes += COLON_SIZE;
        value.serialize(&mut **self)
    }

    #[inline]
    fn end(self) -> Result<()> {
        self.start_collection = false;
        self.bytes += BRACE_SIZE;
        Ok(())
    }
}

impl<'a> ser::SerializeStructVariant for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    #[inline]
    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        if !self.start_collection {
            self.bytes += COMMA_SIZE;
        }
        self.start_collection = false;

        key.serialize(&mut **self)?;
        self.bytes += COLON_SIZE;
        value.serialize(&mut **self)
    }

    #[inline]
    fn end(self) -> Result<()> {
        self.start_collection = false;
        self.bytes += BRACE_SIZE + BRACE_SIZE;
        Ok(())
    }
}

////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    #![allow(clippy::needless_pass_by_value)]

    use std::collections::HashMap;

    use super::*;
    use quickcheck::{Arbitrary, Gen, TestResult};
    use quickcheck_macros::quickcheck;
    use serde_json::json;

    #[derive(Serialize, Clone, Debug)]
    struct MyEvent {
        field_one: bool,
        field_two: u32,
    }

    impl Arbitrary for MyEvent {
        fn arbitrary(g: &mut Gen) -> Self {
            Self {
                field_one: bool::arbitrary(g),
                field_two: u32::arbitrary(g),
            }
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
    struct ValidString(String);

    impl Arbitrary for ValidString {
        fn arbitrary(g: &mut Gen) -> Self {
            loop {
                let s = String::arbitrary(g);
                if !is_inaccurately_counted_bytes(s.as_bytes()) {
                    return Self(s);
                }
            }
        }
    }

    impl std::ops::Deref for ValidString {
        type Target = String;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    #[test]
    fn test_struct() {
        #[derive(Serialize)]
        struct Test {
            int: u32,
            seq: Vec<&'static str>,
        }

        let test = Test {
            int: 1,
            seq: vec!["a", "b"],
        };
        let expected = r#"{"int":1,"seq":["a","b"]}"#;
        assert_eq!(estimated_size_of(&test), expected.len());
    }

    #[test]
    fn test_enum() {
        #[derive(Serialize)]
        enum E {
            Unit,
            Newtype(u32),
            Tuple(u32, u32),
            Struct { a: u32 },
        }

        let u = E::Unit;
        let expected = r#""Unit""#;
        assert_eq!(estimated_size_of(&u), expected.len());

        let n = E::Newtype(1);
        let expected = r#"{"Newtype":1}"#;
        assert_eq!(estimated_size_of(&n), expected.len());

        let t = E::Tuple(1, 2);
        let expected = r#"{"Tuple":[1,2]}"#;
        assert_eq!(estimated_size_of(&t), expected.len());

        let s = E::Struct { a: 1 };
        let expected = r#"{"Struct":{"a":1}}"#;
        assert_eq!(estimated_size_of(&s), expected.len());
    }

    #[quickcheck]
    fn serialize_i8(v: i8) -> bool {
        let got = estimated_size_of(&v);
        let want = serde_json::to_string(&v).unwrap();

        got == want.len()
    }

    #[quickcheck]
    fn serialize_i16(v: i16) -> bool {
        let got = estimated_size_of(&v);
        let want = serde_json::to_string(&v).unwrap();

        got == want.len()
    }

    #[quickcheck]
    fn serialize_i32(v: i32) -> bool {
        let got = estimated_size_of(&v);
        let want = serde_json::to_string(&v).unwrap();

        got == want.len()
    }

    #[quickcheck]
    fn serialize_i64(v: i64) -> bool {
        let got = estimated_size_of(&v);
        let want = serde_json::to_string(&v).unwrap();

        got == want.len()
    }

    #[quickcheck]
    fn serialize_isize(v: isize) -> bool {
        let got = estimated_size_of(&v);
        let want = serde_json::to_string(&v).unwrap();

        got == want.len()
    }

    #[quickcheck]
    fn serialize_u8(v: u8) -> bool {
        let got = estimated_size_of(&v);
        let want = serde_json::to_string(&v).unwrap();

        got == want.len()
    }

    #[quickcheck]
    fn serialize_u16(v: u16) -> bool {
        let got = estimated_size_of(&v);
        let want = serde_json::to_string(&v).unwrap();

        got == want.len()
    }

    #[quickcheck]
    fn serialize_u32(v: u32) -> bool {
        let got = estimated_size_of(&v);
        let want = serde_json::to_string(&v).unwrap();

        got == want.len()
    }

    #[quickcheck]
    fn serialize_u64(v: u64) -> bool {
        let got = estimated_size_of(&v);
        let want = serde_json::to_string(&v).unwrap();

        got == want.len()
    }

    #[quickcheck]
    fn serialize_usize(v: usize) -> bool {
        let got = estimated_size_of(&v);
        let want = serde_json::to_string(&v).unwrap();

        got == want.len()
    }

    #[quickcheck]
    fn serialize_f32(v: f32) -> bool {
        // floats are expected to be finite.
        if !v.is_finite() {
            return true;
        }

        // We need to convert the float to `serde_json::Value`, as both implementations use `ryu` to
        // quickly convert floating point numbers to decimal strings, which differs in the final
        // output compared to the default `Display` implementation of `f32`/`f64`.
        let v = json!(v);

        let got = estimated_size_of(&v);
        let want = serde_json::to_string(&v).unwrap();

        got == want.len()
    }

    #[quickcheck]
    fn serialize_f64(v: f64) -> bool {
        // floats are expected to be finite.
        if !v.is_finite() {
            return true;
        }

        // We need to convert the float to `serde_json::Value`, as both implementations use `ryu` to
        // quickly convert floating point numbers to decimal strings, which differs in the final
        // output compared to the default `Display` implementation of `f32`/`f64`.
        let v = json!(v);

        let got = estimated_size_of(&v);
        let want = serde_json::to_string(&v).unwrap();

        got == want.len()
    }

    #[quickcheck]
    fn serialize_char(v: char) -> TestResult {
        if is_inaccurately_counted_bytes(&[v as u8]) {
            return TestResult::discard();
        }

        let got = estimated_size_of(&v);
        let want = serde_json::to_string(&v).unwrap();

        TestResult::from_bool(got == want.len())
    }

    #[quickcheck]
    fn serialize_str(v: String) -> TestResult {
        if is_inaccurately_counted_bytes(v.as_bytes()) {
            return TestResult::discard();
        }

        let got = estimated_size_of(&v);
        let want = serde_json::to_string(&v).unwrap();

        TestResult::from_bool(got == want.len())
    }

    #[quickcheck]
    fn serialize_bytes(v: Vec<u8>) -> TestResult {
        if is_inaccurately_counted_bytes(&v) {
            return TestResult::discard();
        }

        let got = estimated_size_of(&v);
        let want = serde_json::to_string(&v).unwrap();

        TestResult::from_bool(got == want.len())
    }

    #[quickcheck]
    fn serialize_option(v: Option<bool>) -> bool {
        let got = estimated_size_of(&v);
        let want = serde_json::to_string(&v).unwrap();

        got == want.len()
    }

    #[quickcheck]
    fn serialize_seq(v: Vec<bool>) -> bool {
        let got = estimated_size_of(&v);
        let want = serde_json::to_string(&v).unwrap();

        got == want.len()
    }

    #[quickcheck]
    fn serialize_map(v: HashMap<ValidString, bool>) -> TestResult {
        let got = estimated_size_of(&v);
        let want = serde_json::to_string(&v).unwrap();

        TestResult::from_bool(got == want.len())
    }

    #[quickcheck]
    fn serialize_struct(v: MyEvent) -> bool {
        let got = estimated_size_of(&v);
        let want = serde_json::to_string(&v).unwrap();

        got == want.len()
    }

    #[quickcheck]
    fn serialize_json_encoded_byte_counting_value(v: Value) -> TestResult {
        if is_inaccurately_counted_value(&v) {
            return TestResult::discard();
        }

        let b = JsonEncodedByteCountingValue(&v);

        let got = estimated_size_of(&b);
        let want = serde_json::to_string(&v).unwrap();

        TestResult::from_bool(got == want.len())
    }

    fn is_inaccurately_counted_value(v: &Value) -> bool {
        match v {
            Value::Bytes(v) => is_inaccurately_counted_bytes(v),
            Value::Object(v) => v.iter().any(|(k, v)| {
                is_inaccurately_counted_bytes(k.as_bytes()) || is_inaccurately_counted_value(v)
            }),
            Value::Array(v) => v.iter().any(is_inaccurately_counted_value),
            _ => false,
        }
    }

    // Some strings are known to report invalid sizes with the byte size counting serializer. This
    // is done for performance reasons.
    fn is_inaccurately_counted_bytes<'a>(
        v: impl IntoIterator<Item = &'a u8> + std::fmt::Debug + Clone,
    ) -> bool {
        // Taken from `serde_json`
        const BB: u8 = b'b'; // \x08
        const TT: u8 = b't'; // \x09
        const NN: u8 = b'n'; // \x0A
        const FF: u8 = b'f'; // \x0C
        const RR: u8 = b'r'; // \x0D
        const QU: u8 = b'"'; // \x22
        const BS: u8 = b'\\'; // \x5C
        const UU: u8 = b'u'; // \x00...\x1F except the ones above
        const __: u8 = 0;

        // Lookup table of escape sequences. A value of b'x' at index i means that byte
        // i is escaped as "\x" in JSON. A value of 0 means that byte i is not escaped.
        static ESCAPE: [u8; 256] = [
            //   1   2   3   4   5   6   7   8   9   A   B   C   D   E   F
            UU, UU, UU, UU, UU, UU, UU, UU, BB, TT, NN, UU, FF, RR, UU, UU, // 0
            UU, UU, UU, UU, UU, UU, UU, UU, UU, UU, UU, UU, UU, UU, UU, UU, // 1
            __, __, QU, __, __, __, __, __, __, __, __, __, __, __, __, __, // 2
            __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // 3
            __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // 4
            __, __, __, __, __, __, __, __, __, __, __, __, BS, __, __, __, // 5
            __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // 6
            __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // 7
            __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // 8
            __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // 9
            __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // A
            __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // B
            __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // C
            __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // D
            __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // E
            __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // F
        ];

        v.clone().into_iter().any(|b| ESCAPE[*b as usize] != 0)
            || String::from_utf8(v.into_iter().copied().collect()).is_err()
    }
}
