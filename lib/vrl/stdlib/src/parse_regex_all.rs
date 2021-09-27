use regex::Regex;
use vrl::prelude::*;

use crate::util;

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
        _state: &state::Compiler,
        _info: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let pattern = arguments.required_regex("pattern")?;
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
        ]
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ParseRegexAllFn {
    value: Box<dyn Expression>,
    pattern: Regex,
    numeric_groups: Box<dyn Expression>,
}

impl Expression for ParseRegexAllFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?.try_bytes()?;
        let value = String::from_utf8_lossy(&bytes);
        let numeric_groups = self.numeric_groups.resolve(ctx)?.try_boolean()?;

        Ok(self
            .pattern
            .captures_iter(&value)
            .map(|capture| {
                util::capture_regex_to_map(&self.pattern, capture, numeric_groups).into()
            })
            .collect::<Vec<Value>>()
            .into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        let inner_type_def = TypeDef::new()
            .object(util::regex_type_def(&self.pattern))
            .add_null();

        TypeDef::new()
            .fallible()
            .array_mapped::<(), TypeDef>(map![(): inner_type_def])
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
            tdef: TypeDef::new()
                .fallible()
                .array_mapped::<(), TypeDef>(map![(): TypeDef::new()
                                                  .object::<&str, Kind>(map! {
                                                      "fruit": Kind::Bytes,
                                                      "veg": Kind::Bytes,
                                                      "0": Kind::Bytes | Kind::Null,
                                                      "1": Kind::Bytes | Kind::Null,
                                                      "2": Kind::Bytes | Kind::Null,
                                                  })
                                                  .add_null()
            ]),
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
            tdef: TypeDef::new()
                .fallible()
                .array_mapped::<(), TypeDef>(map![(): TypeDef::new()
                                                  .object::<&str, Kind>(map! {
                                                      "fruit": Kind::Bytes,
                                                      "veg": Kind::Bytes,
                                                      "0": Kind::Bytes | Kind::Null,
                                                      "1": Kind::Bytes | Kind::Null,
                                                      "2": Kind::Bytes | Kind::Null,
                                                  })
                                                  .add_null()
            ]),
        }

        no_matches {
            args: func_args![
                value: "I don't match",
                pattern: Regex::new(r#"(?P<fruit>[\w\.]+) and (?P<veg>[\w]+)"#).unwrap()
            ],
            want: Ok(value!([])),
            tdef: TypeDef::new()
                .fallible()
                .array_mapped::<(), TypeDef>(map![(): TypeDef::new()
                                                  .object::<&str, Kind>(map! {
                                                      "fruit": Kind::Bytes,
                                                      "veg": Kind::Bytes,
                                                      "0": Kind::Bytes | Kind::Null,
                                                      "1": Kind::Bytes | Kind::Null,
                                                      "2": Kind::Bytes | Kind::Null,
                                                  })
                                                  .add_null()
                ]),
        }
    ];
}
