use crate::{path, state, Expression, Object, Result, TypeDef, Value};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    path: path::Path,
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.path.fmt(f)
    }
}

impl AsRef<path::Path> for Path {
    fn as_ref(&self) -> &path::Path {
        &self.path
    }
}

impl<T: AsRef<str>> From<T> for Path {
    fn from(v: T) -> Self {
        let field = path::Field::Quoted(v.as_ref().to_owned());
        let segments = vec![path::Segment::Field(field)];
        let path = path::Path::new_unchecked(segments);

        Self { path }
    }
}

impl From<path::Path> for Path {
    fn from(path: path::Path) -> Self {
        Self { path }
    }
}

impl Into<path::Path> for Path {
    fn into(self) -> path::Path {
        self.path
    }
}

impl Path {
    pub(crate) fn new(path: path::Path) -> Self {
        Self { path }
    }

    pub fn boxed(self) -> Box<dyn Expression> {
        Box::new(self)
    }
}

impl Expression for Path {
    fn execute(&self, _: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let value = object.get(&self.path).ok().flatten().unwrap_or(Value::Null);

        Ok(value)
    }

    /// A path resolves to `Any` by default, but the script might assign
    /// specific values to paths during its execution, which increases our exact
    /// understanding of the value kind the path contains.
    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        state
            .path_query_type(self)
            .cloned()
            .unwrap_or_default()
            .into_fallible(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_type_def, value::Kind};

    test_type_def![
        ident_match {
            expr: |state: &mut state::Compiler| {
                state.path_query_types_mut().insert(Path::from("foo").into(), TypeDef::default());
                Path::from("foo")
            },
            def: TypeDef::default(),
        }

        exact_match {
            expr: |state: &mut state::Compiler| {
                state.path_query_types_mut().insert(Path::from("foo").into(), TypeDef {
                    kind: Kind::Bytes,
                    ..Default::default()
                });

                Path::from("foo")
            },
            def: TypeDef {
                kind: Kind::Bytes,
                ..Default::default()
            },
        }

        ident_mismatch {
            expr: |state: &mut state::Compiler| {
                state.path_query_types_mut().insert(Path::from("foo").into(), TypeDef::default());

                Path::from("bar")
            },
            def: TypeDef::default(),
        }

        empty_state {
            expr: |_| Path::from("foo"),
            def: TypeDef::default(),
        }
    ];
}
