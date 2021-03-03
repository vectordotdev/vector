use serde::{Deserialize, Serialize};
use vrl::{diagnostic::Formatter, state, Runtime, Value};
use vrl_stdlib as stdlib;
use wasm_bindgen::prelude::*;

#[derive(Deserialize, Serialize)]
struct Input {
    program: String,
    event: Value,
}

#[derive(Deserialize, Serialize)]
struct VrlCompileResult {
    output: Value,
    result: Value,
}

#[derive(Deserialize, Serialize)]
struct ErrorResult {
    error: String,
}

impl ErrorResult {
    fn new(error: String) -> Self {
        Self { error }
    }
}

impl VrlCompileResult {
    fn new(output: Value, result: Value) -> Self {
        Self { output, result }
    }
}

fn compile(mut input: Input) -> Result<VrlCompileResult, ErrorResult> {
    let event = &mut input.event;
    let mut state = state::Compiler::default();
    let mut runtime = Runtime::new(state::Runtime::default());
    let program = match vrl::compile_with_state(&input.program, &stdlib::all(), &mut state) {
        Ok(program) => program,
        Err(diagnostics) => {
            let msg = Formatter::new(&input.program, diagnostics).to_string();
            return Err(ErrorResult::new(msg));
        }
    };

    match runtime.resolve(event, &program) {
        Ok(result) => Ok(VrlCompileResult::new(result, event.clone())),
        Err(err) => Err(ErrorResult::new(err.to_string()))
    }
}

#[wasm_bindgen]
pub fn resolve(incoming: &JsValue) -> JsValue {
    let input: Input = incoming.into_serde().unwrap();

    match compile(input) {
        Ok(res) => JsValue::from_serde(&res).unwrap(),
        Err(err) => JsValue::from_serde(&err).unwrap()
    }
}
