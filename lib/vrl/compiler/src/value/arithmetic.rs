use std::{collections::BTreeMap, convert::TryFrom};

use super::convert::VrlValueConvert;
use super::Error;
use crate::value::Kind;
use crate::ExpressionError;
use ::value::Value;

pub trait VrlValueArithmetic: Sized {
    /// Similar to [`std::ops::Mul`], but fallible (e.g. `TryMul`).
    fn try_mul(self, rhs: Self) -> Result<Self, Error>;

    /// Similar to [`std::ops::Div`], but fallible (e.g. `TryDiv`).
    fn try_div(self, rhs: Self) -> Result<Self, Error>;

    /// Similar to [`std::ops::Add`], but fallible (e.g. `TryAdd`).
    fn try_add(self, rhs: Self) -> Result<Self, Error>;

    /// Similar to [`std::ops::Sub`], but fallible (e.g. `TrySub`).
    fn try_sub(self, rhs: Self) -> Result<Self, Error>;

    /// Try to "OR" (`||`) two values types.
    ///
    /// If the lhs value is `null` or `false`, the rhs is evaluated and
    /// returned. The rhs is a closure that can return an error, and thus this
    /// method can return an error as well.
    fn try_or(self, rhs: impl FnMut() -> Result<Self, ExpressionError>) -> Result<Self, Error>;

    /// Try to "AND" (`&&`) two values types.
    ///
    /// A lhs or rhs value of `Null` returns `false`.
    fn try_and(self, rhs: Self) -> Result<Self, Error>;

    /// Similar to [`std::ops::Rem`], but fallible (e.g. `TryRem`).
    fn try_rem(self, rhs: Self) -> Result<Self, Error>;

    /// Similar to [`std::cmp::Ord`], but fallible (e.g. `TryOrd`).
    fn try_gt(self, rhs: Self) -> Result<Self, Error>;

    /// Similar to [`std::cmp::Ord`], but fallible (e.g. `TryOrd`).
    fn try_ge(self, rhs: Self) -> Result<Self, Error>;

    /// Similar to [`std::cmp::Ord`], but fallible (e.g. `TryOrd`).
    fn try_lt(self, rhs: Self) -> Result<Self, Error>;

    /// Similar to [`std::cmp::Ord`], but fallible (e.g. `TryOrd`).
    fn try_le(self, rhs: Self) -> Result<Self, Error>;

    fn try_merge(self, rhs: Self) -> Result<Self, Error>;

    /// Similar to [`std::cmp::Eq`], but does a lossless comparison for integers
    /// and floats.
    fn eq_lossy(&self, rhs: &Self) -> bool;
}

