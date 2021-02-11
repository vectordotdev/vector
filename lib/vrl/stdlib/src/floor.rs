use crate::util::round_to_precision;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Floor;

impl Function for Floor {
    fn identifier(&self) -> &'static str {
        "floor"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::ANY,
                required: true,
            },
            Parameter {
                keyword: "precision",
                kind: kind::ANY,
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let precision = arguments.optional("precision");

        Ok(Box::new(FloorFn { value, precision }))
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "floor",
            source: r#"floor(9.8)"#,
            result: Ok("9.0"),
        }]
    }
}

#[derive(Clone, Debug)]
struct FloorFn {
    value: Box<dyn Expression>,
    precision: Option<Box<dyn Expression>>,
}

impl FloorFn {
    /*
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, precision: Option<Box<dyn Expression>>) -> Self {
        Self { value, precision }
    }
    */
}

impl Expression for FloorFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let precision = match &self.precision {
            Some(expr) => expr.resolve(ctx)?.try_integer()?,
            None => 0,
        };

        match self.value.resolve(ctx)? {
            Value::Float(f) => Ok(round_to_precision(*f, precision, f64::floor).into()),
            v @ Value::Integer(_) => Ok(v),
            _ => unreachable!(),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        TypeDef::new().scalar(match self.value.type_def(state).kind() {
            v if v.is_float() || v.is_integer() => v,
            _ => Kind::Integer | Kind::Float,
        })
    }
}

/*
#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;

    vrl::test_type_def![
        value_float {
            expr: |_| FloorFn {
                value: Literal::from(1.0).boxed(),
                precision: None,
            },
            def: TypeDef { kind: Kind::Float, ..Default::default() },
        }

        value_integer {
            expr: |_| FloorFn {
                value: Literal::from(1).boxed(),
                precision: None,
            },
            def: TypeDef { kind: Kind::Integer, ..Default::default() },
        }

        value_float_or_integer {
            expr: |_| FloorFn {
                value: Variable::new("foo".to_owned(), None).boxed(),
                precision: None,
            },
            def: TypeDef { fallible: true, kind: Kind::Integer | Kind::Float, ..Default::default() },
        }

        fallible_precision {
            expr: |_| FloorFn {
                value: Literal::from(1).boxed(),
                precision: Some(Variable::new("foo".to_owned(), None).boxed()),
            },
            def: TypeDef { fallible: true, kind: Kind::Integer, ..Default::default() },
        }
    ];

    #[test]
    fn floor() {
        let cases = vec![
            (
                btreemap! { "foo" => 1234.2 },
                Ok(1234.0.into()),
                FloorFn::new(Box::new(Path::from("foo")), None),
            ),
            (
                btreemap! {},
                Ok(1234.0.into()),
                FloorFn::new(Box::new(Literal::from(Value::Float(1234.8))), None),
            ),
            (
                btreemap! {},
                Ok(1234.into()),
                FloorFn::new(Box::new(Literal::from(Value::Integer(1234))), None),
            ),
            (
                btreemap! {},
                Ok(1234.3.into()),
                FloorFn::new(
                    Box::new(Literal::from(Value::Float(1234.39429))),
                    Some(Box::new(Literal::from(1))),
                ),
            ),
            (
                btreemap! {},
                Ok(1234.5678.into()),
                FloorFn::new(
                    Box::new(Literal::from(Value::Float(1234.56789))),
                    Some(Box::new(Literal::from(4))),
                ),
            ),
            (
                btreemap! {},
                Ok(9876543210123456789098765432101234567890987654321.98765.into()),
                FloorFn::new(
                    Box::new(Literal::from(
                        9876543210123456789098765432101234567890987654321.987654321,
                    )),
                    Some(Box::new(Literal::from(5))),
                ),
            ),
        ];

        let mut state = state::Program::default();

        for (object, exp, func) in cases {
            let mut object: Value = object.into();
            let got = func
                .resolve(&mut ctx)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
*/
