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

impl Function for Arithmetic {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        let left = self.left.execute(ctx);
        let right = self.right.execute(ctx);

        Ok(match self.op {
            Operator::Multiply => match left? {
                Value::Float(fl) => match right? {
                    Value::Float(fr) => Value::Float(fl * fr),
                    Value::Integer(ir) => Value::Float(fl * ir as f64),
                    vr => return Err(format!("unable to multiply right-hand field type {:?}", vr)),
                },
                Value::Integer(il) => match right? {
                    Value::Float(fr) => Value::Float(il as f64 * fr),
                    Value::Integer(ir) => Value::Integer(il * ir),
                    vr => return Err(format!("unable to multiply right-hand field type {:?}", vr)),
                },
                vl => return Err(format!("unable to multiply left-hand field type {:?}", vl)),
            },

            Operator::Divide => match left? {
                Value::Float(fl) => match right? {
                    Value::Float(fr) => Value::Float(fl / fr),
                    Value::Integer(ir) => Value::Float(fl / ir as f64),
                    vr => return Err(format!("unable to divide right-hand field type {:?}", vr)),
                },
                Value::Integer(il) => match right? {
                    Value::Float(fr) => Value::Float(il as f64 / fr),
                    Value::Integer(ir) => Value::Float(il as f64 / ir as f64),
                    vr => return Err(format!("unable to divide right-hand field type {:?}", vr)),
                },
                vl => return Err(format!("unable to divide left-hand field type {:?}", vl)),
            },

            Operator::Modulo => match left? {
                Value::Integer(il) => match right? {
                    Value::Integer(ir) => Value::Integer(il % ir),
                    vr => return Err(format!("unable to modulo right-hand field type {:?}", vr)),
                },
                vl => return Err(format!("unable to modulo left-hand field type {:?}", vl)),
            },

            _ => return Err("not implemented".into()),
        })
    }
}

//------------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use super::*;
    use crate::mapping::query::{
        path::Path,
        Literal,
    };

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
                Ok(Value::Integer(1)),
                Arithmetic::new(
                    Box::new(Literal::from(Value::Integer(13))),
                    Box::new(Literal::from(Value::Integer(4))),
                    Operator::Modulo,
                ),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }
}
