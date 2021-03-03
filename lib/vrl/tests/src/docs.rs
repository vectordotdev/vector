use serde::Deserialize;
use std::fs;
use std::process::Command;
// use vrl::Value;
use serde_json::{Map, Value};

use crate::Test;

#[derive(Debug, Deserialize)]
pub struct Example {
    title: String,
    input: Map<String, Value>,
    source: String,
    output: Option<Map<String, Value>>,
    raises: Option<Map<String, Value>>,
}

pub struct Function;

pub fn tests() -> Vec<Test> {
    let dir = fs::canonicalize("../../../scripts").unwrap();

    let output = Command::new("bash")
        .current_dir(dir)
        .args(&["cue.sh", "export", "-e", "remap"])
        .output()
        .expect("failed to execute process");

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    let examples: Vec<_> = json
        .pointer("/examples")
        .unwrap()
        .as_array()
        .cloned()
        .unwrap_or_else(|| vec![])
        .into_iter()
        .map(|value| serde_json::from_value(value).unwrap())
        .map(|example| Test::from_cue_example(example))
        .collect();

    examples
}

impl Test {
    fn from_cue_example(example: Example) -> Self {
        use vrl::Value;

        let mut skip = false;
        let mut error = None;
        let object: Value = example
            .input
            .get("log")
            .cloned()
            .map(|value| match serde_json::from_value(value) {
                Ok(value) => value,
                Err(err) => {
                    error = Some(format!("unable to parse log as JSON: {}", err));
                    Value::Null
                }
            })
            .unwrap_or_else(|| {
                // TODO: Add support for metric tests.
                skip = true;
                Value::Null
            });

        let result = match example.output {
            Some(output) => output
                .get("log")
                .cloned()
                .map(|value| match serde_json::from_value::<Value>(value) {
                    Ok(value) => serde_json::to_string(&value).unwrap(),
                    Err(err) => {
                        error = Some(format!("unable to parse log as JSON: {}", err));
                        Value::Null.to_string()
                    }
                })
                .unwrap_or_else(|| {
                    // TODO: Add support for metric tests.
                    skip = true;
                    Value::Null.to_string()
                }),
            None => match example.raises {
                Some(raises) => raises
                    .get("compiletime")
                    .cloned()
                    .map(|value| match serde_json::from_value::<Value>(value) {
                        Ok(value) => value
                            .try_bytes_utf8_lossy()
                            .unwrap_or_default()
                            .into_owned(),
                        Err(err) => {
                            error = Some(format!("unable to parse compiletime as JSON: {}", err));
                            "".to_owned()
                        }
                    })
                    .unwrap_or_else(|| {
                        error = Some("compiletime field expected".to_owned());
                        Value::Null.to_string()
                    }),
                None => {
                    error = Some("one of output or raises field must be present".to_owned());
                    Value::Null.to_string()
                }
            },
        };

        Self {
            name: example.title,
            category: "docs/examples".to_owned(),
            error,
            source: example.source,
            object,
            result,
            result_approx: false,
            skip,
        }
    }
}
