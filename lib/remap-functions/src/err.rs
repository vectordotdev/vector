use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ERR;

impl Function for ERR {
    fn identifier(&self) -> &'static str {
        "err"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |_| true,
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();

        Ok(Box::new(ErrFn { value }))
    }
}

#[derive(Debug, Clone)]
pub struct ErrFn {
    value: Box<dyn Expression>,
}

impl Expression for ErrFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        Ok(self.value.execute(state, object).is_err().into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef {
            kind: value::Kind::Boolean,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        err => ERR;

        pass {
            args: func_args![
                value: remap::expression::Arithmetic::new(
                    Box::new(Literal::from(true).into()),
                    Box::new(Literal::from(false).into()),
                    remap::Operator::Multiply,
                ),
            ],
            want: Ok(true),
        }

        fail {
            args: func_args![value: true],
            want: Ok(false),
        }
    ];

    test_type_def![static_type_def {
        expr: |_| ErrFn {
            value: Literal::from(true).boxed(),
        },
        def: TypeDef {
            kind: value::Kind::Boolean,
            ..Default::default()
        },
    }];
}
