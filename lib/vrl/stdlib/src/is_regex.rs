use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct IsRegex;

impl Function for IsRegex {
    fn identifier(&self) -> &'static str {
        "is_regex"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::ANY,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "string",
                source: r#"is_regex("foobar")"#,
                result: Ok("false"),
            },
            Example {
                title: "regex",
                source: r#"is_regex(r'\d+')"#,
                result: Ok("true"),
            },
            Example {
                title: "null",
                source: r#"is_regex(null)"#,
                result: Ok("false"),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(IsRegexFn { value }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        Ok(value!(args.required("value").is_regex()))
    }
}

#[derive(Clone, Debug)]
struct IsRegexFn {
    value: Box<dyn Expression>,
}

impl Expression for IsRegexFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.value.resolve(ctx).map(|v| value!(v.is_regex()))
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().boolean()
    }
}

#[cfg(test)]
mod tests {
    use regex::Regex;

    use super::*;

    test_function![
        is_regex => IsRegex;

        bytes {
            args: func_args![value: value!("foobar")],
            want: Ok(value!(false)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        regex {
            args: func_args![value: value!(Regex::new(r"\d+").unwrap())],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }
    ];
}
