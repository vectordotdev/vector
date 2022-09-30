use ::value::Value;
use value::Secrets;

use vrl::state::TypeState;
use vrl::{diagnostic::Formatter, prelude::BTreeMap, CompileConfig, Runtime};
use vrl::{TargetValue, TimeZone, VrlRuntime};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    // Use `js_namespace` here to bind `console.log(..)` instead of just
    // `log(..)`
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);

    // The `console.log` is quite polymorphic, so we can bind it with multiple
    // signatures. Note that we need to use `js_name` to ensure we always call
    // `log` in JS.
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log_u32(a: u32);

    // Multiple arguments too!
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log_many(a: &str, b: &str);
}

#[wasm_bindgen]
pub fn run_vrl(program: &str) -> String {
    let mut functions = stdlib::all();
    let state = TypeState::default();
    let mut runtime = Runtime::default();
    let mut config = CompileConfig::default();
    let mut object = TargetValue {
        value: Value::Null,
        metadata: Value::Object(BTreeMap::new()),
        secrets: Secrets::new(),
    };
    let mut timezone = TimeZone::default();
    let runtime_ref = &mut runtime;
    let vrl_runtime = VrlRuntime::default();
    match vrl::compile_with_state(program, &functions, &state, config) {
        Ok(result) => {
            let program_obj = result.program;

            let res = match vrl_runtime {
                VrlRuntime::Ast => runtime_ref
                    .resolve(&mut object, &program_obj, &timezone)
                    .map_err(|err| err.to_string()),
            };

            match res {
                Ok(value) => value.to_string(),
                Err(e) => (e.to_string()),
            }
        }
        Err(diagnostics) => {
            // see about console.log()
            return Formatter::new(program, diagnostics).colored().to_string();
        }
    }
}
