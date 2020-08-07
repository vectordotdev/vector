use super::Function;
use crate::{
    event::{Event, Value},
    mapping::Result,
};

#[derive(Debug, Clone)]
pub enum Operator {
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
pub struct Arithmetic {
    left: Box<dyn Function>,
    right: Box<dyn Function>,
    op: Operator,
}

impl Arithmetic {
    pub fn new(left: Box<dyn Function>, right: Box<dyn Function>, op: Operator) -> Self {
        Self { left, right, op }
    }
}

// If either value is a floating point number type and the other value is an
// integer type the integer is "degraded" into a float. This allows us to
// perform arithmetic on common values, but if both are integers then their
// precision is preserved.
fn consistent_number_types(mut left: Value, mut right: Value) -> (Value, Value) {
    if let Value::Float(_) = left {
        if let Value::Integer(ri) = right {
            right = Value::Float(ri as f64);
        }
    } else if let Value::Float(_) = right {
        if let Value::Integer(li) = left {
            left = Value::Float(li as f64);
        }
    }
    (left, right)
}

// Degrades non-float numerical types into floats for the purposes of convenient
// boolean comparison.
fn compare_number_types(
    left: Value,
    right: Value,
    compare_fn: &dyn Fn(f64, f64) -> bool,
) -> Result<Value> {
    let (left, right) = consistent_number_types(left, right);
    Ok(Value::Boolean(match left {
        Value::Float(fl) => match right {
            Value::Float(fr) => compare_fn(fl, fr),
            vr => return Err(format!("unable to compare right-hand field type {:?}", vr)),
        },
        Value::Integer(il) => match right {
            Value::Integer(ir) => compare_fn(il as f64, ir as f64),
            vr => return Err(format!("unable to compare right-hand field type {:?}", vr)),
        },
        vl => return Err(format!("unable to compare left-hand field type {:?}", vl)),
    }))
}

impl Function for Arithmetic {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        let left = self.left.execute(ctx);
        let right = self.right.execute(ctx);

        Ok(match self.op {
            Operator::Multiply => {
                let (left, right) = consistent_number_types(left?, right?);
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
                let (left, right) = consistent_number_types(left?, right?);
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

            Operator::Modulo => match left? {
                Value::Integer(il) => match right? {
                    Value::Integer(ir) => Value::Integer(il % ir),
                    vr => return Err(format!("unable to modulo right-hand field type {:?}", vr)),
                },
                vl => return Err(format!("unable to modulo left-hand field type {:?}", vl)),
            },

            Operator::Add => {
                let (left, right) = consistent_number_types(left?, right?);
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
                            let mut b = sl.clone();
                            b.extend_from_slice(&sr);
                            Value::Bytes(b)
                        }
                        vr => return Err(format!("unable to add right-hand field type {:?}", vr)),
                    },
                    vl => return Err(format!("unable to add left-hand field type {:?}", vl)),
                }
            }

            Operator::Subtract => {
                let (left, right) = consistent_number_types(left?, right?);
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
                let (left, right) = consistent_number_types(left?, right?);
                Value::Boolean(left == right)
            }
            Operator::NotEqual => {
                let (left, right) = consistent_number_types(left?, right?);
                Value::Boolean(left != right)
            }
            Operator::Greater => compare_number_types(left?, right?, &|lf, rf| lf > rf)?,
            Operator::GreaterOrEqual => compare_number_types(left?, right?, &|lf, rf| lf >= rf)?,
            Operator::Less => compare_number_types(left?, right?, &|lf, rf| lf < rf)?,
            Operator::LessOrEqual => compare_number_types(left?, right?, &|lf, rf| lf <= rf)?,

            Operator::And => match left? {
                Value::Boolean(bl) => match right? {
                    Value::Boolean(br) => Value::Boolean(bl && br),
                    vr => return Err(format!("unable to AND right-hand field type {:?}", vr)),
                },
                vl => return Err(format!("unable to AND left-hand field type {:?}", vl)),
            },

