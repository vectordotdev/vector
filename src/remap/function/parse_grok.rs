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
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();

        let patternbytes = arguments
            .required_literal("pattern")?
            .as_value()
            .clone()
            .try_bytes()?;

        let patternstr = String::from_utf8_lossy(&patternbytes).into_owned();

        let mut grok = grok::Grok::with_patterns();
        let pattern = Arc::new(grok.compile(&patternstr, true).map_err(|e| e.to_string())?);

        Ok(Box::new(ParseGrokFn { value, pattern }))
    }
}

#[derive(Debug, Clone)]
struct ParseGrokFn {
    value: Box<dyn Expression>,
    // Wrapping pattern in an Arc, as cloning the pattern could otherwise be expensive.
    pattern: Arc<grok::Pattern>,
}

impl ParseGrokFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, pattern: String) -> Result<Self> {
        let mut grok = grok::Grok::with_patterns();
        let pattern = Arc::new(
            grok.compile(&pattern, true)
                .map_err(|e| Error::from(e.to_string()))?,
        );

        Ok(Self { value, pattern })
    }
}

impl Expression for ParseGrokFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let bytes = self.value.execute(state, object)?.try_bytes()?;
        let value = String::from_utf8_lossy(&bytes);

        match self.pattern.match_against(&value) {
            Some(matches) => {
                let mut result = BTreeMap::new();

                for (name, value) in matches.iter() {
                    result.insert(name.to_string(), Value::from(value));
                }

                Ok(Value::from(result))
            }
            None => Ok(Value::from(BTreeMap::new())),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(value::Kind::Bytes)
            .with_constraint(value::Kind::Array)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::map;

    remap::test_type_def![string {
        expr: |_| ParseGrokFn {
            value: Literal::from("foo").boxed(),
            pattern: Arc::new(
                grok::Grok::with_patterns()
                    .compile("%{LOGLEVEL:level}", true)
                    .unwrap()
            )
        },
        def: TypeDef {
            kind: value::Kind::Array,
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
                map!["message": "an ungrokkable message"],
                Ok(Value::from(map![])),
                ParseGrokFn::new(
                    Box::new(Path::from("message")),
                    "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"
                        .to_string(),
                )
                .unwrap(),
            ),
            (
                map!["message": "2020-10-02T23:22:12.223222Z an ungrokkable message"],
                Ok(Value::from(map![])),
                ParseGrokFn::new(
                    Box::new(Path::from("message")),
                    "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"
                        .to_string(),
                )
                .unwrap(),
            ),
            (
                map!["message": "2020-10-02T23:22:12.223222Z info Hello world"],
                Ok(Value::from(
                    map!["timestamp": "2020-10-02T23:22:12.223222Z",
                         "level": "info",
                         "message": "Hello world"],
                )),
                ParseGrokFn::new(
                    Box::new(Path::from("message")),
                    "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"
                        .to_string(),
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
