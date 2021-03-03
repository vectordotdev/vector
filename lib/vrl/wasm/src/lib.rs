use serde::{Deserialize, Serialize};
use vrl::Value;
use wasm_bindgen::prelude::*;

#[derive(Deserialize, Serialize)]
struct Input {
    program: String,
    event: Value,
}

fn vrl_resolve(input: Input) -> Result<Value, String> {
    Ok(input.event)
}


#[wasm_bindgen]
pub fn resolve(incoming: &JsValue) -> JsValue {
    let input: Input = incoming.into_serde().unwrap();

    match vrl_resolve(input) {
        Ok(event) => JsValue::from_str(&event.to_string()),
        Err(err) => JsValue::from_str(&err.to_string())
    }
}
