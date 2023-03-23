use ::value::Value;
use regex::Regex;
use vrl::prelude::*;

fn match_(value: Value, pattern: Value) -> Resolved {
    let string = value.try_bytes_utf8_lossy()?;
    let pattern = pattern.try_regex()?;
    Ok(pattern.is_match(&string).into())
}

fn match_static(value: Value, pattern: &Regex) -> Resolved {
    let string = value.try_bytes_utf8_lossy()?;
    Ok(pattern.is_match(&string).into())
}

#[derive(Clone, Copy, Debug)]
pub struct Match;

impl Function for Match {
    fn identifier(&self) -> &'static str {
        "match"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "pattern",
                kind: kind::REGEX,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "match",
                source: r#"match("foobar", r'foo')"#,
                result: Ok("true"),
            },
            Example {
                title: "mismatch",
                source: r#"match("bazqux", r'foo')"#,
                result: Ok("false"),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let pattern = arguments.required("pattern");

        match pattern.as_value() {
            Some(pattern) => {
                let pattern = pattern
                    .try_regex()
                    .map_err(|e| Box::new(e) as Box<dyn DiagnosticMessage>)?;

                let pattern = Regex::new(pattern.as_str()).map_err(|e| {
                    Box::new(ExpressionError::from(e.to_string())) as Box<dyn DiagnosticMessage>
                })?;

                Ok(MatchStaticFn { value, pattern }.as_expr())
            }
            None => Ok(MatchFn { value, pattern }.as_expr()),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MatchFn {
    value: Box<dyn Expression>,
    pattern: Box<dyn Expression>,
}

impl FunctionExpression for MatchFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let pattern = self.pattern.resolve(ctx)?;

        match_(value, pattern)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::boolean().infallible()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MatchStaticFn {
    value: Box<dyn Expression>,
    pattern: Regex,
}

impl FunctionExpression for MatchStaticFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        match_static(value, &self.pattern)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::boolean().infallible()
    }
}

#[cfg(test)]
#[allow(clippy::trivial_regex)]
mod tests {
    use regex::Regex;

    use super::*;

    test_function![
        r#match => Match;

        yes {
            args: func_args![value: "foobar",
                             pattern: Value::Regex(Regex::new("\\s\\w+").unwrap().into())],
            want: Ok(value!(false)),
            tdef: TypeDef::boolean().infallible(),
        }

        no {
            args: func_args![value: "foo 2 bar",
                             pattern: Value::Regex(Regex::new("foo \\d bar").unwrap().into())],
            want: Ok(value!(true)),
            tdef: TypeDef::boolean().infallible(),
        }
    ];
}