            Operator::Or => match left? {
                Value::Boolean(bl) => match right? {
                    Value::Boolean(br) => Value::Boolean(bl || br),
                    vr => return Err(format!("unable to OR right-hand field type {:?}", vr)),
                },
                vl => return Err(format!("unable to OR left-hand field type {:?}", vl)),
            },
        })
    }
}

//------------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use super::*;
    use crate::mapping::query::{path::Path, Literal};

    #[test]
    fn check_compare_query() {
        let cases = vec![
            (
                Event::from(""),
                Ok(Value::Integer(15)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Integer(5))),
                    Box::new(Literal::from(Value::Integer(3))),
                    Operator::Multiply,
                ),
            ),
            (
                Event::from(""),
                Err("unable to multiply left-hand field type Boolean(true)".into()),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Boolean(true))),
                    Box::new(Literal::from(Value::Integer(3))),
                    Operator::Multiply,
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::Integer(5));
                    event.as_mut_log().insert("bar", Value::Integer(10));
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
                Event::from(""),
                Err("unable to divide right-hand field type Boolean(true)".into()),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Integer(3))),
                    Box::new(Literal::from(Value::Boolean(true))),
                    Operator::Divide,
                ),
            ),
            (
                Event::from(""),
                Ok(Value::Integer(1)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Integer(13))),
                    Box::new(Literal::from(Value::Integer(4))),
                    Operator::Modulo,
                ),
            ),
            (
                Event::from(""),
                Ok(Value::Integer(17)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Integer(13))),
                    Box::new(Literal::from(Value::Integer(4))),
                    Operator::Add,
                ),
            ),
            (
                Event::from(""),
                Ok(Value::from("foobar")),
                Arithmetic::new(
                    Box::new(Literal::from(Value::from("foo"))),
                    Box::new(Literal::from(Value::from("bar"))),
                    Operator::Add,
                ),
            ),
            (
                Event::from(""),
                Ok(Value::Float(17.0)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Float(20.0))),
                    Box::new(Literal::from(Value::Integer(3))),
                    Operator::Subtract,
                ),
            ),
            (
                Event::from(""),
                Ok(Value::Boolean(true)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Float(20.0))),
                    Box::new(Literal::from(Value::Integer(20))),
                    Operator::Equal,
                ),
            ),
            (
                Event::from(""),
                Ok(Value::Boolean(true)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Integer(19))),
                    Box::new(Literal::from(Value::Integer(20))),
                    Operator::NotEqual,
                ),
            ),
            (
                Event::from(""),
                Ok(Value::Boolean(true)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Float(21.0))),
                    Box::new(Literal::from(Value::Integer(18))),
                    Operator::Greater,
                ),
            ),
            (
                Event::from(""),
                Ok(Value::Boolean(false)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Float(18.0))),
                    Box::new(Literal::from(Value::Integer(18))),
                    Operator::Greater,
                ),
            ),
            (
                Event::from(""),
                Ok(Value::Boolean(false)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Integer(17))),
                    Box::new(Literal::from(Value::Float(18.0))),
                    Operator::GreaterOrEqual,
                ),
            ),
            (
                Event::from(""),
                Ok(Value::Boolean(true)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Integer(18))),
                    Box::new(Literal::from(Value::Float(18.0))),
                    Operator::GreaterOrEqual,
                ),
            ),
            (
                Event::from(""),
                Ok(Value::Boolean(false)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Integer(18))),
                    Box::new(Literal::from(Value::Float(18.0))),
                    Operator::Less,
                ),
            ),
            (
                Event::from(""),
                Ok(Value::Boolean(true)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Integer(18))),
                    Box::new(Literal::from(Value::Float(18.0))),
                    Operator::LessOrEqual,
                ),
            ),
            (
                Event::from(""),
                Ok(Value::Boolean(true)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Boolean(true))),
                    Box::new(Literal::from(Value::Boolean(false))),
                    Operator::Or,
                ),
            ),
            (
                Event::from(""),
                Ok(Value::Boolean(false)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Boolean(true))),
                    Box::new(Literal::from(Value::Boolean(false))),
                    Operator::And,
                ),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }
}
