use regex::bytes::RegexSet;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct MatchAny;

impl Function for MatchAny {
    fn identifier(&self) -> &'static str {
        "match_any"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "patterns",
                kind: kind::ARRAY,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "match",
                source: r#"match_any("foo bar baz", patterns: [r'foo', r'123'])"#,
                result: Ok("true"),
            },
            Example {
                title: "no_match",
                source: r#"match_any("My name is John Doe", patterns: [r'\d+', r'Jane'])"#,
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
        let patterns = arguments.required_array("patterns")?;

        let mut re_strings = Vec::with_capacity(patterns.len());
        for expr in patterns {
            let value = expr
                .as_value()
                .ok_or(vrl::function::Error::ExpectedStaticExpression {
                    keyword: "patterns",
                    expr,
                })?;

            let re = value
                .try_regex()
                .map_err(|e| Box::new(e) as Box<dyn DiagnosticError>)?;
            re_strings.push(re.to_string());
        }

        let regex_set = RegexSet::new(re_strings).expect("regex were already valid");

        Ok(Box::new(MatchAnyFn { value, regex_set }))
    }
}

#[derive(Clone, Debug)]
struct MatchAnyFn {
    value: Box<dyn Expression>,
    regex_set: RegexSet,
}

impl Expression for MatchAnyFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let bytes = value.try_bytes()?;

        Ok(self.regex_set.is_match(&bytes).into())
    }

    fn type_def(&self, _state: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().boolean()
    }
}

#[cfg(test)]
#[allow(clippy::trivial_regex)]
mod tests {
    use regex::Regex;

    use super::*;

    test_function![
        r#match_any => MatchAny;

        yes {
            args: func_args![value: "foobar",
                             patterns: Value::Array(vec![
                                 Value::Regex(Regex::new("foo").unwrap().into()),
                                 Value::Regex(Regex::new("bar").unwrap().into()),
                                 Value::Regex(Regex::new("baz").unwrap().into()),
                             ])],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        no {
            args: func_args![value: "foo 2 bar",
                             patterns: Value::Array(vec![
                                 Value::Regex(Regex::new("baz|quux").unwrap().into()),
                                 Value::Regex(Regex::new("foobar").unwrap().into()),
                             ])],
            want: Ok(value!(false)),
            tdef: TypeDef::new().infallible().boolean(),
        }
    ];
}
