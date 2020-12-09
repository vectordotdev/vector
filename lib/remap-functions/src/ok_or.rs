use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct OkOr;

impl Function for OkOr {
    fn identifier(&self) -> &'static str {
        "ok_or"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |_| true,
                required: true,
            },
            Parameter {
                keyword: "default",
                accepts: |_| true,
                required: true,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let default = arguments.required("default")?.boxed();

        Ok(Box::new(OkOrFn { value, default }))
    }
}

#[derive(Debug, Clone)]
pub struct OkOrFn {
    value: Box<dyn Expression>,
    default: Box<dyn Expression>,
}

impl Expression for OkOrFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        self.value
            .execute(state, object)
            .or_else(|_| self.default.execute(state, object))
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .merge_with_default_optional(Some(self.default.type_def(state)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use remap::expression::Arithmetic;
    use remap::Operator;

    test_function![
        ok_or => OkOr;

        value_ok_default_ok {
            args: func_args![
                value: "foo",
                default: "bar",
            ],
            want: Ok("foo"),
        }

        value_ok_default_fail {
            args: func_args![
                value: "foo",
                default: Arithmetic::new(
                    Box::new(Literal::from(true).into()),
                    Box::new(Literal::from(false).into()),
                    Operator::Multiply,
                ),
            ],
            want: Ok("foo"),
        }

        value_fail_default_ok {
            args: func_args![
                value: Arithmetic::new(
                    Box::new(Literal::from(true).into()),
                    Box::new(Literal::from(false).into()),
                    Operator::Multiply,
                ),
                default: "foo",
            ],
            want: Ok("foo"),
        }

        value_fail_default_fail {
            args: func_args![
                value: Arithmetic::new(
                    Box::new(Literal::from(true).into()),
                    Box::new(Literal::from(false).into()),
                    Operator::Multiply,
                ),
                default: Arithmetic::new(
                    Box::new(Literal::from(true).into()),
                    Box::new(Literal::from(false).into()),
                    Operator::Add,
                ),
            ],
            want: Err("value error: unable to add value type boolean to boolean"),
        }
    ];

    test_type_def![
        value_ok_default_ok {
            expr: |_| OkOrFn {
                value: Literal::from("foo").boxed(),
                default: Literal::from(true).boxed(),
            },
            def: TypeDef {
                kind: value::Kind::Bytes,
                ..Default::default()
            },
        }

        value_ok_default_fail {
            expr: |_| OkOrFn {
                value: Literal::from("foo").boxed(),
                default: Box::new(Arithmetic::new(
                    Box::new(Literal::from(true).into()),
                    Box::new(Literal::from(false).into()),
                    Operator::Multiply,
                )),
            },
            def: TypeDef {
                kind: value::Kind::Bytes,
                ..Default::default()
            },
        }

        value_fail_default_ok {
            expr: |_| OkOrFn {
                value: Box::new(Arithmetic::new(
                    Box::new(Literal::from(true).into()),
                    Box::new(Literal::from(false).into()),
                    Operator::Multiply,
                )),
                default: Literal::from(true).boxed(),
            },
            def: TypeDef {
                kind: value::Kind::Boolean,
                ..Default::default()
            },
        }

        value_fail_default_fail {
            expr: |_| OkOrFn {
                value: Box::new(Arithmetic::new(
                    Box::new(Literal::from(true).into()),
                    Box::new(Literal::from(false).into()),
                    Operator::Multiply,
                )),
                default: Box::new(Arithmetic::new(
                    Box::new(Literal::from(true).into()),
                    Box::new(Literal::from(false).into()),
                    Operator::Add,
                )),
            },
            def: TypeDef {
                kind: value::Kind::Bytes | value::Kind::Integer | value::Kind::Float,
                fallible: true,
                ..Default::default()
            },
        }
    ];
}
