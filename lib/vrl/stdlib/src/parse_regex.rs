use regex::Regex;
use vrl::{function::Error, prelude::*};

use crate::util;

fn parse_regex(
    value: Value,
    numeric_groups: bool,
    pattern: &Regex,
) -> std::result::Result<Value, ExpressionError> {
    let bytes = value.try_bytes()?;
    let value = String::from_utf8_lossy(&bytes);
    let parsed = pattern
        .captures(&value)
        .map(|capture| util::capture_regex_to_map(pattern, capture, numeric_groups))
        .ok_or("could not find any pattern matches")?;
    Ok(parsed.into())
}

#[derive(Clone, Copy, Debug)]
pub struct ParseRegex;

impl Function for ParseRegex {
    fn identifier(&self) -> &'static str {
        "parse_regex"
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
            Parameter {
                keyword: "numeric_groups",
                kind: kind::BOOLEAN,
                required: false,
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
        let pattern = arguments.required_regex("pattern")?;
        let numeric_groups = arguments
            .optional("numeric_groups")
            .unwrap_or_else(|| expr!(false));

        Ok(Box::new(ParseRegexFn {
            value,
            pattern,
            numeric_groups,
        }))
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "simple match",
                source: r#"parse_regex!("8.7.6.5 - zorp", r'^(?P<host>[\w\.]+) - (?P<user>[\w]+)')"#,
                result: Ok(indoc! { r#"{
                "host": "8.7.6.5",
                "user": "zorp"
            }"# }),
            },
            Example {
                title: "numeric groups",
                source: r#"parse_regex!("8.7.6.5 - zorp", r'^(?P<host>[\w\.]+) - (?P<user>[\w]+)', numeric_groups: true)"#,
                result: Ok(indoc! { r#"{
                "0": "8.7.6.5 - zorp",
                "1": "8.7.6.5",
                "2": "zorp",
                "host": "8.7.6.5",
                "user": "zorp"
            }"# }),
            },
        ]
    }

