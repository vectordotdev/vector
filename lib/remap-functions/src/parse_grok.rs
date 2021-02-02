use remap::prelude::*;
use std::collections::BTreeMap;
use std::sync::Arc;

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
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "pattern",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "remove_empty",
                accepts: |v| matches!(v, Value::Boolean(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();

        let pattern = arguments
            .required_literal("pattern")?
            .as_value()
            .clone()
            .try_bytes_utf8_lossy()?
            .into_owned();

        let mut grok = grok::Grok::with_patterns();
        let pattern = Arc::new(grok.compile(&pattern, true).map_err(|e| e.to_string())?);

        let remove_empty = arguments.optional("remove_empty").map(Expr::boxed);

        Ok(Box::new(ParseGrokFn {
            value,
            pattern,
            remove_empty,
        }))
    }
}

#[derive(Debug, Clone)]
struct ParseGrokFn {
    value: Box<dyn Expression>,
    // Wrapping pattern in an Arc, as cloning the pattern could otherwise be expensive.
    pattern: Arc<grok::Pattern>,
    remove_empty: Option<Box<dyn Expression>>,
}

impl ParseGrokFn {
    #[cfg(test)]
    fn new(
        value: Box<dyn Expression>,
        pattern: String,
        remove_empty: Option<Box<dyn Expression>>,
    ) -> Result<Self> {
        let mut grok = grok::Grok::with_patterns();
        let pattern = Arc::new(
            grok.compile(&pattern, true)
                .map_err(|e| Error::from(e.to_string()))?,
        );

        Ok(Self {
            value,
            pattern,
            remove_empty,
        })
    }
}

impl Expression for ParseGrokFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let bytes = self.value.execute(state, object)?.try_bytes()?;
        let value = String::from_utf8_lossy(&bytes);
        let remove_empty = match &self.remove_empty {
            Some(expr) => expr.execute(state, object)?.try_boolean()?,
            None => false,
        };

        match self.pattern.match_against(&value) {
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

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .into_fallible(true)
            .with_constraint(value::Kind::Map)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use shared::btreemap;

    remap::test_type_def![string {
        expr: |_| ParseGrokFn {
            value: Literal::from("foo").boxed(),
            pattern: Arc::new(
                grok::Grok::with_patterns()
                    .compile("%{LOGLEVEL:level}", true)
                    .unwrap()
            ),
            remove_empty: Some(Literal::from(false).boxed()),
        },
        def: TypeDef {
            kind: value::Kind::Map,
            fallible: true,
            ..Default::default()
        },
    }];

    #[test]
    fn check_invalid_grok_error() {
        let mut arguments = ArgumentList::default();
        arguments.insert(
            "value",
            expression::Argument::new(
                Box::new(Literal::from("foo").into()),
                |_| true,
                "value",
                "parse_grok",
            )
            .into(),
        );
        arguments.insert(
            "pattern",
            expression::Argument::new(
                Box::new(Literal::from("%{NOG}").into()),
                |_| true,
                "pattern",
                "parse_grok",
            )
            .into(),
        );

        let error = ParseGrok.compile(arguments);

        assert_eq!(Error::Call("The given pattern definition name \"NOG\" could not be found in the definition map".to_string()), error.unwrap_err());
    }

    #[test]
    fn check_parse_grok() {
        let cases = vec![
            (
                btreemap! { "message" => "an ungrokkable message" },
                Err("function call error: unable to parse input with grok pattern".into()),
                ParseGrokFn::new(
                    Box::new(Path::from("message")),
                    "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"
                        .to_string(),
                    None,
                )
                .unwrap(),
            ),
            (
                btreemap! { "message" => "2020-10-02T23:22:12.223222Z an ungrokkable message" },
                Err("function call error: unable to parse input with grok pattern".into()),
                ParseGrokFn::new(
                    Box::new(Path::from("message")),
                    "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"
                        .to_string(),
                    None,
                )
                .unwrap(),
            ),
            (
                btreemap! { "message" => "2020-10-02T23:22:12.223222Z info Hello world" },
                Ok(Value::from(btreemap! {
                    "timestamp" => "2020-10-02T23:22:12.223222Z",
                    "level" => "info",
                    "message" => "Hello world",
                })),
                ParseGrokFn::new(
                    Box::new(Path::from("message")),
                    "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"
                        .to_string(),
                    None,
                )
                .unwrap(),
            ),
            (
                btreemap! { "message" => "2020-10-02T23:22:12.223222Z" },
                Ok(Value::from(btreemap! {
                    "timestamp" => "2020-10-02T23:22:12.223222Z",
                    "level" => "",
                })),
                ParseGrokFn::new(
                    Box::new(Path::from("message")),
                    "(%{TIMESTAMP_ISO8601:timestamp}|%{LOGLEVEL:level})".to_string(),
                    None,
                )
                .unwrap(),
            ),
            (
                btreemap! { "message" => "2020-10-02T23:22:12.223222Z" },
                Ok(Value::from(
                    btreemap! { "timestamp" => "2020-10-02T23:22:12.223222Z" },
                )),
                ParseGrokFn::new(
                    Box::new(Path::from("message")),
                    "(%{TIMESTAMP_ISO8601:timestamp}|%{LOGLEVEL:level})".to_string(),
                    Some(Literal::from(true).boxed()),
                )
                .unwrap(),
            ),
        ];

        let mut state = state::Program::default();

        for (object, exp, func) in cases {
            let mut object = Value::Map(object);
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
