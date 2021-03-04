use indoc::indoc;
use vrl::{value, Value};
use vrl_wasm::{resolve, Input, VrlCompileResult};
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

#[wasm_bindgen_test]
fn vrl_resolve_success() {
    let cases: Vec<(&str, Value, Value, Value)> = vec![
        (
            ".",
            value!({}),
            value!({}),
            value!({}),
        ),
        (
            r#".foo = "bar""#, // Program
            value!("bar"), // Expected output
            value!({"bar": "baz"}), // Input event
            value!({"foo": "bar", "bar": "baz"}) // Expected output event
        ),
        (
            indoc! {r#"
                del(.number)
                .quote = upcase(string!(.quote))
                .boolean = false
            "#},
            value!(false),
            value!({"number": 37, "boolean": true, "quote": "testing"}),
            value!({"boolean": false, "quote": "TESTING"}),
        ),
    ];

    for (program, expected_output, event, expected_event) in cases {
        let input = Input::new(program, event);
        let input_js = JsValue::from_serde(&input).unwrap();
        let result_js = resolve(&input_js);
        let result: VrlCompileResult = result_js.into_serde().unwrap();
        assert_eq!(result.result, expected_event);
        assert_eq!(result.output, expected_output);
    }
}
