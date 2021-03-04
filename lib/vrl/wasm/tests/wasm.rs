use std::collections::BTreeMap;
use vrl::Value;
use vrl_wasm::{resolve, Input, VrlCompileResult};
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

macro_rules! to_json {
    ($v:tt) => {
        JsValue::from_serde(&$v).unwrap()
    }
}

macro_rules! from_json {
    ($v:tt) => {
        $v.into_serde().unwrap()
    }
}

macro_rules! test {
    ($program:tt, $event:tt, $expected:tt) => {
        let input = Input::new($program, $event);
        let input_js = to_json!(input);
        let result_js = resolve(&input_js);
        let result: VrlCompileResult = from_json!(result_js);
        assert_eq!(result.result, $expected);
    }
}

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

    for (program, event, expected) in cases {
        test!(program, event, expected);
    }
}
