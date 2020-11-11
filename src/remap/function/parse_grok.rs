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
                accepts: |v| matches!(v, Value::String(_)),
                required: true,
            },
            Parameter {
                keyword: "pattern",
                accepts: |v| matches!(v, Value::String(_)),
                required: true,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;

        let patternstr = match arguments
            .required_expr("pattern")?
            .literal()
            .ok_or("grok pattern must be a literal string")?
        {
            Value::String(patternstr) => String::from_utf8_lossy(&patternstr).into_owned(),
            _ => {
                // The parameter type check should prevent us from reaching this point.
                return Err(Error::Unknown);
            }
        };

        let mut grok = grok::Grok::with_patterns();
        let pattern = Arc::new(
            grok.compile(&patternstr, true)
                .map_err(|e| Error::from(e.to_string()))?,
        );

        Ok(Box::new(ParseGrokFn {
            value,
            pattern,
        }))
    }
}

#[derive(Debug)]
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

impl Clone for ParseGrokFn {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            pattern: Arc::clone(&self.pattern),
        }
    }
}

impl Expression for ParseGrokFn {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        let value = {
            let bytes = required!(state, object, self.value, Value::String(v) => v);
            String::from_utf8_lossy(&bytes).into_owned()
        };

        match self.pattern.match_against(&value) {
            Some(matches) => {
                let mut result = BTreeMap::new();

                for (name, value) in matches.iter() {
                    result.insert(name.to_string(), Value::from(value));
                }

                Ok(Some(Value::from(result)))
            }
            None => Ok(Some(Value::from(BTreeMap::new()))),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::map;

    #[test]
    fn check_parse_grok() {
        let cases = vec![
            (
                map!["message": "an ungrokkable message"],
                Ok(Some(Value::from(map![]))),
                ParseGrokFn::new(
                    Box::new(Path::from("message")),
                    "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"
                        .to_string(),
                )
                .unwrap(),
            ),
            (
                map!["message": "2020-10-02T23:22:12.223222Z an ungrokkable message"],
                Ok(Some(Value::from(map![]))),
                ParseGrokFn::new(
                    Box::new(Path::from("message")),
                    "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"
                        .to_string(),
                )
                .unwrap(),
            ),
            (
                map!["message": "2020-10-02T23:22:12.223222Z info Hello world"],
                Ok(Some(Value::from(
                    map!["timestamp": "2020-10-02T23:22:12.223222Z",
                         "level": "info",
                         "message": "Hello world"],
                ))),
                ParseGrokFn::new(
                    Box::new(Path::from("message")),
                    "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"
                        .to_string(),
                )
                .unwrap(),
            ),
        ];

        let mut state = remap::State::default();

        for (mut object, exp, func) in cases {
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
