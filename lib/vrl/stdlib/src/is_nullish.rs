use vrl::prelude::*;

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
            kind: kind::ANY,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "null",
            source: r#"is_nullish(null)"#,
            result: Ok("true"),
        }]
    }

    fn compile(&self, _state: &state::Compiler, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(IsNullishFn { value }))
    }
}

#[derive(Clone, Debug)]
struct IsNullishFn {
    value: Box<dyn Expression>,
}

impl Expression for IsNullishFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        Ok(util::is_nullish(&self.value.resolve(ctx)?).into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().boolean()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    test_function![
        is_nullish => IsNullish;

        empty_string {
            args: func_args![value: value!("")],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        single_space_string {
            args: func_args![value: value!(" ")],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        multi_space_string {
            args: func_args![value: value!("     ")],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        newline_string {
            args: func_args![value: value!("\n")],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        carriage_return_string {
            args: func_args![value: value!("\r")],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        dash_string {
            args: func_args![value: value!("-")],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        null {
            args: func_args![value: value!(null)],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        non_empty_string {
            args: func_args![value: value!("hello world")],
            want: Ok(value!(false)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        // Shows that a non-string/null literal returns false
        integer {
            args: func_args![value: value!(427)],
            want: Ok(value!(false)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        // Shows that a non-literal type returns false
        array {
            args: func_args![value: value!([1, 2, 3])],
            want: Ok(value!(false)),
            tdef: TypeDef::new().infallible().boolean(),
        }
    ];
}
