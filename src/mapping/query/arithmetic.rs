use super::query_value::QueryValue;
use super::Function;
use crate::{
    event::{Event, Value},
    log_event,
    mapping::Result,
};
use bytes::BytesMut;

#[derive(Debug, Clone)]
pub(in crate::mapping) enum Operator {
    Multiply,
    Divide,
    Modulo,
    Add,
    Subtract,
    Equal,
    NotEqual,
    Greater,
    GreaterOrEqual,
    Less,
    LessOrEqual,
    And,
    Or,
}

#[derive(Debug)]
pub(in crate::mapping) struct Arithmetic {
    left: Box<dyn Function>,
    right: Box<dyn Function>,
    op: Operator,
}

impl Arithmetic {
    pub(in crate::mapping) fn new(
        left: Box<dyn Function>,
        right: Box<dyn Function>,
        op: Operator,
    ) -> Self {
        Self { left, right, op }
    }
}

/// If either value is a floating point number type and the other value is an
/// integer type the integer is "degraded" into a float. This allows us to
/// perform arithmetic on common values, but if both are integers then their
/// precision is preserved.
fn coerce_number_types(left: Value, right: Value) -> (Value, Value) {
    match (&left, &right) {
        (Value::Float(lf), Value::Integer(ri)) => (Value::Float(*lf), Value::Float(*ri as f64)),
        (Value::Integer(li), Value::Float(rf)) => (Value::Float(*li as f64), Value::Float(*rf)),
        _ => (left, right),
    }
}

// Degrades non-float numerical types into floats for the purposes of convenient
// boolean comparison.
fn compare_number_types(
    left: Value,
    right: Value,
    compare_fn: &dyn Fn(f64, f64) -> bool,
) -> Result<Value> {
    match coerce_number_types(left, right) {
        (Value::Integer(li), Value::Integer(ri)) => {
            Ok(Value::Boolean(compare_fn(li as f64, ri as f64)))
        }
        (Value::Float(lf), Value::Float(rf)) => Ok(Value::Boolean(compare_fn(lf, rf))),
        (l, r) => Err(format!(
            "unable to numerically compare field types {:?} and {:?}",
            l, r
        )),
    }
}

impl Function for Arithmetic {
    fn execute(&self, ctx: &Event) -> Result<QueryValue> {
        let left = match self.left.execute(ctx)? {
            QueryValue::Value(value) => value,
            query => {
                return Err(format!(
                    "arithmetic can not be performed with {}",
                    query.kind()
                ))
            }
        };
        let right = match self.right.execute(ctx)? {
            QueryValue::Value(value) => value,
            query => {
                return Err(format!(
                    "arithmetic can not be performed with {}",
                    query.kind()
                ))
            }
        };

        // TODO: A lot of these comparisons could potentially be baked into the
        // Value type. However, we would need to agree on general rules as to
        // how different types are compared.

        Ok(match self.op {
            Operator::Multiply => {
                let (left, right) = coerce_number_types(left, right);
                match left {
                    Value::Float(fl) => match right {
                        Value::Float(fr) => Value::Float(fl * fr),
                        vr => {
                            return Err(format!(
                                "unable to multiply right-hand field type {:?}",
                                vr
                            ))
                        }
                    },
                    Value::Integer(il) => match right {
                        Value::Integer(ir) => Value::Integer(il * ir),
                        vr => {
                            return Err(format!(
                                "unable to multiply right-hand field type {:?}",
                                vr
                            ))
                        }
                    },
                    vl => return Err(format!("unable to multiply left-hand field type {:?}", vl)),
                }
            }

            Operator::Divide => {
                let (left, right) = coerce_number_types(left, right);
                match left {
                    Value::Float(fl) => match right {
                        Value::Float(fr) => Value::Float(fl / fr),
                        vr => {
                            return Err(format!("unable to divide right-hand field type {:?}", vr))
                        }
                    },
                    Value::Integer(il) => match right {
                        Value::Integer(ir) => Value::Float(il as f64 / ir as f64),
                        vr => {
                            return Err(format!("unable to divide right-hand field type {:?}", vr))
                        }
                    },
                    vl => return Err(format!("unable to divide left-hand field type {:?}", vl)),
                }
            }

            Operator::Modulo => match left {
                Value::Integer(il) => match right {
                    Value::Integer(ir) => Value::Integer(il % ir),
                    vr => return Err(format!("unable to modulo right-hand field type {:?}", vr)),
                },
                vl => return Err(format!("unable to modulo left-hand field type {:?}", vl)),
            },

            Operator::Add => {
                let (left, right) = coerce_number_types(left, right);
                match left {
                    Value::Float(fl) => match right {
                        Value::Float(fr) => Value::Float(fl + fr),
                        vr => return Err(format!("unable to add right-hand field type {:?}", vr)),
                    },
                    Value::Integer(il) => match right {
                        Value::Integer(ir) => Value::Integer(il + ir),
                        vr => return Err(format!("unable to add right-hand field type {:?}", vr)),
                    },
                    Value::Bytes(sl) => match right {
                        Value::Bytes(sr) => {
                            let mut buf = BytesMut::with_capacity(sl.len() + sr.len());
                            buf.extend_from_slice(&sl);
                            buf.extend_from_slice(&sr);
                            Value::Bytes(buf.into())
                        }
                        vr => return Err(format!("unable to add right-hand field type {:?}", vr)),
                    },
                    vl => return Err(format!("unable to add left-hand field type {:?}", vl)),
                }
            }

            Operator::Subtract => {
                let (left, right) = coerce_number_types(left, right);
                match left {
                    Value::Float(fl) => match right {
                        Value::Float(fr) => Value::Float(fl - fr),
                        vr => {
                            return Err(format!(
                                "unable to subtract right-hand field type {:?}",
                                vr
                            ))
                        }
                    },
                    Value::Integer(il) => match right {
                        Value::Integer(ir) => Value::Integer(il - ir),
                        vr => {
                            return Err(format!(
                                "unable to subtract right-hand field type {:?}",
                                vr
                            ))
                        }
                    },
                    vl => return Err(format!("unable to subtract left-hand field type {:?}", vl)),
                }
            }

            Operator::Equal => {
                let (left, right) = coerce_number_types(left, right);
                Value::Boolean(left == right)
            }
            Operator::NotEqual => {
                let (left, right) = coerce_number_types(left, right);
                Value::Boolean(left != right)
            }
            Operator::Greater => compare_number_types(left, right, &|lf, rf| lf > rf)?,
            Operator::GreaterOrEqual => compare_number_types(left, right, &|lf, rf| lf >= rf)?,
            Operator::Less => compare_number_types(left, right, &|lf, rf| lf < rf)?,
            Operator::LessOrEqual => compare_number_types(left, right, &|lf, rf| lf <= rf)?,

            Operator::And => match left {
                Value::Boolean(bl) => match right {
                    Value::Boolean(br) => Value::Boolean(bl && br),
                    vr => return Err(format!("unable to AND right-hand field type {:?}", vr)),
                },
                vl => return Err(format!("unable to AND left-hand field type {:?}", vl)),
            },

            Operator::Or => match left {
                Value::Boolean(bl) => match right {
                    Value::Boolean(br) => Value::Boolean(bl || br),
                    vr => return Err(format!("unable to OR right-hand field type {:?}", vr)),
                },
                vl => return Err(format!("unable to OR left-hand field type {:?}", vl)),
            },
        }
        .into())
    }
}

