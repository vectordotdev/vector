use crate::{expression::Path, state, value::Kind, Expression, Object, Result, TypeDef, Value};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct Variable {
    ident: String,
    path: Option<Path>,
}

impl fmt::Display for Variable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.ident)?;

        if let Some(path) = &self.path {
            path.fmt(f)?;
        }

        Ok(())
    }
}

impl Variable {
    pub fn new(ident: String, path: Option<Path>) -> Self {
        Self { ident, path }
    }

    pub fn boxed(self) -> Box<Self> {
        Box::new(self)
    }

    pub fn ident(&self) -> &str {
        &self.ident
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_ref()
    }
}

impl Expression for Variable {
    fn execute(&self, state: &mut state::Program, _: &mut dyn Object) -> Result<Value> {
        let mut value = state.variable(&self.ident).cloned().unwrap_or(Value::Null);

        if let Some(path) = &self.path {
            return path.execute(state, &mut value);
        }

        Ok(value)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        state
            .variable_type(&self.ident)
            .cloned()
            // TODO: we can make it so this can never happen, by making it a
            // compile-time error to reference a variable before it is assigned.
            .unwrap_or(TypeDef {
                kind: Kind::Null,
                ..Default::default()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_type_def;

    test_type_def![
        ident_match {
            expr: |state: &mut state::Compiler| {
                state.variable_types_mut().insert("foo".to_owned(), TypeDef::default());
                Variable::new("foo".to_owned(), None)
            },
            def: TypeDef::default(),
        }

        exact_match {
            expr: |state: &mut state::Compiler| {
                state.variable_types_mut().insert("foo".to_owned(), TypeDef {
                    fallible: true,
                    kind: Kind::Bytes,
                    ..Default::default()
                });

                Variable::new("foo".to_owned(), None)
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Bytes,
                ..Default::default()
            },
        }

        ident_mismatch {
            expr: |state: &mut state::Compiler| {
                state.variable_types_mut().insert("foo".to_owned(), TypeDef {
                    fallible: true,
                    ..Default::default()
                });

                Variable::new("bar".to_owned(), None)
            },
            def: TypeDef {
                kind: Kind::Null,
                ..Default::default()
            },
        }

        empty_state {
            expr: |_| Variable::new("foo".to_owned(), None),
            def: TypeDef {
                kind: Kind::Null,
                ..Default::default()
            },
        }
    ];
}
