use std::{
    collections::{BTreeMap, HashMap},
    fs,
    process::Command,
};

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::Test;

/// A list of function examples that should be skipped in the test run.
///
/// This mostly consists of functions that have a non-deterministic result.
const SKIP_FUNCTION_EXAMPLES: &[&str] = &[
    "type_def", // Not supported on VM runtime
    "random_bytes",
    "uuid_v4",
    "strip_ansi_escape_codes",
    "get_hostname",
    "now",
    "get_env_var",
];

#[derive(Debug, Deserialize)]
pub struct Reference {
    examples: Vec<Example>,
    functions: HashMap<String, Examples>,
    expressions: HashMap<String, Examples>,
}

#[derive(Debug, Deserialize)]
pub struct Examples {
    examples: Vec<Example>,
}

#[derive(Debug, Deserialize)]
pub struct Example {
    title: String,
    #[serde(default)]
    input: Option<Event>,
    source: String,
    #[serde(rename = "return")]
    returns: Option<Value>,
    output: Option<ExampleOutput>,
    raises: Option<Error>,
    skip_test: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ExampleOutput {
    Events(Vec<Event>),
    Event(Event),
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Event {
    log: Map<String, Value>,

    // TODO: unsupported for now
    metric: Option<Map<String, Value>>,
}

#[derive(Debug, Deserialize)]
pub enum Error {
    #[serde(rename = "compiletime")]
    Compiletime(String),
    #[serde(rename = "runtime")]
    Runtime(String),
}

pub fn tests(ignore_cue: bool) -> Vec<Test> {
    if ignore_cue {
        return vec![];
    }

    let dir = fs::canonicalize("../../../scripts").unwrap();

    let output = Command::new("bash")
        .current_dir(dir)
        .args(["cue.sh", "export", "-e", "remap"])
        .output()
        .expect("failed to execute process");

    if output.stdout.is_empty() {
        Vec::new()
    } else {
        let Reference {
            examples,
            functions,
            expressions,
        } = serde_json::from_slice(&output.stdout).unwrap();

        examples_to_tests("reference", {
            let mut map = HashMap::default();
            map.insert("program".to_owned(), Examples { examples });
            map
        })
        .chain(examples_to_tests("functions", functions))
        .chain(examples_to_tests("expressions", expressions))
        .collect()
    }
}

fn examples_to_tests(
    category: &'static str,
    examples: HashMap<String, Examples>,
) -> Box<dyn Iterator<Item = Test>> {
    Box::new(examples.into_iter().flat_map(move |(k, v)| {
        v.examples
            .into_iter()
            .map(|example| test_from_cue_example(category, k.clone(), example))
            .collect::<Vec<_>>()
    }))
}

fn test_from_cue_example(category: &'static str, name: String, example: Example) -> Test {
    use vrl::value::Value;

    let Example {
        title,
        input,
        mut source,
        returns,
        output,
        raises,
        skip_test,
    } = example;

    let mut skip = skip_test.unwrap_or_else(|| SKIP_FUNCTION_EXAMPLES.contains(&name.as_str()));

    let object = match input {
        Some(event) => {
            serde_json::from_value::<Value>(serde_json::Value::Object(event.log)).unwrap()
        }
        None => Value::Object(BTreeMap::default()),
    };

    if returns.is_some() && output.is_some() {
        panic!(
            "example must either specify return or output, not both: {}/{}",
            category, &name
        );
    }

    if let Some(output) = &output {
        let contains_metric_event = match &output {
            ExampleOutput::Events(events) => events.iter().any(|event| event.metric.is_some()),
            ExampleOutput::Event(event) => event.metric.is_some(),
        };

        if contains_metric_event {
            skip = true;
        }

        // when checking the output, we need to add `.` at the end of the
        // program to make sure we correctly evaluate the external object.
        source += "; .";
    }

    let result = match raises {
        Some(Error::Runtime(error) | Error::Compiletime(error)) => error,
        None => serde_json::to_string(
            &returns
                .or_else(|| {
                    output.map(|output| match output {
                        ExampleOutput::Events(events) => serde_json::Value::Array(
                            events
                                .into_iter()
                                .map(|event| serde_json::Value::Object(event.log))
                                .collect(),
                        ),
                        ExampleOutput::Event(event) => serde_json::Value::Object(event.log),
                    })
                })
                .unwrap_or_default(),
        )
        .unwrap(),
    };

    Test {
        name: title,
        category: format!("docs/{}/{}", category, name),
        error: None,
        source,
        object,
        result,
        result_approx: false,
        skip,
        read_only_paths: vec![],
        check_diagnostics: false,
    }
}
