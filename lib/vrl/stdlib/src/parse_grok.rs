use vrl::prelude::*;

use crate::parse_groks::ParseGroks;

#[derive(Clone, Copy, Debug)]
pub struct ParseGrok;

impl Function for ParseGrok {
    fn identifier(&self) -> &'static str {
        "parse_grok"
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
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "aliases",
                kind: kind::OBJECT,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "parse grok pattern",
            source: indoc! {r#"
                value = "2020-10-02T23:22:12.223222Z info Hello world"

                parse_grok!(value, "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}")
            "#},
            result: Ok(indoc! {r#"
                {
                    "timestamp": "2020-10-02T23:22:12.223222Z",
                    "level": "info",
                    "message": "Hello world"
                }
            "#}),
        }]
    }

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required_expr("value");
        let pattern = arguments.required_expr("pattern");
        let patterns = vec![pattern];
        let aliases = arguments.optional_object("aliases")?.unwrap_or_default();

        ParseGroks::compile(value, patterns, aliases)
    }

    fn compile_argument(
        &self,
        args: &[(&'static str, Option<FunctionArgument>)],
        _ctx: &mut FunctionCompileContext,
        name: &str,
        expr: Option<&expression::Expr>,
    ) -> CompiledArgument {
        match (name, expr) {
            ("pattern", Some(expr)) => {
                let pattern = expr.as_literal("pattern")?;
                let patterns = vec![pattern];

                let aliases = args.iter().find_map::<&FunctionArgument, _>(|(name, arg)| {
                    if *name == "aliases" {
                        arg.as_ref()
                    } else {
                        None
                    }
                });

                ParseGroks::compile_pattern_argument(patterns, aliases)
            }
            ("aliases", Some(_)) => Ok(None),
            _ => Ok(None),
        }
    }
}

#[cfg(test)]
mod test {
    use ::value::Value;
    use vector_common::btreemap;

    use super::*;

    test_function![
        parse_grok => ParseGrok;

        invalid_grok {
            args: func_args![ value: "foo",
                              pattern: "%{NOG}"],
            want: Err("failed to parse grok expression '\\A%{NOG}\\z': The given pattern definition name \"NOG\" could not be found in the definition map"),
            tdef: TypeDef::object(Collection::any()).fallible(),
        }

        error {
            args: func_args![ value: "an ungrokkable message",
                              pattern: "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"],
            want: Err("unable to parse grok: value does not match any rule"),
            tdef: TypeDef::object(Collection::any()).fallible(),
        }

        error2 {
            args: func_args![ value: "2020-10-02T23:22:12.223222Z an ungrokkable message",
                              pattern: "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"],
            want: Err("unable to parse grok: value does not match any rule"),
            tdef: TypeDef::object(Collection::any()).fallible(),
        }

        parsed {
            args: func_args![ value: "2020-10-02T23:22:12.223222Z info Hello world",
                              pattern: "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"],
            want: Ok(Value::from(btreemap! {
                "timestamp" => "2020-10-02T23:22:12.223222Z",
                "level" => "info",
                "message" => "Hello world",
            })),
            tdef: TypeDef::object(Collection::any()).fallible(),
        }

        parsed2 {
            args: func_args![ value: "2020-10-02T23:22:12.223222Z",
                              pattern: "(%{TIMESTAMP_ISO8601:timestamp}|%{LOGLEVEL:level})"],
            want: Ok(Value::from(btreemap! {
                "level" => "",
                "timestamp" => "2020-10-02T23:22:12.223222Z",
            })),
            tdef: TypeDef::object(Collection::any()).fallible(),
        }
    ];
}
