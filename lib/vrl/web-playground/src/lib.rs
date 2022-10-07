use ::value::Value;
use gloo_utils::format::JsValueSerdeExt;
use serde::{Deserialize, Serialize};
use value::Secrets;
use vrl::state::TypeState;
use vrl::{diagnostic::Formatter, prelude::BTreeMap, CompileConfig, Runtime};
use vrl::{TargetValue, TimeZone};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
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

fn compile(mut input: Input) -> Result<VrlCompileResult, String> {
    let event = &mut input.event;
    let functions = stdlib::all();
    let state = TypeState::default();
    let mut runtime = Runtime::default();
    let config = CompileConfig::default();
    let timezone = TimeZone::default();

    let mut target_value = TargetValue {
        value: event.clone(),
        metadata: Value::Object(BTreeMap::new()),
        secrets: Secrets::new(),
    };

    let program = match vrl::compile_with_state(&input.program, &functions, &state, config) {
        Ok(program) => program,
        Err(diagnostics) => {
            let msg = Formatter::new(&input.program, diagnostics)
                .colored()
                .to_string();
            return Err(msg);
        }
    };

    match runtime.resolve(&mut target_value, &program.program, &timezone) {
        Ok(result) => Ok(VrlCompileResult::new(result, target_value.value)),
        Err(err) => Err(err.to_string()),
    }
}

// The user-facing function
#[wasm_bindgen]
pub fn run_vrl(incoming: &JsValue) -> JsValue {
    let input: Input = incoming.into_serde().unwrap();

    match compile(input) {
        Ok(res) => JsValue::from_serde(&res).unwrap(),
        Err(err) => {
            log(&err);
            JsValue::from_serde("invalid vrl").unwrap()
        }
    }
}
