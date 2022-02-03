use std::{collections::BTreeMap, convert::TryFrom};

use super::{Error, Value};
use crate::ExpressionError;

impl Value {
    /// Similar to [`std::ops::Mul`], but fallible (e.g. `TryMul`).
    pub fn try_mul(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Mul(self.kind(), rhs.kind());

        // When multiplying a string by an integer, if the number is negative we set it to zero to
        // return an empty string.
        let as_usize = |num| if num < 0 { 0 } else { num as usize };

        let value = match self {
            Value::Integer(lhv) if rhs.is_bytes() => rhs.try_bytes()?.repeat(as_usize(lhv)).into(),
            Value::Integer(lhv) if rhs.is_float() => (lhv as f64 * rhs.try_float()?).into(),
            Value::Integer(lhv) => (lhv * i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv * f64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Bytes(lhv) if rhs.is_integer() => {
                lhv.repeat(as_usize(rhs.try_integer()?)).into()
            }
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::ops::Div`], but fallible (e.g. `TryDiv`).
    pub fn try_div(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Div(self.kind(), rhs.kind());

        let rhv = f64::try_from(&rhs).map_err(|_| err())?;

        if rhv == 0.0 {
            return Err(Error::DivideByZero);
        }

        let value = match self {
            Value::Integer(lhv) => (lhv as f64 / rhv).into(),
            Value::Float(lhv) => (lhv.into_inner() / rhv).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::ops::Add`], but fallible (e.g. `TryAdd`).
    pub fn try_add(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Add(self.kind(), rhs.kind());

        let value = match self {
            Value::Integer(lhv) if rhs.is_float() => (lhv as f64 + rhs.try_float()?).into(),
            Value::Integer(lhv) => (lhv + i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv + f64::try_from(&rhs).map_err(|_| err())?).into(),
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

    /// Similar to [`std::ops::Sub`], but fallible (e.g. `TrySub`).
    pub fn try_sub(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Sub(self.kind(), rhs.kind());

        let value = match self {
            Value::Integer(lhv) if rhs.is_float() => (lhv as f64 - rhs.try_float()?).into(),
            Value::Integer(lhv) => (lhv - i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv - f64::try_from(&rhs).map_err(|_| err())?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Try to "OR" (`||`) two values types.
    ///
    /// If the lhs value is `null` or `false`, the rhs is evaluated and
    /// returned. The rhs is a closure that can return an error, and thus this
    /// method can return an error as well.
    pub fn try_or(
        self,
        mut rhs: impl FnMut() -> Result<Self, ExpressionError>,
    ) -> Result<Self, Error> {
        let err = Error::Or;

        match self {
            Value::Null => rhs().map_err(err),
            Value::Boolean(false) => rhs().map_err(err),
            value => Ok(value),
        }
    }

    /// Try to "AND" (`&&`) two values types.
    ///
    /// A lhs or rhs value of `Null` returns `false`.
    pub fn try_and(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::And(self.kind(), rhs.kind());

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

    /// Similar to [`std::ops::Rem`], but fallible (e.g. `TryRem`).
    pub fn try_rem(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Rem(self.kind(), rhs.kind());

        let value = match self {
            Value::Integer(lhv) if rhs.is_float() => (lhv as f64 % rhs.try_float()?).into(),
            Value::Integer(lhv) => (lhv % i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv % f64::try_from(&rhs).map_err(|_| err())?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::cmp::Ord`], but fallible (e.g. `TryOrd`).
    pub fn try_gt(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Rem(self.kind(), rhs.kind());

        let value = match self {
            Value::Integer(lhv) if rhs.is_float() => (lhv as f64 > rhs.try_float()?).into(),
            Value::Integer(lhv) => (lhv > i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => {
                (lhv.into_inner() > f64::try_from(&rhs).map_err(|_| err())?).into()
            }
            Value::Bytes(lhv) => (lhv > rhs.try_bytes()?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::cmp::Ord`], but fallible (e.g. `TryOrd`).
    pub fn try_ge(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Ge(self.kind(), rhs.kind());

        let value = match self {
            Value::Integer(lhv) if rhs.is_float() => (lhv as f64 >= rhs.try_float()?).into(),
            Value::Integer(lhv) => (lhv >= i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => {
                (lhv.into_inner() >= f64::try_from(&rhs).map_err(|_| err())?).into()
            }
            Value::Bytes(lhv) => (lhv >= rhs.try_bytes()?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::cmp::Ord`], but fallible (e.g. `TryOrd`).
    pub fn try_lt(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Ge(self.kind(), rhs.kind());

        let value = match self {
            Value::Integer(lhv) if rhs.is_float() => ((lhv as f64) < rhs.try_float()?).into(),
            Value::Integer(lhv) => (lhv < i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => {
                (lhv.into_inner() < f64::try_from(&rhs).map_err(|_| err())?).into()
            }
            Value::Bytes(lhv) => (lhv < rhs.try_bytes()?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::cmp::Ord`], but fallible (e.g. `TryOrd`).
    pub fn try_le(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Ge(self.kind(), rhs.kind());

        let value = match self {
            Value::Integer(lhv) if rhs.is_float() => (lhv as f64 <= rhs.try_float()?).into(),
            Value::Integer(lhv) => (lhv <= i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => {
                (lhv.into_inner() <= f64::try_from(&rhs).map_err(|_| err())?).into()
            }
            Value::Bytes(lhv) => (lhv <= rhs.try_bytes()?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    pub fn try_merge(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Merge(self.kind(), rhs.kind());

        let value = match (&self, &rhs) {
            (Value::Object(lhv), Value::Object(rhv)) => lhv
                .iter()
                .chain(rhv.iter())
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<BTreeMap<String, Value>>()
                .into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::cmp::Eq`], but does a lossless comparison for integers
    /// and floats.
    pub fn eq_lossy(&self, rhs: &Self) -> bool {
        use Value::*;

        match self {
            Integer(lhv) => f64::try_from(rhs)
                .map(|rhv| *lhv as f64 == rhv)
                .unwrap_or(false),

            Float(lhv) => f64::try_from(rhs)
                .map(|rhv| lhv.into_inner() == rhv)
                .unwrap_or(false),

            _ => self == rhs,
        }
    }
}
