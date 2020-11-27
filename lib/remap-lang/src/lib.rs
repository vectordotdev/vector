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
    use crate::map;

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
                Ok("valid: baz".into()),
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
            (r#". = .foo"#, Ok(()), Ok(map!["bar": "baz", "qux": Value::Array(vec![1.into(), 2.into(), map!["quux": true].into()])].into())),
            (r#"."#, Ok(()), Ok(map!["foo": map!["bar": "baz", "qux": Value::Array(vec![1.into(), 2.into(), map!["quux": true].into()])]].into())),
            (r#" . "#, Ok(()), Ok(map!["foo": map!["bar": "baz", "qux": Value::Array(vec![1.into(), 2.into(), map!["quux": true].into()])]].into())),
            (r#".foo"#, Ok(()), Ok(map!["bar": "baz", "qux": Value::Array(vec![1.into(), 2.into(), map!["quux": true].into()])].into())),
            (r#".foo.qux[0]"#, Ok(()), Ok(1.into())),
            (r#".foo.bar"#, Ok(()), Ok("baz".into())),
            (r#".(nope | foo)"#, Ok(()), Ok(map!["bar": "baz", "qux": Value::Array(vec![1.into(), 2.into(), map!["quux": true].into()])].into())),
            (r#".(foo | nope)"#, Ok(()), Ok(map!["bar": "baz", "qux": Value::Array(vec![1.into(), 2.into(), map!["quux": true].into()])].into())),
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
                Ok(map![
                    "bar": map![
                        "bar2": map![
                            "baz": vec![
                                Value::Null,
                                Value::Null,
                                "qux".into(),
                            ],
                        ],
                    ],
                    "qux": Value::Array(vec![1.into(), 2.into(), map!["quux": true].into()]),
                ].into()),
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

            // FIXME: make variable assignment behave like paths by implementing
            // `remap::Object` for `store::Variable`.
            ("$foo[0] = true\n$foo", Ok(()), Ok(true.into())),
        ];

        for (script, compile_expected, runtime_expected) in cases {
            let program = Program::new(
                script,
                &[
                    Box::new(test_functions::RegexPrinter),
                    Box::new(test_functions::EnumValidator),
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
            let mut event: Value = map![
                "foo":
                    map![
                        "bar": "baz",
                        "qux": Value::Array(vec![
                            1.into(),
                            2.into(),
                            map![
                                "quux": true,
                            ].into(),
                        ]),
                    ],
            ]
            .into();

            let result = runtime
                .execute(&mut event, &program)
                .map_err(|e| e.to_string());

            assert_eq!(result, runtime_expected.map_err(|e: &str| e.to_string()));
        }
    }

    mod test_functions {
        use super::*;

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
                Ok(Box::new(RegexPrinterFn(arguments.required_regex("value")?)))
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
    }
}
