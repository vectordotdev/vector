use ::value::Value;
use regex::Regex;
use vrl::prelude::*;
use vrl::state::{TypeInfo, TypeState};

use crate::util;

fn parse_regex_all(value: Value, numeric_groups: bool, pattern: Value) -> Resolved {
    let bytes = value.try_bytes()?;
    let value = String::from_utf8_lossy(&bytes);
    match pattern {
        Value::Bytes(bytes) => {
            let pattern = Regex::new(&String::from_utf8_lossy(&bytes)).unwrap();
            let parsed = pattern
                .captures_iter(&value)
                .map(|capture| {
                    util::capture_regex_to_map(&pattern, &capture, numeric_groups).into()
                })
                .collect::<Vec<Value>>();
            Ok(parsed.into())
        }
        Value::Regex(regex) => {
            let parsed = regex
                .captures_iter(&value)
                .map(|capture| util::capture_regex_to_map(&regex, &capture, numeric_groups).into())
                .collect::<Vec<Value>>();
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
pub struct ParseRegexAll;

impl Function for ParseRegexAll {
    fn identifier(&self) -> &'static str {
        "parse_regex_all"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::ANY,
                required: true,
            },
            Parameter {
                keyword: "pattern",
                kind: kind::ANY,
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

        Ok(Box::new(ParseRegexAllFn {
            value,
            pattern,
            numeric_groups,
        }))
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "Simple match",
                source: r#"parse_regex_all!("apples and carrots, peaches and peas", r'(?P<fruit>[\w\.]+) and (?P<veg>[\w]+)')"#,
                result: Ok(indoc! { r#"[
               {"fruit": "apples",
                "veg": "carrots"},
               {"fruit": "peaches",
                "veg": "peas"}]"# }),
            },
            Example {
                title: "Simple match (pattern as parameter)",
                source: r#"msg = "apples and carrots, peaches and peas"; reg = r'(?P<fruit>[\w\.]+) and (?P<veg>[\w]+)'; parse_regex_all!(msg, reg)"#,
                result: Ok(indoc! { r#"[
               {"fruit": "apples",
                "veg": "carrots"},
               {"fruit": "peaches",
                "veg": "peas"}]"# }),
            },
            Example {
                title: "Numeric groups",
                source: r#"parse_regex_all!("apples and carrots, peaches and peas", r'(?P<fruit>[\w\.]+) and (?P<veg>[\w]+)', numeric_groups: true)"#,
                result: Ok(indoc! { r#"[
               {"fruit": "apples",
                "veg": "carrots",
                "0": "apples and carrots",
                "1": "apples",
                "2": "carrots"},
               {"fruit": "peaches",
                "veg": "peas",
                "0": "peaches and peas",
                "1": "peaches",
                "2": "peas"}]"# }),
            },
            Example {
                title: "Numeric groups (pattern as parameter)",
                source: r#"reg = r'(?P<fruit>[\w\.]+) and (?P<veg>[\w]+)'; parse_regex_all!("apples and carrots, peaches and peas", reg, numeric_groups: true)"#,
                result: Ok(indoc! { r#"[
               {"fruit": "apples",
                "veg": "carrots",
                "0": "apples and carrots",
                "1": "apples",
                "2": "carrots"},
               {"fruit": "peaches",
                "veg": "peas",
                "0": "peaches and peas",
                "1": "peaches",
                "2": "peas"}]"# }),
            },
        ]
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ParseRegexAllFn {
    value: Box<dyn Expression>,
    pattern: Box<dyn Expression>,
    numeric_groups: Box<dyn Expression>,
}

impl Expression for ParseRegexAllFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let numeric_groups = self.numeric_groups.resolve(ctx)?;
        let pattern = self.pattern.resolve(ctx)?;

        parse_regex_all(value, numeric_groups.try_boolean()?, pattern)
    }

    fn type_def(&self, _: &TypeState) -> TypeDef {
        TypeDef::array(Collection::any()).fallible()
    }

    fn type_info(&self, state: &TypeState) -> TypeInfo {
        let fallibility = true;
        let state = state.clone();
        TypeInfo::new(state, TypeDef::regex().with_fallibility(fallibility))
    }
}

impl FunctionExpression for ParseRegexAllFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let numeric_groups = self.numeric_groups.resolve(ctx)?;
        let pattern = self.pattern.resolve(ctx)?;

        parse_regex_all(value, numeric_groups.try_boolean()?, pattern)
    }

    fn type_def(&self, state: &state::TypeState) -> TypeDef {
        /*
        TypeDef::array(Collection::from_unknown(
            Kind::object(util::regex_kind(&self.pattern)).or_null(),
        ))
        .fallible()
        */

        let kind = state.external.target_kind().clone();
        TypeDef::array(Collection::from_unknown(kind)).fallible()
    }
}

#[cfg(test)]
#[allow(clippy::trivial_regex)]
mod tests {
    use super::*;

    test_function![
        parse_regex_all => ParseRegexAll;

        matches {
            args: func_args![
                value: "apples and carrots, peaches and peas",
                pattern: Regex::new(r#"(?P<fruit>[\w\.]+) and (?P<veg>[\w]+)"#).unwrap(),
            ],
            want: Ok(value!([{"fruit": "apples",
                              "veg": "carrots"},
                             {"fruit": "peaches",
                              "veg": "peas"}])),
            tdef: TypeDef::array(Collection::any()).fallible(),
        }

        numeric_groups {
            args: func_args![
                value: "apples and carrots, peaches and peas",
                pattern: Regex::new(r#"(?P<fruit>[\w\.]+) and (?P<veg>[\w]+)"#).unwrap(),
                numeric_groups: true
            ],
            want: Ok(value!([{"fruit": "apples",
                              "veg": "carrots",
                              "0": "apples and carrots",
                              "1": "apples",
                              "2": "carrots"},
                             {"fruit": "peaches",
                              "veg": "peas",
                              "0": "peaches and peas",
                              "1": "peaches",
                              "2": "peas"}])),
            tdef: TypeDef::array(Collection::any()).fallible(),
        }

        no_matches {
            args: func_args![
                value: "I don't match",
                pattern: Regex::new(r#"(?P<fruit>[\w\.]+) and (?P<veg>[\w]+)"#).unwrap()
            ],
            want: Ok(value!([])),
            tdef: TypeDef::array(Collection::any()).fallible(),
        }
    ];
}