//------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::LookupBuf,
        mapping::query::{path::Path, regex::Regex, Literal},
    };

    #[test]
    fn check_compare_query() {
        let cases = vec![
            (
                log_event! {
                    crate::config::log_schema().message_key().clone() => "".to_string(),
                    crate::config::log_schema().message_key().clone() => chrono::Utc::now(),
                },
                Ok(Value::Integer(15)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Integer(5))),
                    Box::new(Literal::from(Value::Integer(3))),
                    Operator::Multiply,
                ),
            ),
            (
                log_event! {
                    crate::config::log_schema().message_key().clone() => "".to_string(),
                    crate::config::log_schema().message_key().clone() => chrono::Utc::now(),
                },
                Err("unable to multiply left-hand field type Boolean(true)".into()),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Boolean(true))),
                    Box::new(Literal::from(Value::Integer(3))),
                    Operator::Multiply,
                ),
            ),
            (
                {
                    let mut event = log_event! {
                        crate::config::log_schema().message_key().clone() => "".to_string(),
                        crate::config::log_schema().message_key().clone() => chrono::Utc::now(),
                    };
                    event
                        .as_mut_log()
                        .insert(LookupBuf::from("foo"), Value::Integer(5));
                    event
                        .as_mut_log()
                        .insert(LookupBuf::from("bar"), Value::Integer(10));
                    event
                },
                Ok(Value::Float(2.0)),
                Arithmetic::new(
                    Box::new(Path::from("bar")),
                    Box::new(Path::from("foo")),
                    Operator::Divide,
                ),
            ),
            (
                log_event! {
                    crate::config::log_schema().message_key().clone() => "".to_string(),
                    crate::config::log_schema().message_key().clone() => chrono::Utc::now(),
                },
                Err("unable to divide right-hand field type Boolean(true)".into()),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Integer(3))),
                    Box::new(Literal::from(Value::Boolean(true))),
                    Operator::Divide,
                ),
            ),
            (
                log_event! {
                    crate::config::log_schema().message_key().clone() => "".to_string(),
                    crate::config::log_schema().message_key().clone() => chrono::Utc::now(),
                },
                Err("arithmetic can not be performed with regex".to_string()),
                Arithmetic::new(
                    Box::new(Literal::from(QueryValue::Regex(
                        Regex::new("a".to_string(), false, false, false).unwrap(),
                    ))),
                    Box::new(Literal::from(QueryValue::Regex(
                        Regex::new("a".to_string(), false, false, false).unwrap(),
                    ))),
                    Operator::And,
                ),
            ),
            (
                log_event! {
                    crate::config::log_schema().message_key().clone() => "".to_string(),
                    crate::config::log_schema().message_key().clone() => chrono::Utc::now(),
                },
                Ok(Value::Integer(1)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Integer(13))),
                    Box::new(Literal::from(Value::Integer(4))),
                    Operator::Modulo,
                ),
            ),
            (
                log_event! {
                    crate::config::log_schema().message_key().clone() => "".to_string(),
                    crate::config::log_schema().message_key().clone() => chrono::Utc::now(),
                },
                Ok(Value::Integer(17)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Integer(13))),
                    Box::new(Literal::from(Value::Integer(4))),
                    Operator::Add,
                ),
            ),
            (
                log_event! {
                    crate::config::log_schema().message_key().clone() => "".to_string(),
                    crate::config::log_schema().message_key().clone() => chrono::Utc::now(),
                },
                Ok(Value::from("foobar")),
                Arithmetic::new(
                    Box::new(Literal::from(Value::from("foo"))),
                    Box::new(Literal::from(Value::from("bar"))),
                    Operator::Add,
                ),
            ),
            (
                log_event! {
                    crate::config::log_schema().message_key().clone() => "".to_string(),
                    crate::config::log_schema().message_key().clone() => chrono::Utc::now(),
                },
                Ok(Value::Float(17.0)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Float(20.0))),
                    Box::new(Literal::from(Value::Integer(3))),
                    Operator::Subtract,
                ),
            ),
            (
                log_event! {
                    crate::config::log_schema().message_key().clone() => "".to_string(),
                    crate::config::log_schema().message_key().clone() => chrono::Utc::now(),
                },
                Ok(Value::Boolean(true)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Float(20.0))),
                    Box::new(Literal::from(Value::Integer(20))),
                    Operator::Equal,
                ),
            ),
            (
                log_event! {
                    crate::config::log_schema().message_key().clone() => "".to_string(),
                    crate::config::log_schema().message_key().clone() => chrono::Utc::now(),
                },
                Ok(Value::Boolean(true)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Integer(19))),
                    Box::new(Literal::from(Value::Integer(20))),
                    Operator::NotEqual,
                ),
            ),
            (
                log_event! {
                    crate::config::log_schema().message_key().clone() => "".to_string(),
                    crate::config::log_schema().message_key().clone() => chrono::Utc::now(),
                },
                Ok(Value::Boolean(true)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Float(21.0))),
                    Box::new(Literal::from(Value::Integer(18))),
                    Operator::Greater,
                ),
            ),
            (
                log_event! {
                    crate::config::log_schema().message_key().clone() => "".to_string(),
                    crate::config::log_schema().message_key().clone() => chrono::Utc::now(),
                },
                Ok(Value::Boolean(false)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Float(18.0))),
                    Box::new(Literal::from(Value::Integer(18))),
                    Operator::Greater,
                ),
            ),
            (
                log_event! {
                    crate::config::log_schema().message_key().clone() => "".to_string(),
                    crate::config::log_schema().message_key().clone() => chrono::Utc::now(),
                },
                Ok(Value::Boolean(false)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Integer(17))),
                    Box::new(Literal::from(Value::Float(18.0))),
                    Operator::GreaterOrEqual,
                ),
            ),
            (
                log_event! {
                    crate::config::log_schema().message_key().clone() => "".to_string(),
                    crate::config::log_schema().message_key().clone() => chrono::Utc::now(),
                },
                Ok(Value::Boolean(true)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Integer(18))),
                    Box::new(Literal::from(Value::Float(18.0))),
                    Operator::GreaterOrEqual,
                ),
            ),
            (
                log_event! {
                    crate::config::log_schema().message_key().clone() => "".to_string(),
                    crate::config::log_schema().message_key().clone() => chrono::Utc::now(),
                },
                Ok(Value::Boolean(false)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Integer(18))),
                    Box::new(Literal::from(Value::Float(18.0))),
                    Operator::Less,
                ),
            ),
            (
                log_event! {
                    crate::config::log_schema().message_key().clone() => "".to_string(),
                    crate::config::log_schema().message_key().clone() => chrono::Utc::now(),
                },
                Ok(Value::Boolean(true)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Integer(18))),
                    Box::new(Literal::from(Value::Float(18.0))),
                    Operator::LessOrEqual,
                ),
            ),
            (
                log_event! {
                    crate::config::log_schema().message_key().clone() => "".to_string(),
                    crate::config::log_schema().message_key().clone() => chrono::Utc::now(),
                },
                Ok(Value::Boolean(true)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Boolean(true))),
                    Box::new(Literal::from(Value::Boolean(false))),
                    Operator::Or,
                ),
            ),
            (
                log_event! {
                    crate::config::log_schema().message_key().clone() => "".to_string(),
                    crate::config::log_schema().message_key().clone() => chrono::Utc::now(),
                },
                Ok(Value::Boolean(false)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Boolean(true))),
                    Box::new(Literal::from(Value::Boolean(false))),
                    Operator::And,
                ),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp.map(QueryValue::Value));
        }
    }
}