impl VrlValueArithmetic for Value {
    fn try_mul(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Mul(self.vrl_kind(), rhs.vrl_kind());

        let value = match self {
            Value::Integer(lhv) if rhs.is_bytes() => rhs.try_bytes()?.repeat(lhv as usize).into(),
            Value::Integer(lhv) if rhs.is_float() => {
                Value::try_from_f64(lhv as f64 * rhs.try_float()?)?
            }
            Value::Integer(lhv) => (lhv * rhs.as_int().ok_or_else(err)?).into(),
            Value::Float(lhv) => (lhv * rhs.as_float().ok_or_else(err)?).into(),
            Value::Bytes(lhv) if rhs.is_integer() => lhv.repeat(rhs.try_integer()? as usize).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    fn try_div(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Div(self.vrl_kind(), rhs.vrl_kind());

        let rhv = rhs.as_float().ok_or_else(err)?;

        if rhv == 0.0 {
            return Err(Error::DivideByZero);
        }

        let value = match self {
            Value::Integer(lhv) => Value::try_from_f64(lhv as f64 / rhv.into_inner())?,
            Value::Float(lhv) => (lhv / rhv).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    fn try_add(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Add(self.vrl_kind(), rhs.vrl_kind());

        let value = match self {
            Value::Integer(lhv) if rhs.is_float() => {
                Value::try_from_f64(lhv as f64 + rhs.try_float()?)?
            }
            Value::Integer(lhv) => (lhv + rhs.as_int().ok_or_else(err)?).into(),
            Value::Float(lhv) => (lhv + rhs.as_float().ok_or_else(err)?).into(),
            Value::Bytes(_) if rhs.is_null() => self,
            Value::Bytes(_) if rhs.is_bytes() => format!(
                "{}{}",
                self.try_bytes_utf8_lossy()?,
                rhs.try_bytes_utf8_lossy()?,
            )
            .into(),
            Value::Null if rhs.is_bytes() => rhs,
            _ => return Err(err()),
        };

        Ok(value)
    }

    fn try_sub(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Sub(self.vrl_kind(), rhs.vrl_kind());

        let value = match self {
            Value::Integer(lhv) if rhs.is_float() => {
                Value::try_from_f64(lhv as f64 - rhs.try_float()?)?
            }
            Value::Integer(lhv) => (lhv - rhs.as_int().ok_or_else(err)?).into(),
            Value::Float(lhv) => (lhv - rhs.as_float().ok_or_else(err)?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    fn try_or(self, mut rhs: impl FnMut() -> Result<Self, ExpressionError>) -> Result<Self, Error> {
        let err = Error::Or;

        match self {
            Value::Null => rhs().map_err(err),
            Value::Boolean(false) => rhs().map_err(err),
            value => Ok(value),
        }
    }

    fn try_and(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::And(self.vrl_kind(), rhs.vrl_kind());

        let value = match self {
            Value::Null => false.into(),
            Value::Boolean(lhv) => match rhs {
                Value::Null => false.into(),
                Value::Boolean(rhv) => (lhv && rhv).into(),
                _ => return Err(err()),
            },
            _ => return Err(err()),
        };

        Ok(value)
    }

    fn try_rem(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Rem(self.vrl_kind(), rhs.vrl_kind());

        let value = match self {
            Value::Integer(lhv) if rhs.is_float() => {
                Value::try_from_f64(lhv as f64 % rhs.try_float()?)?
            }
            Value::Integer(lhv) => (lhv % rhs.as_int().ok_or_else(err)?).into(),
            Value::Float(lhv) => (lhv % rhs.as_float().ok_or_else(err)?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    fn try_gt(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Rem(self.vrl_kind(), rhs.vrl_kind());

        let value = match self {
            Value::Integer(lhv) if rhs.is_float() => (lhv as f64 > rhs.try_float()?).into(),
            Value::Integer(lhv) => (lhv > rhs.as_int().ok_or_else(err)?).into(),
            Value::Float(lhv) => (lhv > rhs.as_float().ok_or_else(err)?).into(),
            Value::Bytes(lhv) => (lhv > rhs.try_bytes()?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    fn try_ge(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Ge(self.vrl_kind(), rhs.vrl_kind());

        let value = match self {
            Value::Integer(lhv) if rhs.is_float() => (lhv as f64 >= rhs.try_float()?).into(),
            Value::Integer(lhv) => (lhv >= rhs.as_int().ok_or_else(err)?).into(),
            Value::Float(lhv) => (lhv >= rhs.as_float().ok_or_else(err)?).into(),
            Value::Bytes(lhv) => (lhv >= rhs.try_bytes()?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    fn try_lt(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Ge(self.vrl_kind(), rhs.vrl_kind());

        let value = match self {
            Value::Integer(lhv) if rhs.is_float() => ((lhv as f64) < rhs.try_float()?).into(),
            Value::Integer(lhv) => (lhv < rhs.as_int().ok_or_else(err)?).into(),
            Value::Float(lhv) => (lhv < rhs.as_float().ok_or_else(err)?).into(),
            Value::Bytes(lhv) => (lhv < rhs.try_bytes()?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    fn try_le(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Ge(self.vrl_kind(), rhs.vrl_kind());

        let value = match self {
            Value::Integer(lhv) if rhs.is_float() => (lhv as f64 <= rhs.try_float()?).into(),
            Value::Integer(lhv) => (lhv <= rhs.as_int().ok_or_else(err)?).into(),
            Value::Float(lhv) => (lhv <= rhs.as_float().ok_or_else(err)?).into(),
            Value::Bytes(lhv) => (lhv <= rhs.try_bytes()?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    fn try_merge(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Merge(Kind::from(&self), Kind::from(&rhs));

        let value = match (&self, &rhs) {
            (Value::Map(lhv), Value::Map(rhv)) => lhv
                .iter()
                .chain(rhv.iter())
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<BTreeMap<String, Value>>()
                .into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    fn eq_lossy(&self, rhs: &Self) -> bool {
        use Value::*;

        match self {
            Integer(lhv) => rhs
                .as_float()
                .map(|rhv| (*lhv as f64) == rhv.into_inner())
                .unwrap_or(false),
            Float(lhv) => rhs.as_float().map(|rhv| *lhv == rhv).unwrap_or(false),
            _ => self == rhs,
        }
    }
}
