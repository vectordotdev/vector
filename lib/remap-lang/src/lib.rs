mod error;
mod object;
mod operator;
mod parser;
mod path;
mod program;
mod runtime;
mod test_util;
mod type_def;

pub mod expression;
pub mod function;
pub mod prelude;
pub mod state;
pub mod value;

pub use error::{Error, RemapError};
pub use expression::{Expr, Expression};
pub use function::{Function, Parameter};
pub use object::Object;
pub use operator::Operator;
pub use path::{Field, Path, Segment};
pub use program::{Program, TypeConstraint};
pub use runtime::Runtime;
pub use type_def::TypeDef;
pub use value::Value;

pub use paste::paste;

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::function::ArgumentList;
    use crate::value;

    #[test]
    fn it_works() {
        #[rustfmt::skip]
        let cases = vec![
            (r#".foo = null || "bar""#, Ok(()), Ok("bar".into())),
            (r#"$foo = null || "bar""#, Ok(()), Ok("bar".into())),
            (r#".qux == .quux"#, Ok(()), Ok(true.into())),
            (
                r#"if "foo" { "bar" }"#,
                Ok(()),
                Err(r#"remap error: value error: expected "boolean", got "string""#),
            ),
            (r#".foo = (null || "bar")"#, Ok(()), Ok("bar".into())),
            (r#"!false"#, Ok(()), Ok(true.into())),
            (r#"!!false"#, Ok(()), Ok(false.into())),
            (r#"!!!true"#, Ok(()), Ok(false.into())),
            (r#"if true { "yes" } else { "no" }"#, Ok(()), Ok("yes".into())),
            // (
            //     r#".a.b.(c | d) == .e."f.g"[2].(h | i)"#,
            //     Ok(Value::Boolean(false)),
            // ),
            ("$bar = true\n.foo = $bar", Ok(()), Ok(Value::Boolean(true))),
            (
                r#"{
                    $foo = "foo"
                    .foo = $foo + "bar"
                    .foo
                }"#,
                Ok(()),
                Ok("foobar".into()),
            ),
            (
                r#"
                    .foo = false
                    false || (.foo = true) && true
                    .foo
                "#,
                Ok(()),
                Ok(true.into()),
            ),
            (r#"if false { 1 }"#, Ok(()), Ok(Value::Null)),
            (r#"if true { 1 }"#, Ok(()), Ok(1.into())),
            (r#"if false { 1 } else { 2 }"#, Ok(()), Ok(2.into())),
            (r#"if false { 1 } else if false { 2 }"#, Ok(()), Ok(Value::Null)),
            (r#"if false { 1 } else if true { 2 }"#, Ok(()), Ok(2.into())),
            (
                r#"if false { 1 } else if false { 2 } else { 3 }"#,
                Ok(()), Ok(3.into()),
            ),
            (
                r#"if false { 1 } else if true { 2 } else { 3 }"#,
                Ok(()), Ok(2.into()),
            ),
            (
                r#"if false { 1 } else if false { 2 } else if false { 3 }"#,
                Ok(()), Ok(Value::Null),
            ),
            (
                r#"if false { 1 } else if false { 2 } else if true { 3 }"#,
                Ok(()), Ok(3.into()),
            ),
            (
                r#"if false { 1 } else if true { 2 } else if false { 3 } else { 4 }"#,
                Ok(()), Ok(2.into()),
            ),
            (
                r#"if false { 1 } else if false { 2 } else if false { 3 } else { 4 }"#,
                Ok(()), Ok(4.into()),
            ),
            (
                r#"if ($foo = true; $foo) { $foo } else { false }"#,
                Ok(()), Ok(true.into())
            ),
            (
                r#"if ($foo = "sproink"
                       $foo == "sproink") {
                      $foo
                   } else {
                     false
                   }"#,
                Ok(()), Ok("sproink".into())
            ),
            (
                r#"regex_printer(/escaped\/forward slash/)"#,
                Ok(()), Ok("regex: escaped/forward slash".into()),
            ),
            (
                r#"enum_validator("foo")"#,
                Ok(()),
                Ok("valid: foo".into()),
            ),
            (
                r#"enum_validator("bar")"#,
                Ok(()),
                Ok("valid: bar".into()),
            ),
            (
                r#"enum_validator("baz")"#,
                Err("remap error: function error: unknown enum variant: baz, must be one of: foo, bar"),
                Ok(().into()),
            ),
            (r#"false || true"#, Ok(()), Ok(true.into())),
            (r#"false || false"#, Ok(()), Ok(false.into())),
            (r#"true || false"#, Ok(()), Ok(true.into())),
            (r#"true || true"#, Ok(()), Ok(true.into())),
            (r#"false || "foo""#, Ok(()), Ok("foo".into())),
            (r#""foo" || false"#, Ok(()), Ok("foo".into())),
            (r#"null || false"#, Ok(()), Ok(false.into())),
            (r#"false || null"#, Ok(()), Ok(().into())),
            (r#"null || "foo""#, Ok(()), Ok("foo".into())),
            (r#". = .foo"#, Ok(()), Ok(value!({"bar": "baz", "qux": [1, 2, {"quux": true}]}))),
            (r#"."#, Ok(()), Ok(value!({"foo": {"bar": "baz", "qux": [1, 2, {"quux": true}]}}))),
            (r#" . "#, Ok(()), Ok(value!({"foo": {"bar": "baz", "qux": [1, 2, {"quux": true}]}}))),
            (r#".foo"#, Ok(()), Ok(value!({"bar": "baz", "qux": [1, 2, {"quux": true}]}))),
            (r#".foo.qux[0]"#, Ok(()), Ok(1.into())),
            (r#".foo.bar"#, Ok(()), Ok("baz".into())),
            (r#".(nope | foo)"#, Ok(()), Ok(value!({"bar": "baz", "qux": [1, 2, {"quux": true}]}))),
            (r#".(foo | nope)"#, Ok(()), Ok(value!({"bar": "baz", "qux": [1, 2, {"quux": true}]}))),
            (r#".(nope | foo).bar"#, Ok(()), Ok("baz".into())),
            (r#".foo.(nope | bar)"#, Ok(()), Ok("baz".into())),
            (r#".foo.(nope | no)"#, Ok(()), Ok(().into())),
            (r#".foo.(nope | qux)[1]"#, Ok(()), Ok(2.into())),
            (
                r#"
                    .foo.bar.(bar1 | bar2).baz[2] = "qux"
                    .foo
                "#,
                Ok(()),
                Ok(value!({"bar": {"bar2": {"baz": [null, null, "qux"]}}, "qux": [1, 2, {"quux": true}]})),
            ),
            (
                r#"
                    .foo.bar = "baz"
                    $foo = .foo
                    .foo.bar
                "#,
                Ok(()),
                Ok("baz".into()),
            ),
            ("$foo = .foo\n$foo.bar", Ok(()), Ok("baz".into())),
            ("$foo = .foo.qux\n$foo[1]", Ok(()), Ok(2.into())),
            ("$foo = .foo.qux\n$foo[2].quux", Ok(()), Ok(true.into())),
            (
                "$foo[0] = true",
                Err(r#"remap error: parser error: path in variable assignment unsupported, use "$foo" without ".[0]""#),
                Ok(().into()),
            ),
            (r#"["foo", "bar", "baz"]"#, Ok(()), Ok(value!(["foo", "bar", "baz"]))),
            (
                r#"
                    .foo = [
                        "foo",
                        5,
                        ["bar"],
                    ]
                    .foo
                "#,
                Ok(()),
                Ok(value!(["foo", 5, ["bar"]])),
            ),
            (
                r#"array_printer(["foo", /bar/, 5, ["baz", 4.2], true, /qu+x/, {"1": 1, "true": true}])"#,
                Ok(()),
                Ok(value!([
                    r#"Bytes(b"foo")"#,
                    r#"Regex(bar)"#,
                    r#"Integer(5)"#,
                    r#"[Bytes(b"baz"), Float(4.2)]"#,
                    r#"Boolean(true)"#,
                    r#"Regex(qu+x)"#,
                    r#"{"1": Integer(1), "true": Boolean(true)}"#,
                ])),
            ),
            (
                r#"
                    .foo = ["foo", "bar"]
                    array_printer(.foo)
                "#,
                Err("remap error: unexpected expression: expected Array, got Path"),
                Ok(().into()),
            ),
            (
                r#"enum_list_validator(["foo"])"#,
                Ok(()),
                Ok(r#"valid: ["foo"]"#.into()),
            ),
            (
                r#"enum_list_validator(["bar", "foo"])"#,
                Ok(()),
                Ok(r#"valid: ["bar", "foo"]"#.into()),
            ),
            (
                r#"enum_list_validator(["qux"])"#,
                Err("remap error: function error: unknown enum variant: qux, must be one of: foo, bar, baz"),
                Ok(().into()),
            ),
            (
                r#"enum_list_validator("qux")"#,
                Err("remap error: unexpected expression: expected Array, got Literal"),
                Ok(().into()),
            ),
            (
                r#"
                    .foo        \
                        =       \
                        null || \
                        "bar"
                "#,
                Ok(()),
                Ok("bar".into()),
            ),
            (
                r#"$foo = 1;$nork = $foo + 3;$nork"#,
                Ok(()),
                Ok(4.into()),
            ),
            (r#"{ "foo" }"#, Ok(()), Ok("foo".into())),
            (r#"{ "foo": "bar" }"#, Ok(()), Ok(value!({"foo": "bar"}))),
            (r#"{ "foo": true, "bar": true, "baz": false }"#, Ok(()), Ok(value!({"foo": true, "bar": true, "baz": false}))),
            (
                r#"
                    .result = {
                        .foo = true
                        $bar = 5
                        { "foo": .foo, "bar": $bar, "baz": "qux" }
                    }

                    { "result": .result }
                "#,
                Ok(()),
                Ok(value!({"result": {"foo": true, "bar": 5, "baz": "qux"}})),
            ),
            ("{}", Ok(()), Ok(value!({}))),
            (
                r#"map_printer({"a": "foo", "b": /bar/, "c": 5, "d": ["baz", 4.2], "e": true, "f": /qu+x/, "g": {"1": 1, "true": true}})"#,
                Ok(()),
                Ok(value!({
                    "a": r#"Bytes(b"foo")"#,
                    "b": r#"Regex(bar)"#,
                    "c": r#"Integer(5)"#,
                    "d": r#"[Bytes(b"baz"), Float(4.2)]"#,
                    "e": r#"Boolean(true)"#,
                    "f": r#"Regex(qu+x)"#,
                    "g": r#"{"1": Integer(1), "true": Boolean(true)}"#,
                })),
            ),
        ];

        for (script, compile_expected, runtime_expected) in cases {
            let program = Program::new(
                script,
                &[
                    Box::new(test_functions::RegexPrinter),
                    Box::new(test_functions::EnumValidator),
                    Box::new(test_functions::EnumListValidator),
                    Box::new(test_functions::ArrayPrinter),
                    Box::new(test_functions::MapPrinter),
                ],
                None,
            );

            assert_eq!(
                program.as_ref().map(|_| ()).map_err(|e| e.to_string()),
                compile_expected.map_err(|e: &str| e.to_string())
            );

            if program.is_err() && compile_expected.is_err() {
                continue;
            }

            let program = program.unwrap();
            let mut runtime = Runtime::new(state::Program::default());
            let mut event = value!({"foo": {"bar": "baz", "qux": [1, 2, {"quux": true}]}});

            let result = runtime
                .execute(&mut event, &program)
                .map_err(|e| e.to_string());

            assert_eq!(result, runtime_expected.map_err(|e: &str| e.to_string()));
        }
    }

    mod test_functions {
        use super::*;
        use crate::expression::{Array, Map};
        use std::collections::BTreeMap;
        use std::convert::TryFrom;

        #[derive(Debug, Clone)]
        pub(super) struct EnumValidator;
        impl Function for EnumValidator {
            fn identifier(&self) -> &'static str {
                "enum_validator"
            }

            fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
                Ok(Box::new(EnumValidatorFn(
                    arguments.required_enum("value", &["foo", "bar"])?,
                )))
            }

            fn parameters(&self) -> &'static [Parameter] {
                &[Parameter {
                    keyword: "value",
                    accepts: |_| true,
                    required: true,
                }]
            }
        }

        #[derive(Debug, Clone)]
        struct EnumValidatorFn(String);
        impl Expression for EnumValidatorFn {
            fn execute(&self, _: &mut state::Program, _: &mut dyn Object) -> Result<Value> {
                Ok(format!("valid: {}", self.0).into())
            }

            fn type_def(&self, _: &state::Compiler) -> TypeDef {
                TypeDef::default()
            }
        }

        #[derive(Debug, Clone)]
        pub(super) struct RegexPrinter;
        impl Function for RegexPrinter {
            fn identifier(&self) -> &'static str {
                "regex_printer"
            }

            fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
                Ok(Box::new(RegexPrinterFn(
                    arguments
                        .required_literal("value")?
                        .into_value()
                        .try_regex()?,
                )))
            }

            fn parameters(&self) -> &'static [Parameter] {
                &[Parameter {
                    keyword: "value",
                    accepts: |_| true,
                    required: true,
                }]
            }
        }

        #[derive(Debug, Clone)]
        struct RegexPrinterFn(regex::Regex);
        impl Expression for RegexPrinterFn {
            fn execute(&self, _: &mut state::Program, _: &mut dyn Object) -> Result<Value> {
                Ok(format!("regex: {:?}", self.0).into())
            }

            fn type_def(&self, _: &state::Compiler) -> TypeDef {
                TypeDef::default()
            }
        }

        #[derive(Debug, Clone)]
        pub(super) struct ArrayPrinter;
        impl Function for ArrayPrinter {
            fn identifier(&self) -> &'static str {
                "array_printer"
            }

            fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
                Ok(Box::new(ArrayPrinterFn(arguments.required_array("value")?)))
            }

            fn parameters(&self) -> &'static [Parameter] {
                &[Parameter {
                    keyword: "value",
                    accepts: |_| true,
                    required: true,
                }]
            }
        }

        #[derive(Debug, Clone)]
        struct ArrayPrinterFn(Array);
        impl Expression for ArrayPrinterFn {
            fn execute(&self, _: &mut state::Program, _: &mut dyn Object) -> Result<Value> {
                Ok(self
                    .0
                    .clone()
                    .into_iter()
                    .map(|v| format!("{:?}", v))
                    .collect::<Vec<_>>()
                    .into())
            }

            fn type_def(&self, _: &state::Compiler) -> TypeDef {
                TypeDef::default()
            }
        }

        #[derive(Debug, Clone)]
        pub(super) struct MapPrinter;
        impl Function for MapPrinter {
            fn identifier(&self) -> &'static str {
                "map_printer"
            }

            fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
                Ok(Box::new(MapPrinterFn(arguments.required("value")?)))
            }

            fn parameters(&self) -> &'static [Parameter] {
                &[Parameter {
                    keyword: "value",
                    accepts: |_| true,
                    required: true,
                }]
            }
        }

        #[derive(Debug, Clone)]
        struct MapPrinterFn(Expr);
        impl Expression for MapPrinterFn {
            fn execute(&self, _: &mut state::Program, _: &mut dyn Object) -> Result<Value> {
                Ok(Map::try_from(self.0.clone())
                    .unwrap()
                    .into_iter()
                    .map(|(k, v)| (k, format!("{:?}", v).into()))
                    .collect::<BTreeMap<_, _>>()
                    .into())
            }

            fn type_def(&self, _: &state::Compiler) -> TypeDef {
                TypeDef::default()
            }
        }

        #[derive(Debug, Clone)]
        pub(super) struct EnumListValidator;
        impl Function for EnumListValidator {
            fn identifier(&self) -> &'static str {
                "enum_list_validator"
            }

            fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
                Ok(Box::new(EnumListValidatorFn(
                    arguments.required_enum_list("value", &["foo", "bar", "baz"])?,
                )))
            }

            fn parameters(&self) -> &'static [Parameter] {
                &[Parameter {
                    keyword: "value",
                    accepts: |_| true,
                    required: true,
                }]
            }
        }

        #[derive(Debug, Clone)]
        struct EnumListValidatorFn(Vec<String>);
        impl Expression for EnumListValidatorFn {
            fn execute(&self, _: &mut state::Program, _: &mut dyn Object) -> Result<Value> {
                Ok(format!("valid: {:?}", self.0).into())
            }

            fn type_def(&self, _: &state::Compiler) -> TypeDef {
                TypeDef::default()
            }
        }
    }
}
