use gloo_utils::format::JsValueSerdeExt;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vrl::compiler::runtime::{Runtime, Terminate};
use vrl::compiler::TimeZone;
use vrl::compiler::{compile_with_state, CompileConfig, TargetValue, TypeState};
use vrl::diagnostic::DiagnosticList;
use vrl::diagnostic::Formatter;
use vrl::value::Secrets;
use vrl::value::Value;
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

// The module returns the result of the last expression and the event that results from the
// applied program
#[derive(Deserialize, Serialize)]
pub struct VrlCompileResult {
    pub output: Value,
    pub result: Value,
}

impl VrlCompileResult {
    fn new(output: Value, result: Value) -> Self {
        Self { output, result }
    }
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

fn compile(mut input: Input) -> Result<VrlCompileResult, VrlDiagnosticResult> {
    let mut functions = vrl::stdlib::all();
    functions.extend(vector_vrl_functions::all());
    functions.extend(enrichment::vrl_functions());
    functions.extend(vrl_cache::vrl_functions());

    let event = &mut input.event;
    let state = TypeState::default();
    let mut runtime = Runtime::default();
    let config = CompileConfig::default();
    let timezone = TimeZone::default();

    let mut target_value = TargetValue {
        value: event.clone(),
        metadata: Value::Object(BTreeMap::new()),
        secrets: Secrets::new(),
    };

    let program = match compile_with_state(&input.program, &functions, &state, config) {
        Ok(program) => program,
        Err(diagnostics) => return Err(VrlDiagnosticResult::new(&input.program, diagnostics)),
    };

    match runtime.resolve(&mut target_value, &program.program, &timezone) {
        Ok(result) => Ok(VrlCompileResult::new(result, target_value.value)),
        Err(err) => Err(VrlDiagnosticResult::new_runtime_error(&input.program, err)),
    }
}

// The user-facing function
#[wasm_bindgen]
pub fn run_vrl(incoming: &JsValue) -> JsValue {
    let input: Input = incoming.into_serde().unwrap();

    match compile(input) {
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
