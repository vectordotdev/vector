use indoc::indoc;
use ::value::Value;
use gloo_utils::format::JsValueSerdeExt;
use vrl::{value};
use crate::{run_vrl, Input, VrlCompileResult};
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

// look up the wasm bindgen test
#[wasm_bindgen_test]
fn vrl_resolve_success() {
    let cases: Vec<(Value, &str, Value, Value)> = vec![
        (value!({}), ".", value!({}), value!({})),
        (
            value!({"bar": "baz"}),               // Input event
            r#".foo = "bar""#,                    // Program
            value!({"foo": "bar", "bar": "baz"}), // Expected output event
            value!("bar"),                        // Expected output
        ),
        (
            value!({"number": 37, "boolean": true, "quote": "testing"}),
            indoc! {r#"
                del(.number)
                .quote = upcase(string!(.quote))
                .boolean = false
            "#},
            value!({"boolean": false, "quote": "TESTING"}),
            value!(false),
        ),
    ];

    for (event, program, expected_event, expected_output) in cases {
        let input = Input::new(program, event);
        let input_js = JsValue::from_serde(&input).unwrap();
        let result_js = run_vrl(&input_js);
        let result: VrlCompileResult = result_js.into_serde().unwrap();
        assert_eq!(result.result, expected_event);
        assert_eq!(result.output, expected_output);
    }
}

// We can revisit these tests when we have a less cumbersome way to validate error output
#[wasm_bindgen_test]
fn vrl_resolve_failure() {
    let cases: Vec<(Value, &str, &str)> = vec![
        (value!({}), "1/0", "\n\x1B[0m\x1B[1m\x1B[38;5;9merror[E100]\x1B[0m\x1B[1m: unhandled error\x1B[0m\n  \x1B[0m\x1B[34m┌─\x1B[0m :1:1\n  \x1B[0m\x1B[34m│\x1B[0m\n\x1B[0m\x1B[34m1\x1B[0m \x1B[0m\x1B[34m│\x1B[0m \x1B[0m\x1B[31m1/0\x1B[0m\n  \x1B[0m\x1B[34m│\x1B[0m \x1B[0m\x1B[31m^^^\x1B[0m\n  \x1B[0m\x1B[34m│\x1B[0m \x1B[0m\x1B[31m│\x1B[0m\n  \x1B[0m\x1B[34m│\x1B[0m \x1B[0m\x1B[31mexpression can result in runtime error\x1B[0m\n  \x1B[0m\x1B[34m│\x1B[0m \x1B[0m\x1B[34mhandle the error case to ensure runtime success\x1B[0m\n  \x1B[0m\x1B[34m│\x1B[0m\n  \x1B[0m\x1B[34m=\x1B[0m see documentation about error handling at https://errors.vrl.dev/#handling\n  \x1B[0m\x1B[34m=\x1B[0m learn more about error code 100 at https://errors.vrl.dev/100\n  \x1B[0m\x1B[34m=\x1B[0m see language documentation at https://vrl.dev\n  \x1B[0m\x1B[34m=\x1B[0m try your code in the VRL REPL, learn more at https://vrl.dev/examples\n"),
    ];


    for (event, program, expected_error) in cases {
        let input = Input::new(program, event);
        let input_js = JsValue::from_serde(&input).unwrap();
        let result_js = run_vrl(&input_js);
        let error: String = result_js.into_serde().unwrap();
        assert_eq!(error, expected_error);
    }
}
