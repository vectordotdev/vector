use std::collections::BTreeMap;
use vrl::Value;
use vrl_wasm::{resolve, Input, VrlCompileResult};
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

#[wasm_bindgen_test]
fn vrl_resolve() {
    let mut event: BTreeMap<String, Value> = BTreeMap::new();
    event.insert("bar".to_owned(), "baz".to_owned().into());

    let mut expected: BTreeMap<String, Value> = BTreeMap::new();
    expected.insert("bar".to_owned(), "baz".to_owned().into());
    expected.insert("foo".to_owned(), "bar".to_owned().into());

    let cases: Vec<(&str, Value, Value)> = vec![
        (r#".foo = "bar""#, Value::Object(event), Value::Object(expected)),
    ];

    for (program, event, expected_event) in cases {
        let input = Input::new(program, event);
        let input_js = JsValue::from_serde(&input).unwrap();
        let result_js = resolve(&input_js);
        let result: VrlCompileResult = result_js.into_serde().unwrap();
        assert_eq!(result.result, expected_event);
    }
}
