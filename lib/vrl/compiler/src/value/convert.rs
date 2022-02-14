use std::{borrow::Cow, collections::BTreeMap};

use crate::value::{Kind, VrlValueError, VrlValueKind};
use crate::{expression::Expr, Expression};
use bytes::Bytes;
use chrono::{DateTime, Utc};

use value::kind::Collection;
use value::{Value, ValueRegex};

pub trait VrlValueConvert: Sized {
    /// Convert a given [`Value`] into a [`Expression`] trait object.
    fn into_expression(self) -> Box<dyn Expression>;

    fn try_integer(self) -> Result<i64, VrlValueError>;
    fn try_float(self) -> Result<f64, VrlValueError>;
    fn try_bytes(self) -> Result<Bytes, VrlValueError>;
    fn try_boolean(self) -> Result<bool, VrlValueError>;
    fn try_regex(self) -> Result<ValueRegex, VrlValueError>;
    fn try_null(self) -> Result<(), VrlValueError>;
    fn try_array(self) -> Result<Vec<Value>, VrlValueError>;
    fn try_object(self) -> Result<BTreeMap<String, Value>, VrlValueError>;
    fn try_timestamp(self) -> Result<DateTime<Utc>, VrlValueError>;

    fn try_into_i64(&self) -> Result<i64, VrlValueError>;
    fn try_into_f64(&self) -> Result<f64, VrlValueError>;

    fn try_bytes_utf8_lossy(&self) -> Result<Cow<'_, str>, VrlValueError>;
}

impl VrlValueConvert for Value {
    /// Convert a given [`Value`] into a [`Expression`] trait object.
    fn into_expression(self) -> Box<dyn Expression> {
        Box::new(Expr::from(self))
    }

    fn try_integer(self) -> Result<i64, VrlValueError> {
        match self {
            Value::Integer(v) => Ok(v),
            _ => Err(VrlValueError::Expected {
                got: self.kind(),
                expected: Kind::integer(),
            }),
        }
    }

    fn try_into_i64(self: &Value) -> Result<i64, VrlValueError> {
        match self {
            Value::Integer(v) => Ok(*v),
            Value::Float(v) => Ok(v.into_inner() as i64),
            _ => Err(VrlValueError::Coerce(self.kind(), Kind::integer())),
        }
    }

    fn try_float(self) -> Result<f64, VrlValueError> {
        match self {
            Value::Float(v) => Ok(v.into_inner()),
            _ => Err(VrlValueError::Expected {
                got: self.kind(),
                expected: Kind::float(),
            }),
        }
    }

    fn try_into_f64(&self) -> Result<f64, VrlValueError> {
        match self {
            Value::Integer(v) => Ok(*v as f64),
            Value::Float(v) => Ok(v.into_inner()),
            _ => Err(VrlValueError::Coerce(self.kind(), Kind::float())),
        }
    }

    fn try_bytes(self) -> Result<Bytes, VrlValueError> {
        match self {
            Value::Bytes(v) => Ok(v),
            _ => Err(VrlValueError::Expected {
                got: self.kind(),
                expected: Kind::bytes(),
            }),
        }
    }

    fn try_bytes_utf8_lossy(&self) -> Result<Cow<'_, str>, VrlValueError> {
        match self.as_bytes() {
            Some(bytes) => Ok(String::from_utf8_lossy(bytes)),
            None => Err(VrlValueError::Expected {
                got: self.kind(),
                expected: Kind::bytes(),
            }),
        }
    }

    fn try_boolean(self) -> Result<bool, VrlValueError> {
        match self {
            Value::Boolean(v) => Ok(v),
            _ => Err(VrlValueError::Expected {
                got: self.kind(),
                expected: Kind::boolean(),
            }),
        }
    }

    fn try_regex(self) -> Result<ValueRegex, VrlValueError> {
        match self {
            Value::Regex(v) => Ok(v),
            _ => Err(VrlValueError::Expected {
                got: self.kind(),
                expected: Kind::regex(),
            }),
        }
    }

    fn try_null(self) -> Result<(), VrlValueError> {
        match self {
            Value::Null => Ok(()),
            _ => Err(VrlValueError::Expected {
                got: self.kind(),
                expected: Kind::null(),
            }),
        }
    }

    fn try_array(self) -> Result<Vec<Value>, VrlValueError> {
        match self {
            Value::Array(v) => Ok(v),
            _ => Err(VrlValueError::Expected {
                got: self.kind(),
                expected: Kind::array(Collection::any()),
            }),
        }
    }

    fn try_object(self) -> Result<BTreeMap<String, Value>, VrlValueError> {
        match self {
            Value::Object(v) => Ok(v),
            _ => Err(VrlValueError::Expected {
                got: self.kind(),
                expected: Kind::object(Collection::any()),
            }),
        }
    }

    fn try_timestamp(self) -> Result<DateTime<Utc>, VrlValueError> {
        match self {
            Value::Timestamp(v) => Ok(v),
            _ => Err(VrlValueError::Expected {
                got: self.kind(),
                expected: Kind::timestamp(),
            }),
        }
    }
}

// impl From<Value> for VectorValue {
//     fn from(v: Value) -> Self {
//         match v {
//             Value::Bytes(v) => VectorValue::Bytes(v),
//             Value::Integer(v) => VectorValue::Integer(v),
//             Value::Float(v) => VectorValue::Float(v),
//             Value::Boolean(v) => VectorValue::Boolean(v),
//             Value::Object(v) => {
//                 VectorValue::Object(v.into_iter().map(|(k, v)| (k, v.into())).collect())
//             }
//             Value::Array(v) => VectorValue::Array(v.into_iter().map(Into::into).collect()),
//             Value::Timestamp(v) => VectorValue::Timestamp(v),
//             Value::Regex(v) => {
//                 VectorValue::Bytes(bytes::Bytes::copy_from_slice(v.to_string().as_bytes()))
//             }
//             Value::Null => VectorValue::Null,
//         }
//     }
// }
//
// impl From<VectorValue> for Value {
//     fn from(v: VectorValue) -> Self {
//         match v {
//             VectorValue::Bytes(v) => v.into(),
//             VectorValue::Regex(regex) => regex.into_inner().into(),
//             VectorValue::Integer(v) => v.into(),
//             VectorValue::Float(v) => v.into(),
//             VectorValue::Boolean(v) => v.into(),
//             VectorValue::Object(v) => {
//                 Value::Object(v.into_iter().map(|(k, v)| (k, v.into())).collect())
//             }
//             VectorValue::Array(v) => Value::Array(v.into_iter().map(Into::into).collect()),
//             VectorValue::Timestamp(v) => v.into(),
//             VectorValue::Null => Value::Null,
//         }
//     }
// }
