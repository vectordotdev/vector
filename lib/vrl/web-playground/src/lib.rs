use ::value::Value;
use gloo_utils::format::JsValueSerdeExt;
use serde::{Deserialize, Serialize};
use value::Secrets;
use vrl::diagnostic::DiagnosticList;
use vrl::state::TypeState;
use vrl::{diagnostic::Formatter, prelude::BTreeMap, CompileConfig, Runtime};
use vrl::{TargetValue, TimeZone};
use wasm_bindgen::prelude::*;

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
}

// TODO: return diagnostic if fails upon compilation, currently being ignored
fn compile(mut input: Input) -> Result<VrlCompileResult, VrlDiagnosticResult> {
    let event = &mut input.event;
    let functions = stdlib::all();
    let state = TypeState::default();
    let mut runtime = Runtime::default();
    let config = CompileConfig::default();
    let timezone = TimeZone::default();

    let mut diagnostics_res: VrlDiagnosticResult = VrlDiagnosticResult::default();
    let mut target_value = TargetValue {
        value: event.clone(),
        metadata: Value::Object(BTreeMap::new()),
        secrets: Secrets::new(),
    };

    let program = match vrl::compile_with_state(&input.program, &functions, &state, config) {
        Ok(program) => program,
        Err(diagnostics) => {
            diagnostics_res = VrlDiagnosticResult::new(&input.program, diagnostics);
            return Err(diagnostics_res);
        }
    };

    match runtime.resolve(&mut target_value, &program.program, &timezone) {
        Ok(result) => Ok(VrlCompileResult::new(result, target_value.value)),
        Err(_err) => Err(diagnostics_res),
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
