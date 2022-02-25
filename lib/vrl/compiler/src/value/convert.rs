use std::{borrow::Cow, collections::BTreeMap};

use crate::value::{Error, Kind};
use crate::{expression::Expr, Expression};
use bytes::Bytes;
use chrono::{DateTime, Utc};

use value::kind::Collection;
use value::{Value, ValueRegex};

pub trait VrlValueConvert: Sized {
    /// Convert a given [`Value`] into a [`Expression`] trait object.
    fn into_expression(self) -> Box<dyn Expression>;

    fn try_integer(self) -> Result<i64, Error>;
    fn try_float(self) -> Result<f64, Error>;
    fn try_bytes(self) -> Result<Bytes, Error>;
    fn try_boolean(self) -> Result<bool, Error>;
    fn try_regex(self) -> Result<ValueRegex, Error>;
    fn try_null(self) -> Result<(), Error>;
    fn try_array(self) -> Result<Vec<Value>, Error>;
    fn try_object(self) -> Result<BTreeMap<String, Value>, Error>;
    fn try_timestamp(self) -> Result<DateTime<Utc>, Error>;

    fn try_into_i64(&self) -> Result<i64, Error>;
    fn try_into_f64(&self) -> Result<f64, Error>;

    fn try_bytes_utf8_lossy(&self) -> Result<Cow<'_, str>, Error>;
}

impl VrlValueConvert for Value {
    /// Convert a given [`Value`] into a [`Expression`] trait object.
    fn into_expression(self) -> Box<dyn Expression> {
        Box::new(Expr::from(self))
    }

    fn try_integer(self) -> Result<i64, Error> {
        match self {
            Value::Integer(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::integer(),
            }),
        }
    }

    fn try_into_i64(self: &Value) -> Result<i64, Error> {
        match self {
            Value::Integer(v) => Ok(*v),
            Value::Float(v) => Ok(v.into_inner() as i64),
            _ => Err(Error::Coerce(self.kind(), Kind::integer())),
        }
    }

    fn try_float(self) -> Result<f64, Error> {
        match self {
            Value::Float(v) => Ok(v.into_inner()),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::float(),
            }),
        }
    }

    fn try_into_f64(&self) -> Result<f64, Error> {
        match self {
            Value::Integer(v) => Ok(*v as f64),
            Value::Float(v) => Ok(v.into_inner()),
            _ => Err(Error::Coerce(self.kind(), Kind::float())),
        }
    }

    fn try_bytes(self) -> Result<Bytes, Error> {
        match self {
            Value::Bytes(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::bytes(),
            }),
        }
    }

    fn try_bytes_utf8_lossy(&self) -> Result<Cow<'_, str>, Error> {
        match self.as_bytes() {
            Some(bytes) => Ok(String::from_utf8_lossy(bytes)),
            None => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::bytes(),
            }),
        }
    }

    fn try_boolean(self) -> Result<bool, Error> {
        match self {
            Value::Boolean(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::boolean(),
            }),
        }
    }

    fn try_regex(self) -> Result<ValueRegex, Error> {
        match self {
            Value::Regex(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::regex(),
            }),
        }
    }

    fn try_null(self) -> Result<(), Error> {
        match self {
            Value::Null => Ok(()),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::null(),
            }),
        }
    }

    fn try_array(self) -> Result<Vec<Value>, Error> {
        match self {
            Value::Array(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::array(Collection::any()),
            }),
        }
    }

    fn try_object(self) -> Result<BTreeMap<String, Value>, Error> {
        match self {
            Value::Object(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::object(Collection::any()),
            }),
        }
    }

    fn try_timestamp(self) -> Result<DateTime<Utc>, Error> {
        match self {
            Value::Timestamp(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::timestamp(),
            }),
        }
    }
}
