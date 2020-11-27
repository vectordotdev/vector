mod error;
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
pub use operator::Operator;
pub use path::{Field, Path, Segment};
pub use program::{Program, TypeConstraint};
pub use runtime::Runtime;
pub use type_def::TypeDef;
pub use value::Value;

pub use paste::paste;

pub type Result<T> = std::result::Result<T, Error>;

/// Any object you want to map through the remap language has to implement this
/// trait.
pub trait Object: std::fmt::Debug {
    /// Insert a given [`Value`] in the provided [`Object`].
    ///
    /// The `path` parameter determines _where_ in the given object the value
    /// should be inserted.
    ///
    /// A path contains dot-delimited segments, and can contain a combination
    /// of:
    ///
    /// * regular path segments:
    ///
    ///   ```txt
    ///   .foo.bar.baz
    ///   ```
    ///
    /// * quoted path segments:
    ///
    ///   ```txt
    ///   .foo."bar.baz"
    ///   ```
    ///
    /// * coalesced path segments:
    ///
    ///   ```txt
    ///   .foo.(bar.baz | foobar | "bar.baz").qux
    ///   ```
    ///
    /// * path indices:
    ///
    ///   ```txt
    ///   .foo[2]
    ///   ```
    ///
    /// When inserting into a coalesced path, the implementor is encouraged to
    /// insert into the right-most segment if none exists, but can return an
    /// error if needed.
    fn insert(&mut self, path: &[Vec<String>], value: Value) -> std::result::Result<(), String>;

    /// Find a value for a given path.
    ///
    /// See [`Object::insert`] for more details.
    fn find(&self, path: &[Vec<String>]) -> std::result::Result<Option<Value>, String>;

    /// Get the list of paths in the object.
    ///
    /// Paths are represented similar to what's documented in [`Object::insert`].
    fn paths(&self) -> Vec<String>;

    /// Remove the given path from the object.
    ///
    /// If `compact` is true, after deletion, if an empty object or array is
    /// left behind, it should be removed as well.
    fn remove(&mut self, path: &str, compact: bool);
}

impl Object for std::collections::HashMap<String, Value> {
    fn insert(&mut self, path: &[Vec<String>], value: Value) -> std::result::Result<(), String> {
        self.insert(vec_path_to_string(path), value);

        Ok(())
    }

    fn find(&self, path: &[Vec<String>]) -> std::result::Result<Option<Value>, String> {
        Ok(self.get(&vec_path_to_string(path)).cloned())
    }

    fn paths(&self) -> Vec<String> {
        self.keys().cloned().collect::<Vec<_>>()
    }

    fn remove(&mut self, path: &str, _: bool) {
        self.remove(path);
    }
}

impl Object for std::collections::BTreeMap<String, Value> {
    fn insert(&mut self, path: &[Vec<String>], value: Value) -> std::result::Result<(), String> {
        self.insert(vec_path_to_string(path), value);

        Ok(())
    }

    fn find(&self, path: &[Vec<String>]) -> std::result::Result<Option<Value>, String> {
        Ok(self.get(&vec_path_to_string(path)).cloned())
    }

    fn paths(&self) -> Vec<String> {
        self.keys().cloned().collect::<Vec<_>>()
    }

    fn remove(&mut self, path: &str, _: bool) {
        self.remove(path);
    }
}

fn vec_path_to_string(path: &[Vec<String>]) -> String {
    path.iter()
        .map(|v| v.join("."))
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::function::ArgumentList;
    use std::collections::HashMap;

    #[test]
    fn it_works() {
        let cases = vec![
            (r#".foo = null || "bar""#, Ok(()), Ok("bar".into())),
            (r#"$foo = null || "bar""#, Ok(()), Ok("bar".into())),
            // (r#".foo == .bar"#, Ok(Value::Boolean(false))),
            (
                r#".foo == .bar"#,
                Ok(()),
                Ok(true.into()),
            ),
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
            (r#". = "bar""#, Ok(()), Ok("bar".into())),
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
            let mut event = HashMap::default();

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
