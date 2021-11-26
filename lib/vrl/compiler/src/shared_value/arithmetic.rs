use crate::value::Error;
use crate::{ExpressionError, SharedValue, Value};
use ordered_float::NotNan;
use std::collections::BTreeMap;
use std::convert::TryFrom;

impl SharedValue {
    /// Similar to [`std::ops::Mul`], but fallible (e.g. `TryMul`).
    pub fn try_mul(&self, rhs: SharedValue) -> Result<Self, Error> {
        let lhs = self.borrow();
        let rhs = rhs.borrow();
        let err = || Error::Mul(lhs.kind(), rhs.kind());

        let value = match &*lhs {
            Value::Integer(lhv) if rhs.is_bytes() => {
                rhs.as_bytes().unwrap().repeat(*lhv as usize).into()
            }
            Value::Integer(lhv) if rhs.is_float() => (*lhv as f64 * rhs.as_float().unwrap()).into(),
            Value::Integer(lhv) => (lhv * i64::try_from(&*rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv * f64::try_from(&*rhs).map_err(|_| err())?).into(),
            Value::Bytes(lhv) if rhs.is_integer() => {
                lhv.repeat(rhs.as_integer().unwrap() as usize).into()
            }
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::ops::Div`], but fallible (e.g. `TryDiv`).
    pub fn try_div(&self, rhs: SharedValue) -> Result<Self, Error> {
        let lhs = self.borrow();
        let rhs = rhs.borrow();
        let err = || Error::Div(lhs.kind(), rhs.kind());

        let rhv = f64::try_from(&*rhs).map_err(|_| err())?;

        if rhv == 0.0 {
            return Err(Error::DivideByZero);
        }

        let value = match &*lhs {
            Value::Integer(lhv) => (*lhv as f64 / rhv).into(),
            Value::Float(lhv) => (lhv.into_inner() / rhv).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::ops::Add`], but fallible (e.g. `TryAdd`).
    pub fn try_add(&self, rhs: SharedValue) -> Result<Self, Error> {
        let lhs = self.borrow();
        let rhs_borrowed = rhs.borrow();

        let err = || Error::Add(lhs.kind(), rhs_borrowed.kind());

        let value = match &*lhs {
            Value::Integer(lhv) if rhs_borrowed.is_float() => SharedValue::from(Value::Float(
                NotNan::new(*lhv as f64 + rhs_borrowed.as_float().unwrap()).unwrap(),
            )),
            Value::Integer(lhv) => (lhv + i64::try_from(&*rhs_borrowed).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv + f64::try_from(&*rhs_borrowed).map_err(|_| err())?).into(),
            Value::Bytes(_) if rhs_borrowed.is_null() => self.clone(),
            Value::Bytes(_) if rhs_borrowed.is_bytes() => SharedValue::from(format!(
                "{}{}",
                lhs.try_bytes_utf8_lossy()?,
                rhs_borrowed.try_bytes_utf8_lossy()?,
            )),
            Value::Null if rhs_borrowed.is_bytes() => rhs.clone(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::ops::Sub`], but fallible (e.g. `TrySub`).
    pub fn try_sub(&self, rhs: SharedValue) -> Result<Self, Error> {
        let lhs = self.borrow();
        let rhs = rhs.borrow();

        let err = || Error::Sub(lhs.kind(), rhs.kind());

        let value = match &*lhs {
            Value::Integer(lhv) if rhs.is_float() => (*lhv as f64 - rhs.as_float().unwrap()).into(),
            Value::Integer(lhv) => (lhv - i64::try_from(&*rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv - f64::try_from(&*rhs).map_err(|_| err())?).into(),
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
        mut rhs: impl FnMut() -> Result<SharedValue, ExpressionError>,
    ) -> Result<Self, Error> {
        let lhs = self.borrow();

        let err = |err| Error::Or(err);

        match &*lhs {
            Value::Null => rhs().map_err(err),
            Value::Boolean(false) => rhs().map_err(err),
            _ => Ok(self.clone()),
        }
    }

    /// Try to "AND" (`&&`) two values types.
    ///
    /// A lhs or rhs value of `Null` returns `false`.
    pub fn try_and(self, rhs: Self) -> Result<Self, Error> {
        let lhs = self.borrow();
        let rhs = rhs.borrow();

        let err = || Error::And(lhs.kind(), rhs.kind());

        let value = match &*lhs {
            Value::Null => false.into(),
            Value::Boolean(lhv) => match &*rhs {
                Value::Null => false.into(),
                Value::Boolean(rhv) => (*lhv && *rhv).into(),
                _ => return Err(err()),
            },
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::ops::Rem`], but fallible (e.g. `TryRem`).
    pub fn try_rem(self, rhs: SharedValue) -> Result<Self, Error> {
        let lhs = self.borrow();
        let rhs = rhs.borrow();
        let err = || Error::Rem(lhs.kind(), rhs.kind());

        let value = match &*lhs {
            Value::Integer(lhv) if rhs.is_float() => (*lhv as f64 % rhs.try_float()?).into(),
            Value::Integer(lhv) => (lhv % i64::try_from(&*rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv % f64::try_from(&*rhs).map_err(|_| err())?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::cmp::Ord`], but fallible (e.g. `TryOrd`).
    pub fn try_gt(self, rhs: Self) -> Result<Self, Error> {
        let lhs = self.borrow();
        let rhs = rhs.borrow();
        let err = || Error::Rem(lhs.kind(), rhs.kind());

        let value = match &*lhs {
            Value::Integer(lhv) if rhs.is_float() => (*lhv as f64 > rhs.try_float()?).into(),
            Value::Integer(lhv) => (*lhv > i64::try_from(&*rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => {
                (lhv.into_inner() > f64::try_from(&*rhs).map_err(|_| err())?).into()
            }
            Value::Bytes(lhv) => (lhv > &rhs.try_bytes()?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::cmp::Ord`], but fallible (e.g. `TryOrd`).
    pub fn try_ge(self, rhs: Self) -> Result<Self, Error> {
        let lhs = self.borrow();
        let rhs = rhs.borrow();
        let err = || Error::Ge(lhs.kind(), rhs.kind());

        let value = match &*lhs {
            Value::Integer(lhv) if rhs.is_float() => (*lhv as f64 >= rhs.try_float()?).into(),
            Value::Integer(lhv) => (lhv >= &i64::try_from(&*rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => {
                (lhv.into_inner() >= f64::try_from(&*rhs).map_err(|_| err())?).into()
            }
            Value::Bytes(lhv) => (lhv >= &rhs.try_bytes()?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::cmp::Ord`], but fallible (e.g. `TryOrd`).
    pub fn try_lt(self, rhs: Self) -> Result<Self, Error> {
        let lhs = self.borrow();
        let rhs = rhs.borrow();
        let err = || Error::Ge(lhs.kind(), rhs.kind());

        let value = match &*lhs {
            Value::Integer(lhv) if rhs.is_float() => ((*lhv as f64) < rhs.try_float()?).into(),
            Value::Integer(lhv) => (lhv < &i64::try_from(&*rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => {
                (lhv.into_inner() < f64::try_from(&*rhs).map_err(|_| err())?).into()
            }
            Value::Bytes(lhv) => (lhv < &rhs.try_bytes()?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::cmp::Ord`], but fallible (e.g. `TryOrd`).
    pub fn try_le(self, rhs: Self) -> Result<Self, Error> {
        let lhs = self.borrow();
        let rhs = rhs.borrow();
        let err = || Error::Ge(lhs.kind(), rhs.kind());

        let value = match &*lhs {
            Value::Integer(lhv) if rhs.is_float() => (*lhv as f64 <= rhs.try_float()?).into(),
            Value::Integer(lhv) => (lhv <= &i64::try_from(&*rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => {
                (lhv.into_inner() <= f64::try_from(&*rhs).map_err(|_| err())?).into()
            }
            Value::Bytes(lhv) => (lhv <= &rhs.try_bytes()?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    pub fn try_merge(&self, rhs: SharedValue) -> Result<Self, Error> {
        let lhs = self.borrow();
        let rhs = rhs.borrow();
        let err = || Error::Merge(lhs.kind(), rhs.kind());

        let value = match (&*lhs, &*rhs) {
            (Value::Object(lhv), Value::Object(rhv)) => lhv
                .iter()
                .chain(rhv.iter())
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<BTreeMap<String, SharedValue>>()
                .into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::cmp::Eq`], but does a lossless comparison for integers
    /// and floats.
    pub fn eq_lossy(&self, rhs: SharedValue) -> bool {
        let lhs = self.borrow();
        let rhs = rhs.borrow();

        match &*lhs {
            Value::Integer(lhv) => f64::try_from(&*rhs)
                .map(|rhv| *lhv as f64 == rhv)
                .unwrap_or(false),

            Value::Float(lhv) => f64::try_from(&*rhs)
                .map(|rhv| lhv.into_inner() == rhv)
                .unwrap_or(false),

            _ => &*lhs == &*rhs,
        }
    }
}
