use ::value::Value;
use regex::Regex;
use vrl::{prelude::*, state::TypeState};

use crate::util;

fn parse_regex(value: Value, numeric_groups: bool, pattern: Value) -> Resolved {
    let bytes = value.try_bytes()?;
    let value = String::from_utf8_lossy(&bytes);

    match pattern {
        Value::Bytes(bytes) => {
            let pattern = Regex::new(&String::from_utf8_lossy(&bytes)).unwrap();
            let parsed = pattern
                .captures(&value)
                .map(|capture| util::capture_regex_to_map(&pattern, &capture, numeric_groups))
                .ok_or("could not find any pattern matches")?;
            Ok(parsed.into())
        }
        Value::Regex(regex) => {
            let parsed = regex
                .captures(&value)
                .map(|capture| util::capture_regex_to_map(&regex, &capture, numeric_groups))
                .ok_or("could not find any pattern matches")?;
            Ok(parsed.into())
        }
        value => Err(value::Error::Expected {
            got: value.kind(),
            expected: Kind::regex() | Kind::bytes(),
        }
        .into()),
    }
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
                kind: kind::BYTES | kind::REGEX,
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
        _state: &TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let pattern = arguments.required("pattern");
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
                title: "simpl match (pattern as parameter)",
                source: r#"msg = "8.7.6.5 - zorp"; reg = r'^(?P<host>[\w\.]+) - (?P<user>[\w]+)'; parse_regex!(msg, reg)"#,
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
            Example {
                title: "numeric groups (pattern as parameter)",
                source: r#"reg = r'^(?P<host>[\w\.]+) - (?P<user>[\w]+)'; parse_regex!("8.7.6.5 - zorp", reg, numeric_groups: true)"#,
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
}

#[derive(Debug, Clone)]
pub(crate) struct ParseRegexFn {
    value: Box<dyn Expression>,
    pattern: Box<dyn Expression>,
    numeric_groups: Box<dyn Expression>,
}

impl Expression for ParseRegexFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let numeric_groups = self.numeric_groups.resolve(ctx)?;
        let pattern = self.pattern.resolve(ctx)?;

        parse_regex(value, numeric_groups.try_boolean()?, pattern)
    }

    fn type_def(&self, _: &TypeState) -> TypeDef {
        TypeDef::object(Collection::any()).fallible()
    }

    fn type_info(&self, state: &TypeState) -> TypeInfo {
        let fallibility = true;
        let state = state.clone();
        TypeInfo::new(state, TypeDef::regex().with_fallibility(fallibility))
    }
}

impl FunctionExpression for ParseRegexFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let numeric_groups = self.numeric_groups.resolve(ctx)?;
        let pattern = self.pattern.resolve(ctx)?;

        parse_regex(value, numeric_groups.try_boolean()?, pattern)
    }

    fn type_def(&self, state: &state::TypeState) -> TypeDef {
        //TypeDef::object(util::regex_kind(&self.pattern)).fallible()

        //TypeDef::object(Collection::any()).fallible()

        let k: value::Kind = state.external.target_kind().clone();
        TypeDef::from(k).fallible()
    }
}

#[cfg(test)]
#[allow(clippy::trivial_regex)]
mod tests {
    use super::*;

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
            tdef: TypeDef::object(Collection::any()).fallible(),
        }

        single_match {
            args: func_args! [
                value: "first group and second group",
                pattern: Regex::new(r#"(?P<number>.*?) group"#).unwrap()
            ],
            want: Ok(value!({"number": "first"})),
            tdef: TypeDef::object(Collection::any()).fallible(),
        }

        no_match {
            args: func_args! [
                value: "I don't match",
                pattern: Regex::new(r#"^(?P<host>[\w\.]+) - (?P<user>[\w]+) (?P<bytes_in>[\d]+) \[(?P<timestamp>.*)\] "(?P<method>[\w]+) (?P<path>.*)" (?P<status>[\d]+) (?P<bytes_out>[\d]+)$"#)
                            .unwrap()
            ],
            want: Err("could not find any pattern matches"),
            tdef: TypeDef::object(Collection::any()).fallible(),
        }
    ];
}
