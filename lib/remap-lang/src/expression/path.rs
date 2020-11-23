use super::Error as E;
use crate::{state, Expression, Object, Result, TypeDef, Value};

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
    #[error("missing path: {0}")]
    Missing(String),

    #[error("unable to resolve path: {0}")]
    Resolve(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    // TODO: Switch to String once Event API is cleaned up.
    segments: Vec<Vec<String>>,
}

impl<T: AsRef<str>> From<T> for Path {
    fn from(v: T) -> Self {
        Self {
            segments: vec![vec![v.as_ref().to_owned()]],
        }
    }
}

impl Path {
    pub(crate) fn new(segments: Vec<Vec<String>>) -> Self {
        Self { segments }
    }

    pub fn segments(&self) -> &[Vec<String>] {
        &self.segments
    }

    pub fn as_string(&self) -> String {
        self.segments
            .iter()
            .map(|c| {
                c.iter()
                    .map(|p| p.replace(".", "\\."))
                    .collect::<Vec<_>>()
                    .join(".")
            })
            .collect::<Vec<_>>()
            .join(".")
    }
}

impl Expression for Path {
    fn execute(&self, _: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let value = object
            .find(&self.segments)
            .map_err(|e| E::from(Error::Resolve(e)))?
            .unwrap_or(Value::Null);

        Ok(value)
    }

    /// A path resolves to `Any` by default, but the script might assign
    /// specific values to paths during its execution, which increases our exact
    /// understanding of the value kind the path contains.
    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        state
            .path_query_type(self.as_string())
            .cloned()
            .unwrap_or(TypeDef {
                fallible: true,
                ..Default::default()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_type_def, value::Kind};

    test_type_def![
        ident_match {
            expr: |state: &mut state::Compiler| {
                state.path_query_types_mut().insert("foo".to_owned(), TypeDef::default());
                Path::from("foo")
            },
            def: TypeDef::default(),
        }

        exact_match {
            expr: |state: &mut state::Compiler| {
                state.path_query_types_mut().insert("foo".to_owned(), TypeDef {
                    fallible: true,
                    kind: Kind::Bytes
                });

                Path::from("foo")
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Bytes,
            },
        }

        ident_mismatch {
            expr: |state: &mut state::Compiler| {
                state.path_query_types_mut().insert("foo".to_owned(), TypeDef {
                    fallible: true,
                    ..Default::default()
                });

                Path::from("bar")
            },
            def: TypeDef {
                fallible: true,
                ..Default::default()
            },
        }

        empty_state {
            expr: |_| Path::from("foo"),
            def: TypeDef {
                fallible: true,
                ..Default::default()
            },
        }
    ];
}