    fn compile_argument(
        &self,
        _args: &[(&'static str, Option<FunctionArgument>)],
        _info: &FunctionCompileContext,
        name: &str,
        expr: Option<&expression::Expr>,
    ) -> CompiledArgument {
        match (name, expr) {
            ("pattern", Some(expr)) => {
                let regex: regex::Regex = match expr {
                    expression::Expr::Literal(expression::Literal::Regex(regex)) => {
                        Ok((**regex).clone())
                    }
                    expr => Err(Error::UnexpectedExpression {
                        keyword: "pattern",
                        expected: "regex",
                        expr: expr.clone(),
                    }),
                }?;

                Ok(Some(Box::new(regex) as _))
            }
            _ => Ok(None),
        }
    }

    fn call_by_vm(
        &self,
        _ctx: &mut Context,
        args: &mut VmArgumentList,
    ) -> std::result::Result<Value, ExpressionError> {
        let pattern = args
            .required_any("pattern")
            .downcast_ref::<regex::Regex>()
            .ok_or("no pattern")?;
        let value = args.required("value");
        let numeric_groups = args
            .optional("numeric_groups")
            .map(|value| value.try_boolean())
            .transpose()?
            .unwrap_or(false);

        parse_regex(value, numeric_groups, pattern)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ParseRegexFn {
    value: Box<dyn Expression>,
    pattern: Regex,
    numeric_groups: Box<dyn Expression>,
}

impl Expression for ParseRegexFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let numeric_groups = self.numeric_groups.resolve(ctx)?;
        let pattern = &self.pattern;

        parse_regex(value, numeric_groups.try_boolean()?, pattern)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::object(util::regex_kind(&self.pattern)).fallible()
    }
}

#[cfg(test)]
#[allow(clippy::trivial_regex)]
mod tests {
    use super::*;
    use vector_common::btreemap;

    test_function![
        find => ParseRegex;

        numeric_groups {
            args: func_args! [
                value: "5.86.210.12 - zieme4647 5667 [19/06/2019:17:20:49 -0400] \"GET /embrace/supply-chains/dynamic/vertical\" 201 20574",
                pattern: Regex::new(r#"^(?P<host>[\w\.]+) - (?P<user>[\w]+) (?P<bytes_in>[\d]+) \[(?P<timestamp>.*)\] "(?P<method>[\w]+) (?P<path>.*)" (?P<status>[\d]+) (?P<bytes_out>[\d]+)$"#)
                    .unwrap(),
                numeric_groups: true,
            ],
            want: Ok(value!({"bytes_in": "5667",
                             "host": "5.86.210.12",
                             "user": "zieme4647",
                             "timestamp": "19/06/2019:17:20:49 -0400",
                             "method": "GET",
                             "path": "/embrace/supply-chains/dynamic/vertical",
                             "status": "201",
                             "bytes_out": "20574",
                             "0": "5.86.210.12 - zieme4647 5667 [19/06/2019:17:20:49 -0400] \"GET /embrace/supply-chains/dynamic/vertical\" 201 20574",
                             "1": "5.86.210.12",
                             "2": "zieme4647",
                             "3": "5667",
                             "4": "19/06/2019:17:20:49 -0400",
                             "5": "GET",
                             "6": "/embrace/supply-chains/dynamic/vertical",
                             "7": "201",
                             "8": "20574",
            })),
            tdef: TypeDef::object(btreemap! {
                    Field::from("bytes_in") => Kind::bytes(),
                    Field::from("host") => Kind::bytes(),
                    Field::from("user") => Kind::bytes(),
                    Field::from("timestamp") => Kind::bytes(),
                    Field::from("method") => Kind::bytes(),
                    Field::from("path") => Kind::bytes(),
                    Field::from("status") => Kind::bytes(),
                    Field::from("bytes_out") => Kind::bytes(),
                    Field::from("0") => Kind::bytes() | Kind::null(),
                    Field::from("1") => Kind::bytes() | Kind::null(),
                    Field::from("2") => Kind::bytes() | Kind::null(),
                    Field::from("3") => Kind::bytes() | Kind::null(),
                    Field::from("4") => Kind::bytes() | Kind::null(),
                    Field::from("5") => Kind::bytes() | Kind::null(),
                    Field::from("6") => Kind::bytes() | Kind::null(),
                    Field::from("7") => Kind::bytes() | Kind::null(),
                    Field::from("8") => Kind::bytes() | Kind::null(),
                }).fallible(),
        }

        single_match {
            args: func_args! [
                value: "first group and second group",
                pattern: Regex::new(r#"(?P<number>.*?) group"#).unwrap()
            ],
            want: Ok(value!({"number": "first"})),
            tdef: TypeDef::object(btreemap! {
                        Field::from("number") => Kind::bytes(),
                        Field::from("0") => Kind::bytes() | Kind::null(),
                        Field::from("1") => Kind::bytes() | Kind::null(),
                }).fallible(),
        }

        no_match {
            args: func_args! [
                value: "I don't match",
                pattern: Regex::new(r#"^(?P<host>[\w\.]+) - (?P<user>[\w]+) (?P<bytes_in>[\d]+) \[(?P<timestamp>.*)\] "(?P<method>[\w]+) (?P<path>.*)" (?P<status>[\d]+) (?P<bytes_out>[\d]+)$"#)
                            .unwrap()
            ],
            want: Err("could not find any pattern matches"),
            tdef: TypeDef::object(btreemap! {
                    Field::from("host") => Kind::bytes(),
                    Field::from("user") => Kind::bytes(),
                    Field::from("bytes_in") => Kind::bytes(),
                    Field::from("timestamp") => Kind::bytes(),
                    Field::from("method") => Kind::bytes(),
                    Field::from("path") => Kind::bytes(),
                    Field::from("status") => Kind::bytes(),
                    Field::from("bytes_out") => Kind::bytes(),
                    Field::from("0") => Kind::bytes() | Kind::null(),
                    Field::from("1") => Kind::bytes() | Kind::null(),
                    Field::from("2") => Kind::bytes() | Kind::null(),
                    Field::from("3") => Kind::bytes() | Kind::null(),
                    Field::from("4") => Kind::bytes() | Kind::null(),
                    Field::from("5") => Kind::bytes() | Kind::null(),
                    Field::from("6") => Kind::bytes() | Kind::null(),
                    Field::from("7") => Kind::bytes() | Kind::null(),
                    Field::from("8") => Kind::bytes() | Kind::null(),
                }).fallible(),
        }
    ];
}
