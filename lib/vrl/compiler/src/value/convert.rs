use std::{borrow::Cow, collections::BTreeMap};

use bytes::Bytes;
use chrono::{DateTime, Utc};
use value::kind::Collection;
use value::{Value, ValueRegex};

use crate::expression::{Container, Variant};
use crate::value::{Error, Kind};
use crate::{expression::Expr, Expression};

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

    fn try_chars_iter(&self) -> Result<Chars, Error>;

    fn try_as_bytes(&self) -> Result<&Bytes, Error>;
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

    fn try_chars_iter(&self) -> Result<Chars, Error> {
        let bytes = self.as_bytes().ok_or(Error::Expected {
            got: self.kind(),
            expected: Kind::bytes(),
        })?;

        Ok(Chars::new(bytes))
    }

    fn try_as_bytes(&self) -> Result<&Bytes, Error> {
        match self {
            Value::Bytes(v) => Ok(v),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::bytes(),
            }),
        }
    }
}

impl<'a> VrlValueConvert for Cow<'a, Value> {
    /// Convert a given [`Value`] into a [`Expression`] trait object.
    fn into_expression(self) -> Box<dyn Expression> {
        Box::new(Expr::from(self.into_owned()))
    }

    fn try_integer(self) -> Result<i64, Error> {
        match self.as_ref() {
            Value::Integer(v) => Ok(*v),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::integer(),
            }),
        }
    }

    fn try_into_i64(&self) -> Result<i64, Error> {
        match self.as_ref() {
            Value::Integer(v) => Ok(*v),
            Value::Float(v) => Ok(v.into_inner() as i64),
            _ => Err(Error::Coerce(self.kind(), Kind::integer())),
        }
    }

    fn try_float(self) -> Result<f64, Error> {
        match self.as_ref() {
            Value::Float(v) => Ok(v.into_inner()),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::float(),
            }),
        }
    }

    fn try_into_f64(&self) -> Result<f64, Error> {
        match self.as_ref() {
            Value::Integer(v) => Ok(*v as f64),
            Value::Float(v) => Ok(v.into_inner()),
            _ => Err(Error::Coerce(self.kind(), Kind::float())),
        }
    }

    fn try_bytes(self) -> Result<Bytes, Error> {
        match self.as_ref() {
            Value::Bytes(v) => Ok(v.clone()),
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
        match self.as_ref() {
            Value::Boolean(v) => Ok(*v),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::boolean(),
            }),
        }
    }

    fn try_regex(self) -> Result<ValueRegex, Error> {
        match self.as_ref() {
            Value::Regex(v) => Ok(v.clone()),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::regex(),
            }),
        }
    }

    fn try_null(self) -> Result<(), Error> {
        match self.as_ref() {
            Value::Null => Ok(()),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::null(),
            }),
        }
    }

    fn try_array(self) -> Result<Vec<Value>, Error> {
        match self.as_ref() {
            Value::Array(v) => Ok(v.clone()),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::array(Collection::any()),
            }),
        }
    }

    fn try_object(self) -> Result<BTreeMap<String, Value>, Error> {
        match self.as_ref() {
            Value::Object(v) => Ok(v.clone()),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::object(Collection::any()),
            }),
        }
    }

    fn try_timestamp(self) -> Result<DateTime<Utc>, Error> {
        match self.as_ref() {
            Value::Timestamp(v) => Ok(*v),
            _ => Err(Error::Expected {
                got: self.kind(),
                expected: Kind::timestamp(),
            }),
        }
    }

    fn try_chars_iter(&self) -> Result<Chars, Error> {
        self.as_ref().try_chars_iter()
    }

    fn try_as_bytes(&self) -> Result<&Bytes, Error> {
        self.as_ref().try_as_bytes()
    }
}

pub struct Chars<'a> {
    bytes: &'a Bytes,
    pos: usize,
}

impl<'a> Chars<'a> {
    pub fn new(bytes: &'a Bytes) -> Self {
        Self { bytes, pos: 0 }
    }
}

impl<'a> Iterator for Chars<'a> {
    type Item = std::result::Result<char, u8>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.bytes.len() {
            return None;
        }

        let width = utf8_width::get_width(self.bytes[self.pos]);
        if width == 1 {
            self.pos += 1;
            Some(Ok(self.bytes[self.pos - 1] as char))
        } else {
            let c = std::str::from_utf8(&self.bytes[self.pos..self.pos + width]);
            match c {
                Ok(chr) => {
                    self.pos += width;
                    Some(Ok(chr.chars().next().unwrap()))
                }
                Err(_) => {
                    self.pos += 1;
                    Some(Err(self.bytes[self.pos]))
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.bytes.len()))
    }
}

/// Converts from an `Expr` into a `Value`. This is only possible if the expression represents
/// static values - `Literal`s and `Container`s containing `Literal`s.
/// The error returns the expression back so it can be used in the error report.
impl TryFrom<Expr> for Value {
    type Error = Expr;

    fn try_from(expr: Expr) -> Result<Self, Self::Error> {
        match expr {
            #[cfg(feature = "expr-literal")]
            Expr::Literal(literal) => Ok(literal.to_value()),
            Expr::Container(Container {
                variant: Variant::Object(object),
            }) => Ok(Value::Object(
                object
                    .iter()
                    .map(|(key, value)| Ok((key.clone(), value.clone().try_into()?)))
                    .collect::<Result<_, Self::Error>>()?,
            )),
            Expr::Container(Container {
                variant: Variant::Array(array),
            }) => Ok(Value::Array(
                array
                    .iter()
                    .map(|value| value.clone().try_into())
                    .collect::<Result<_, _>>()?,
            )),
            expr => Err(expr),
        }
    }
}
