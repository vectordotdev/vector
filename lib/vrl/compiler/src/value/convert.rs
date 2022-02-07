use super::{Error, Kind, Value};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use ordered_float::NotNan;
use std::{borrow::Cow, collections::BTreeMap};
use value::ValueRegex;

/// conversions that should be added to `Value` but rely on things outside of the `value` crate
pub trait VrlValueConvert {
    fn try_bytes(self) -> Result<Bytes, Error>;
    // TODO: rename to "try_coerce_string_lossy"
    fn try_bytes_utf8_lossy(&self) -> Result<Cow<'_, str>, Error>;
    fn try_float(&self) -> Result<f64, Error>;
    fn try_integer(&self) -> Result<i64, Error>;
    fn try_boolean(self) -> Result<bool, Error>;
    fn try_timestamp(self) -> Result<DateTime<Utc>, Error>;
    fn try_object(self) -> Result<BTreeMap<String, Value>, Error>;
    fn try_array(self) -> Result<Vec<Value>, Error>;
    fn try_regex(self) -> Result<ValueRegex, Error>;
    fn try_from_f64(f: f64) -> Result<Value, Error> {
        let float = NotNan::new(f).map_err(|_| Error::NanFloat)?;
        Ok(Value::Float(float))
    }

    fn coerce_f64(&self) -> Result<f64, Error>;
    /// This was renamed from "kind" so it doesn't collide with the now existing "kind".
    /// This should be fixed when this is unified with the `value/kind` type
    fn vrl_kind(&self) -> Kind;
}

impl VrlValueConvert for Value {
    fn coerce_f64(&self) -> Result<f64, Error> {
        match self {
            Value::Integer(v) => Ok(*v as f64),
            Value::Float(v) => Ok(v.into_inner()),
            _ => Err(Error::Coerce(self.vrl_kind(), Kind::Float)),
        }
    }

    fn try_bytes(self) -> Result<Bytes, Error> {
        match self {
            Value::Bytes(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.vrl_kind(),
                expected: Kind::Bytes,
            }),
        }
    }

    fn try_bytes_utf8_lossy(&self) -> Result<Cow<'_, str>, Error> {
        match self.as_bytes() {
            Some(bytes) => Ok(String::from_utf8_lossy(bytes)),
            None => Err(Error::Expected {
                got: self.vrl_kind(),
                expected: Kind::Bytes,
            }),
        }
    }

    fn try_float(&self) -> Result<f64, Error> {
        match self {
            Value::Float(v) => Ok(v.into_inner()),
            _ => Err(Error::Expected {
                got: self.vrl_kind(),
                expected: Kind::Float,
            }),
        }
    }
    // <<<<<<< HEAD
    // =======
    // }
    //
    // impl From<NotNan<f64>> for Value {
    //     fn from(v: NotNan<f64>) -> Self {
    //         Value::Float(v)
    //     }
    // }
    //
    // impl TryFrom<&Value> for f64 {
    //     type Error = Error;
    //
    //     fn try_from(v: &Value) -> Result<Self, Self::Error> {
    //         match v {
    //             Value::Integer(v) => Ok(*v as f64),
    //             Value::Float(v) => Ok(v.into_inner()),
    //             _ => Err(Error::Coerce(v.kind(), Kind::Float)),
    //         }
    //     }
    // }
    //
    // // TODO: this exists to satisfy the `vector_common::Convert` utility.
    // //
    // // We'll have to fix that so that we can remove this impl.
    // impl From<f64> for Value {
    //     fn from(v: f64) -> Self {
    //         let v = if v.is_nan() { 0.0 } else { v };
    //
    //         Value::Float(NotNan::new(v).unwrap())
    //     }
    // }
    //
    // // impl TryFrom<f64> for Value {
    // //     type Error = Error;
    //
    // //     fn try_from(v: f64) -> Result<Self, Self::Error> {
    // //         Ok(Value::Float(NotNan::new(v).map_err(|_| Error::NanFloat)?))
    // //     }
    // // }
    //
    // // Value::Bytes ----------------------------------------------------------------
    //
    // impl Value {
    //     pub fn is_bytes(&self) -> bool {
    //         matches!(self, Value::Bytes(_))
    //     }
    //
    //     pub fn as_bytes(&self) -> Option<&Bytes> {
    //         match self {
    //             Value::Bytes(v) => Some(v),
    //             _ => None,
    //         }
    //     }
    // >>>>>>> jean/value-lib

    fn try_integer(&self) -> Result<i64, Error> {
        match self {
            Value::Integer(v) => Ok(*v),
            _ => Err(Error::Expected {
                got: self.vrl_kind(),
                expected: Kind::Integer,
            }),
        }
    }

    fn try_boolean(self) -> Result<bool, Error> {
        match self {
            Value::Boolean(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.vrl_kind(),
                expected: Kind::Boolean,
            }),
        }
    }

    fn try_timestamp(self) -> Result<DateTime<Utc>, Error> {
        match self {
            Value::Timestamp(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.vrl_kind(),
                expected: Kind::Timestamp,
            }),
        }
    }

    fn try_object(self) -> Result<BTreeMap<String, Value>, Error> {
        match self {
            Value::Map(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.vrl_kind(),
                expected: Kind::Object,
            }),
        }
    }

    fn try_array(self) -> Result<Vec<Value>, Error> {
        match self {
            Value::Array(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.vrl_kind(),
                expected: Kind::Array,
            }),
        }
    }

    fn try_regex(self) -> Result<ValueRegex, Error> {
        match self {
            Value::Regex(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.vrl_kind(),
                expected: Kind::Regex,
            }),
        }
    }

    fn vrl_kind(&self) -> Kind {
        Kind::from(self)
    }
}

