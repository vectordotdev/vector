use std::collections::BTreeMap;

use gloo_utils::format::JsValueSerdeExt;
use serde::{Deserialize, Serialize};
use vrl::{
    compiler::{
        CompileConfig, TargetValue, TimeZone, TypeState, compile_with_state,
        runtime::{Runtime, Terminate},
    },
    diagnostic::{DiagnosticList, Formatter},
    value::{Secrets, Value},
};
use wasm_bindgen::prelude::*;

pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

#[derive(Serialize, Deserialize)]
pub struct Input {
    pub program: String,
    pub event: Value,
}

impl Input {
    pub fn new(program: &str, event: Value) -> Self {
        Self {
            program: program.to_owned(),
            event,
        }
    }
}

// The module returns the result of the last expression, the resulting event,
// and the execution time.
#[derive(Deserialize, Serialize)]
pub struct VrlCompileResult {
    // Pre-serialized target_value to avoid f64 precision loss in JS JSON.parse.
    pub output: String,
    pub runtime_result: Value,
    pub target_value: Value,
    pub elapsed_time: Option<f64>,
}

// Serialize a `Value` to pretty JSON using tab indentation, matching the layout the
// playground previously produced with `JSON.stringify(value, null, "\t")`.
fn to_pretty_json(value: &Value) -> Result<String, serde_json::Error> {
    let mut buf = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"\t");
    let mut serializer = serde_json::Serializer::with_formatter(&mut buf, formatter);
    value.serialize(&mut serializer)?;
    // `serde_json` only ever writes valid UTF-8.
    Ok(String::from_utf8(buf).expect("serde_json emits valid UTF-8"))
}

#[derive(Deserialize, Serialize, Default)]
pub struct VrlDiagnosticResult {
    pub list: Vec<String>,
    pub msg: String,
    pub msg_colorized: String,
}

impl VrlDiagnosticResult {
    fn new(program: &str, diagnostic_list: DiagnosticList) -> Self {
        Self {
            list: diagnostic_list
                .clone()
                .into_iter()
                .map(|diag| String::from(diag.message()))
                .collect(),
            msg: Formatter::new(program, diagnostic_list.clone()).to_string(),
            msg_colorized: Formatter::new(program, diagnostic_list)
                .colored()
                .to_string(),
        }
    }

    fn new_runtime_error(program: &str, terminate: Terminate) -> Self {
        Self {
            list: Vec::with_capacity(1),
            msg: Formatter::new(program, terminate.clone().get_expression_error()).to_string(),
            msg_colorized: Formatter::new(program, terminate.get_expression_error())
                .colored()
                .to_string(),
        }
    }
}

fn compile(
    mut input: Input,
    tz_str: Option<String>,
) -> Result<VrlCompileResult, VrlDiagnosticResult> {
    let functions = vector_vrl_functions::all();

    let event = &mut input.event;
    let state = TypeState::default();
    let mut runtime = Runtime::default();
    let config = CompileConfig::default();

    let timezone = match tz_str.as_deref() {
        // Empty or "Default" tz string will default to tz default
        None | Some("") | Some("Default") => TimeZone::default(),
        Some(other) => match other.parse() {
            Ok(tz) => TimeZone::Named(tz),
            Err(_) => {
                // Returns error message if tz parsing has failed.
                // This avoids head scratching, instead of it silently using the default timezone.
                let error_message = format!("Invalid timezone identifier: '{other}'");
                return Err(VrlDiagnosticResult {
                    list: vec![error_message.clone()],
                    msg: error_message.clone(),
                    msg_colorized: error_message,
                });
            }
        },
    };

    let mut target_value = TargetValue {
        value: event.clone(),
        metadata: Value::Object(BTreeMap::new()),
        secrets: Secrets::new(),
    };

    let compilation_result = match compile_with_state(&input.program, &functions, &state, config) {
        Ok(result) => result,
        Err(diagnostics) => return Err(VrlDiagnosticResult::new(&input.program, diagnostics)),
    };

    let (result, elapsed_time) =
        if let Some(performance) = web_sys::window().and_then(|w| w.performance()) {
            let start_time = performance.now();
            let result = runtime.resolve(&mut target_value, &compilation_result.program, &timezone);
            let end_time = performance.now();
            (result, Some(end_time - start_time))
        } else {
            // If performance API is not available, run the program without timing.
            let result = runtime.resolve(&mut target_value, &compilation_result.program, &timezone);
            (result, None)
        };

    match result {
        Ok(runtime_result) => {
            // Full-precision JSON for display.
            let output = to_pretty_json(&target_value.value).map_err(|err| {
                let msg = format!("failed to serialize result: {err}");
                VrlDiagnosticResult {
                    msg_colorized: msg.clone(),
                    msg,
                    ..Default::default()
                }
            })?;
            Ok(VrlCompileResult {
                output,
                runtime_result, // This is the value of the last expression.
                target_value: target_value.value, // The value of the final event
                elapsed_time,
            })
        }
        Err(err) => Err(VrlDiagnosticResult::new_runtime_error(&input.program, err)),
    }
}

// The user-facing function
#[wasm_bindgen]
pub fn run_vrl(incoming: &JsValue, tz_str: &str) -> JsValue {
    let input: Input = incoming.into_serde().unwrap();

    match compile(input, Some(tz_str.to_string())) {
        Ok(res) => JsValue::from_serde(&res).unwrap(),
        Err(err) => JsValue::from_serde(&err).unwrap(),
    }
}

#[wasm_bindgen]
pub fn vector_version() -> String {
    built_info::VECTOR_VERSION.to_string()
}

#[wasm_bindgen]
pub fn vector_link() -> String {
    built_info::VECTOR_LINK.to_string()
}

#[wasm_bindgen]
pub fn vrl_version() -> String {
    built_info::VRL_VERSION.to_string()
}

#[wasm_bindgen]
pub fn vrl_link() -> String {
    built_info::VRL_LINK.to_string()
}

#[cfg(test)]
mod tests {
    // `compile`/`run_vrl` can't be exercised here: they call `web_sys::window()`, which
    // panics on non-wasm targets, so they need a `wasm-bindgen-test` harness.
    use super::*;
    use vrl::value::ObjectMap;

    // A scalar integer larger than f64's safe range must serialize to its exact digits.
    // JS `JSON.parse` (used when marshaling the wasm result) would round these; see
    // https://github.com/vectordotdev/vrl/issues/1535.
    #[test]
    fn to_pretty_json_preserves_large_integers() {
        // An XXH64 hash (`xxhash("foo", "XXH64")`).
        assert_eq!(
            to_pretty_json(&Value::from(3_728_699_739_546_630_719_i64)).unwrap(),
            "3728699739546630719"
        );
        // A seahash whose `u64 as i64` reinterpretation wrapped negative (`seahash("bar")`).
        assert_eq!(
            to_pretty_json(&Value::from(-2_796_170_501_982_571_315_i64)).unwrap(),
            "-2796170501982571315"
        );
    }

    // An empty event renders the same as before (`JSON.stringify({}, null, "\t")`).
    #[test]
    fn to_pretty_json_renders_empty_object() {
        assert_eq!(
            to_pretty_json(&Value::Object(ObjectMap::new())).unwrap(),
            "{}"
        );
    }
}
