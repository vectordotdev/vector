mod error;
mod expression;
mod function;
mod operator;
mod parser;
mod program;
mod runtime;
mod state;
mod value;

use expression::{Expr, Expression};
use function::{Argument, ArgumentList, Function, Parameter};
use operator::Operator;
use state::State;

pub use error::Error;
pub use program::Program;
pub use runtime::Runtime;
pub use value::Value;

type Result<T> = std::result::Result<T, Error>;

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::str::FromStr;

    #[derive(Debug, Default)]
    struct Event {
        paths: HashMap<String, Value>,
    }

    impl Object for Event {
        fn insert(
            &mut self,
            path: &[Vec<String>],
            value: Value,
        ) -> std::result::Result<(), String> {
            let path = path
                .iter()
                .map(|c| c.join("."))
                .collect::<Vec<_>>()
                .join(".");

            self.paths.insert(path, value);
            Ok(())
        }

        fn find(&self, path: &[Vec<String>]) -> std::result::Result<Option<Value>, String> {
            Ok(self
                .paths
                .get(
                    &path
                        .iter()
                        .map(|c| c.join("."))
                        .collect::<Vec<_>>()
                        .join("."),
                )
                .cloned())
        }
    }

    #[test]
    fn it_works() {
        let cases: Vec<(&str, Result<Option<Value>>)> = vec![
            (r#".foo = null || "bar""#, Ok(Some("bar".into()))),
            // (r#".foo == .bar"#, Ok(Some(Value::Boolean(false)))),
            (
                r#".foo == .bar"#,
                Err(
                    expression::Error::Path(expression::path::Error::Missing("foo".to_owned()))
                        .into(),
                ),
            ),
            (r#".foo = (null || "bar")"#, Ok(Some("bar".into()))),
            (r#"!false"#, Ok(Some(true.into()))),
            (r#"!!false"#, Ok(Some(false.into()))),
            (r#"!!!true"#, Ok(Some(false.into()))),
            (r#"if true { "yes" } else { "no" }"#, Ok(Some("yes".into()))),
            // (
            //     r#".a.b.(c | d) == .e."f.g"[2].(h | i)"#,
            //     Ok(Some(Value::Boolean(false))),
            // ),
            (".bar = true\n.foo = .bar", Ok(Some(Value::Boolean(true)))),
            (
                r#"split("bar", pattern = /a/)"#,
                Ok(Some(vec!["b", "r"].into())),
            ),
            (
                r#"{
                    .foo = "foo"
                    .foo = .foo + "bar"
                    .foo
                }"#,
                Ok(Some("foobar".into())),
            ),
            (
                r#"
                    .foo = false
                    false || (.foo = true) && true
                    .foo
                "#,
                Ok(Some(true.into())),
            ),
        ];

        for (script, result) in cases {
            let program = Program::from_str(script)
                .map_err(|e| {
                    println!("{}", &e);
                    e
                })
                .unwrap();
            let mut runtime = Runtime::new(State::default());
            let mut event = Event::default();

            assert_eq!(runtime.execute(&mut event, &program), result);
        }
    }
}