// use value::Value;

// Value::Integer --------------------------------------------------------------

// impl Value {
//     pub fn is_integer(&self) -> bool {
//         matches!(self, Value::Integer(_))
//     }
//
//     pub fn as_integer(&self) -> Option<i64> {
//         match self {
//             Value::Integer(v) => Some(*v),
//             _ => None,
//         }
//     }
//

// }

// impl From<i8> for Value {
//     fn from(v: i8) -> Self {
//         Value::Integer(v as i64)
//     }
// }
//
// impl From<i16> for Value {
//     fn from(v: i16) -> Self {
//         Value::Integer(v as i64)
//     }
// }
//
// impl From<i32> for Value {
//     fn from(v: i32) -> Self {
//         Value::Integer(v as i64)
//     }
// }
//
// impl From<i64> for Value {
//     fn from(v: i64) -> Self {
//         Value::Integer(v)
//     }
// }
//
// impl From<u16> for Value {
//     fn from(v: u16) -> Self {
//         Value::Integer(v as i64)
//     }
// }
//
// impl From<u32> for Value {
//     fn from(v: u32) -> Self {
//         Value::Integer(v as i64)
//     }
// }
//
// impl From<u64> for Value {
//     fn from(v: u64) -> Self {
//         Value::Integer(v as i64)
//     }
// }
//
// impl From<usize> for Value {
//     fn from(v: usize) -> Self {
//         Value::Integer(v as i64)
//     }
// }

// impl TryFrom<&Value> for i64 {
//     type Error = Error;
//
//     fn try_from(v: &Value) -> Result<Self, Self::Error> {
//         match v {
//             Value::Integer(v) => Ok(*v),
//             Value::Float(v) => Ok(v.into_inner() as i64),
//             _ => Err(Error::Coerce(v.kind(), Kind::Integer)),
//         }
//     }
// }

// Value::Float ----------------------------------------------------------------

// impl Value {
//     pub fn is_float(&self) -> bool {
//         matches!(self, Value::Float(_))
//     }
//
//     pub fn as_float(&self) -> Option<f64> {
//         match self {
//             Value::Float(v) => Some(v.into_inner()),
//             _ => None,
//         }
//     }
//

// }

// impl From<NotNan<f64>> for Value {
//     fn from(v: NotNan<f64>) -> Self {
//         Value::Float(v)
//     }
// }
//
// impl TryFrom<&Value> for f64 {
//     type Error = Error;
//
//     fn try_from(v: &Value) -> Result<Self, Self::Error> {
//         match v {
//             Value::Integer(v) => Ok(*v as f64),
//             Value::Float(v) => Ok(v.into_inner()),
//             _ => Err(Error::Coerce(v.kind(), Kind::Float)),
//         }
//     }
// }

// // TODO: this exists to satisfy the `shared::Convert` utility.
// //
// // We'll have to fix that so that we can remove this impl.
// impl From<f64> for Value {
//     fn from(v: f64) -> Self {
//         let v = if v.is_nan() { 0.0 } else { v };
//
//         Value::Float(NotNan::new(v).unwrap())
//     }
// }

// impl TryFrom<f64> for Value {
//     type Error = Error;

//     fn try_from(v: f64) -> Result<Self, Self::Error> {
//         Ok(Value::Float(NotNan::new(v).map_err(|_| Error::NanFloat)?))
//     }
// }

// Value::Bytes ----------------------------------------------------------------

// impl Value {
//     pub fn is_bytes(&self) -> bool {
//         matches!(self, Value::Bytes(_))
//     }
//
//     pub fn as_bytes(&self) -> Option<&Bytes> {
//         match self {
//             Value::Bytes(v) => Some(v),
//             _ => None,
//         }
//     }
//

