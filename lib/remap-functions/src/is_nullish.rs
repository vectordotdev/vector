use remap::prelude::*;

use crate::util;

#[derive(Clone, Copy, Debug)]
pub struct IsNullish;

impl Function for IsNullish {
    fn identifier(&self) -> &'static str {
        "is_nullish"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Bytes(_) | Value::Null),
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();

        Ok(Box::new(IsNullishFn { value }))
    }
}

#[derive(Clone, Debug)]
struct IsNullishFn {
    value: Box<dyn Expression>,
}

impl Expression for IsNullishFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        Ok(util::is_nullish(&self.value.execute(state, object)?).into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;

        self.value
            .type_def(state)
            .into_fallible(false)
            .with_constraint(Kind::Boolean)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use value::Kind;

    test_type_def![
        string_infallible {
            expr: |_| IsNullishFn {
                value: Literal::from("some string").boxed(),
            },
            def: TypeDef {
                kind: Kind::Boolean,
                ..Default::default()
            },
        }

        null_infallible {
            expr: |_| IsNullishFn {
                value: Literal::from(()).boxed(),
            },
            def: TypeDef {
                kind: Kind::Boolean,
                ..Default::default()
            },
        }

        // Show that non-string/null literal is infallible
        integer_fallible {
            expr: |_| IsNullishFn {
                value: lit!(42).boxed(),
            },
            def: TypeDef {
                kind: Kind::Boolean,
                ..Default::default()
            },
        }

        // Show that non-literal is infallible
        array_fallible {
            expr: |_| IsNullishFn {
                value: array!["foo"].boxed(),
            },
            def: TypeDef {
                kind: Kind::Boolean,
                ..Default::default()
            },
        }
    ];

    test_function![
        is_nullish => IsNullish;

        empty_string {
            args: func_args![value: value!("")],
            want: Ok(value!(true)),
        }

        single_space_string {
            args: func_args![value: value!(" ")],
            want: Ok(value!(true)),
        }

        multi_space_string {
            args: func_args![value: value!("     ")],
            want: Ok(value!(true)),
        }

        newline_string {
            args: func_args![value: value!("\n")],
            want: Ok(value!(true)),
        }

        carriage_return_string {
            args: func_args![value: value!("\r")],
            want: Ok(value!(true)),
        }

        dash_string {
            args: func_args![value: value!("-")],
            want: Ok(value!(true)),
        }

        null {
            args: func_args![value: value!(null)],
            want: Ok(value!(true)),
        }

        non_empty_string {
            args: func_args![value: value!("hello world")],
            want: Ok(value!(false)),
        }

        // Shows that a non-string/null literal returns false
        integer {
            args: func_args![value: value!(427)],
            want: Ok(value!(false)),
        }

        // Shows that a non-literal type returns false
        array {
            args: func_args![value: array![1, 2, 3]],
            want: Ok(value!(false)),
        }
    ];
}
