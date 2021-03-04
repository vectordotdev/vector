use serde::{Deserialize, Serialize};
use vrl::{diagnostic::Formatter, state, Runtime, Value};
use vrl_stdlib as stdlib;
use wasm_bindgen::prelude::*;

// The module takes in a VRL program and a VRL event as input
#[derive(Deserialize, Serialize)]
pub struct Input {
    pub program: String,
    pub event: Value,
}

impl Input {
    pub fn new(program: &str, event: Value) -> Self {
        Self { program: program.to_owned(), event }
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

// Errors are output as JSON
#[derive(Deserialize, Serialize)]
pub struct ErrorResult {
    error: String,
}

impl ErrorResult {
    fn new(error: String) -> Self {
        Self { error }
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
        Err(err) => Err(ErrorResult::new(err.to_string())),
    }
}

// The user-facing function
#[wasm_bindgen]
pub fn resolve(incoming: &JsValue) -> JsValue {
    let input: Input = incoming.into_serde().unwrap();

    match compile(input) {
        Ok(res) => JsValue::from_serde(&res).unwrap(),
        Err(err) => JsValue::from_serde(&err).unwrap(),
    }
}