//

//

// }

// impl From<Bytes> for Value {
//     fn from(v: Bytes) -> Self {
//         Value::Bytes(v)
//     }
// }
//
// impl From<Cow<'_, str>> for Value {
//     fn from(v: Cow<'_, str>) -> Self {
//         v.as_ref().into()
//     }
// }
//
// impl From<Vec<u8>> for Value {
//     fn from(v: Vec<u8>) -> Self {
//         v.as_slice().into()
//     }
// }
//
// impl From<&[u8]> for Value {
//     fn from(v: &[u8]) -> Self {
//         Value::Bytes(Bytes::copy_from_slice(v))
//     }
// }
//
// impl From<String> for Value {
//     fn from(v: String) -> Self {
//         Value::Bytes(v.into())
//     }
// }
//
// impl From<&str> for Value {
//     fn from(v: &str) -> Self {
//         Value::Bytes(Bytes::copy_from_slice(v.as_bytes()))
//     }
// }

// Value::Boolean --------------------------------------------------------------

// impl Value {
//     pub fn is_boolean(&self) -> bool {
//         matches!(self, Value::Boolean(_))
//     }
//
//     pub fn as_boolean(&self) -> Option<bool> {
//         match self {
//             Value::Boolean(v) => Some(*v),
//             _ => None,
//         }
//     }
//

// }
//
// impl From<bool> for Value {
//     fn from(v: bool) -> Self {
//         Value::Boolean(v)
//     }
// }
//
// // Value::Regex ----------------------------------------------------------------
//
// impl Value {
//     pub fn is_regex(&self) -> bool {
//         matches!(self, Value::Regex(_))
//     }
//
//     pub fn as_regex(&self) -> Option<&Regex> {
//         match self {
//             Value::Regex(v) => Some(v),
//             _ => None,
//         }
//     }
//

// }
//
// impl From<Regex> for Value {
//     fn from(v: Regex) -> Self {
//         Value::Regex(v)
//     }
// }
//
// impl From<regex::Regex> for Value {
//     fn from(regex: regex::Regex) -> Self {
//         Value::Regex(regex.into())
//     }
// }
//
// // Value::Null -----------------------------------------------------------------
//
// impl Value {
//     pub fn is_null(&self) -> bool {
//         matches!(self, Value::Null)
//     }
//
//     pub fn as_null(&self) -> Option<()> {
//         match self {
//             Value::Null => Some(()),
//             _ => None,
//         }
//     }
//
//     pub fn try_null(self) -> Result<(), Error> {
//         match self {
//             Value::Null => Ok(()),
//             _ => Err(Error::Expected {
//                 got: self.kind(),
//                 expected: Kind::Null,
//             }),
//         }
//     }
// }
//

//
// impl<T: Into<Value>> From<Option<T>> for Value {
//     fn from(v: Option<T>) -> Self {
//         match v {
//             Some(v) => v.into(),
//             None => Value::Null,
//         }
//     }
// }
//
// Value::Array ----------------------------------------------------------------

// impl Value {

//
// // Value::Object ---------------------------------------------------------------
//
// impl Value {
//     pub fn is_object(&self) -> bool {
//         matches!(self,Value::Map(_))
//     }
//
//     pub fn as_object(&self) -> Option<&BTreeMap<String, Value>> {
//         match self {
//            Value::Map(v) => Some(v),
//             _ => None,
//         }
//     }
//
//     pub fn as_object_mut(&mut self) -> Option<&mut BTreeMap<String, Value>> {
//         match self {
//            Value::Map(v) => Some(v),
//             _ => None,
//         }
//     }
//

// }
//
// impl From<BTreeMap<String, Value>> for Value {
//     fn from(value: BTreeMap<String, Value>) -> Self {
//        Value::Map(value)
//     }
// }
//
// impl FromIterator<(String, Value)> for Value {
//     fn from_iter<I: IntoIterator<Item = (String, Value)>>(iter: I) -> Self {
//        Value::Map(iter.into_iter().collect::<BTreeMap<_, _>>())
//     }
// }
//
// // Value::Timestamp ------------------------------------------------------------
//
// impl Value {
//     pub fn is_timestamp(&self) -> bool {
//         matches!(self, Value::Timestamp(_))
//     }
//
//     pub fn as_timestamp(&self) -> Option<&DateTime<Utc>> {
//         match self {
//             Value::Timestamp(v) => Some(v),
//             _ => None,
//         }
//     }
//

// }
//
// impl From<DateTime<Utc>> for Value {
//     fn from(v: DateTime<Utc>) -> Self {
//         Value::Timestamp(v)
//     }
// }
