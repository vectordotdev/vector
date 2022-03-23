use vrl::prelude::*;

fn match_array(list: Value, pattern: Value, all: Option<Value>) -> Resolved {
    let pattern = pattern.try_regex()?;
    let list = list.try_array()?;
    let all = match all {
        Some(value) => value.try_boolean()?,
        None => false,
    };
    let matcher = |i: &Value| match i.try_bytes_utf8_lossy() {
        Ok(v) => pattern.is_match(&v),
        _ => false,
    };
    let included = if all {
        list.iter().all(matcher)
    } else {
        list.iter().any(matcher)
    };
    Ok(included.into())
}

#[derive(Clone, Copy, Debug)]
pub struct MatchArray;

impl Function for MatchArray {
    fn identifier(&self) -> &'static str {
        "match_array"
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "match",
                source: r#"match_array(["foobar", "bazqux"], r'foo')"#,
                result: Ok("true"),
            },
            Example {
                title: "mismatch",
                source: r#"match_array(["bazqux", "xyz"], r'foo')"#,
                result: Ok("false"),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let pattern = arguments.required("pattern");
        let all = arguments.optional("all");

        Ok(Box::new(MatchArrayFn {
            value,
            pattern,
            all,
        }))
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::ARRAY,
                required: true,
            },
            Parameter {
                keyword: "pattern",
                kind: kind::REGEX,
                required: true,
            },
            Parameter {
                keyword: "all",
                kind: kind::BOOLEAN,
                required: false,
            },
        ]
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let value = args.required("value");
        let pattern = args.required("pattern");
        let all = args.optional("all");

        match_array(value, pattern, all)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MatchArrayFn {
    value: Box<dyn Expression>,
    pattern: Box<dyn Expression>,
    all: Option<Box<dyn Expression>>,
}

impl Expression for MatchArrayFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let list = self.value.resolve(ctx)?;
        let pattern = self.pattern.resolve(ctx)?;
        let all = self
            .all
            .as_ref()
            .map(|expr| expr.resolve(ctx))
            .transpose()?;

        match_array(list, pattern, all)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::boolean().infallible()
    }
}

#[cfg(test)]
#[allow(clippy::trivial_regex)]
mod tests {
    use regex::Regex;

    use super::*;

    test_function![
        match_array => MatchArray;

        default {
            args: func_args![
                value: value!(["foo", "foobar", "barfoo"]),
                pattern: Value::Regex(Regex::new("foo").unwrap().into())
            ],
            want: Ok(value!(true)),
            tdef: TypeDef::boolean().infallible(),
        }

        all {
            args: func_args![
                value: value!(["foo", "foobar", "barfoo"]),
                pattern: Value::Regex(Regex::new("foo").unwrap().into()),
                all: value!(true),
            ],
            want: Ok(value!(true)),
            tdef: TypeDef::boolean().infallible(),
        }

        not_all {
            args: func_args![
                value: value!(["foo", "foobar", "baz"]),
                pattern: Value::Regex(Regex::new("foo").unwrap().into()),
                all: value!(true),
            ],
            want: Ok(value!(false)),
            tdef: TypeDef::boolean().infallible(),
        }

        mixed_values {
            args: func_args![
                value: value!(["foo", "123abc", 1, true, [1,2,3]]),
                pattern: Value::Regex(Regex::new("abc").unwrap().into())
            ],
            want: Ok(value!(true)),
            tdef: TypeDef::boolean().infallible(),
        }

        mixed_values_no_match {
            args: func_args![
                value: value!(["foo", "123abc", 1, true, [1,2,3]]),
                pattern: Value::Regex(Regex::new("xyz").unwrap().into()),
            ],
            want: Ok(value!(false)),
            tdef: TypeDef::boolean().infallible(),
        }

        mixed_values_no_match_all {
            args: func_args![
                value: value!(["foo", "123abc", 1, true, [1,2,3]]),
                pattern: Value::Regex(Regex::new("abc`").unwrap().into()),
                all: value!(true),
            ],
            want: Ok(value!(false)),
            tdef: TypeDef::boolean().infallible(),
        }
    ];
}
