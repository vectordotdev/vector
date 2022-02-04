use std::{collections::BTreeMap, fmt, sync::Arc};

use vrl::{
    diagnostic::{Label, Span},
    prelude::*,
};

#[derive(Debug)]
pub enum Error {
    InvalidGrokPattern(grok::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidGrokPattern(err) => write!(f, "{}", err),
        }
    }
}

impl std::error::Error for Error {}

impl DiagnosticError for Error {
    fn code(&self) -> usize {
        109
    }

    fn labels(&self) -> Vec<Label> {
        match self {
            Error::InvalidGrokPattern(err) => {
                vec![Label::primary(
                    format!("grok pattern error: {}", err),
                    Span::default(),
                )]
            }
        }
    }
}

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
                keyword: "remove_empty",
                kind: kind::BOOLEAN,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "parse grok pattern",
            source: indoc! {r#"
                value = "2020-10-02T23:22:12.223222Z info Hello world"
                pattern = "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"

                parse_grok!(value, pattern)
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
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        let pattern = arguments
            .required_literal("pattern")?
            .to_value()
            .try_bytes_utf8_lossy()
            .expect("grok pattern not bytes")
            .into_owned();

        let mut grok = grok::Grok::with_patterns();
        let pattern = Arc::new(
            grok.compile(&pattern, true)
                .map_err(|e| Box::new(Error::InvalidGrokPattern(e)) as Box<dyn DiagnosticError>)?,
        );

        let remove_empty = arguments
            .optional("remove_empty")
            .unwrap_or_else(|| expr!(false));

        Ok(Box::new(ParseGrokFn {
            value,
            pattern,
            remove_empty,
        }))
    }
}

#[derive(Clone, Debug)]
struct ParseGrokFn {
    value: Box<dyn Expression>,

    // Wrapping pattern in an Arc, as cloning the pattern could otherwise be expensive.
    pattern: Arc<grok::Pattern>,
    remove_empty: Box<dyn Expression>,
}

impl Expression for ParseGrokFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let bytes = value.try_bytes_utf8_lossy()?;
        let remove_empty = self.remove_empty.resolve(ctx)?.try_boolean()?;

        match self.pattern.match_against(&bytes) {
            Some(matches) => {
                let mut result = BTreeMap::new();

                for (name, value) in matches.iter() {
                    if !remove_empty || !value.is_empty() {
                        result.insert(name.to_string(), Value::from(value));
                    }
                }

                Ok(Value::from(result))
            }
            None => Err("unable to parse input with grok pattern".into()),
        }
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().object::<(), Kind>(map! {
            (): Kind::all(),
        })
    }
}

#[cfg(test)]
mod test {
    use vector_common::btreemap;

    use super::*;

    test_function![
        parse_grok => ParseGrok;

        invalid_grok {
            args: func_args![ value: "foo",
                              pattern: "%{NOG}"],
            want: Err("The given pattern definition name \"NOG\" could not be found in the definition map"),
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all(),
            }),
        }

        error {
            args: func_args![ value: "an ungrokkable message",
                              pattern: "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"],
            want: Err("unable to parse input with grok pattern"),
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all(),
            }),
        }

        error2 {
            args: func_args![ value: "2020-10-02T23:22:12.223222Z an ungrokkable message",
                              pattern: "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"],
            want: Err("unable to parse input with grok pattern"),
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all(),
            }),
        }

        parsed {
            args: func_args![ value: "2020-10-02T23:22:12.223222Z info Hello world",
                              pattern: "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"],
            want: Ok(Value::from(btreemap! {
                "timestamp" => "2020-10-02T23:22:12.223222Z",
                "level" => "info",
                "message" => "Hello world",
            })),
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all(),
            }),
        }

        parsed2 {
            args: func_args![ value: "2020-10-02T23:22:12.223222Z",
                              pattern: "(%{TIMESTAMP_ISO8601:timestamp}|%{LOGLEVEL:level})"],
            want: Ok(Value::from(btreemap! {
                "timestamp" => "2020-10-02T23:22:12.223222Z",
                "level" => "",
            })),
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all(),
            }),
        }

        remove_empty {
            args: func_args![ value: "2020-10-02T23:22:12.223222Z",
                              pattern: "(%{TIMESTAMP_ISO8601:timestamp}|%{LOGLEVEL:level})",
                              remove_empty: true,
            ],
            want: Ok(Value::from(
                btreemap! { "timestamp" => "2020-10-02T23:22:12.223222Z" },
            )),
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all(),
            }),
        }
    ];
}
