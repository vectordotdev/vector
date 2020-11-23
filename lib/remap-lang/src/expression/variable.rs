use crate::{state, Expression, Object, Result, TypeDef, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct Variable {
    ident: String,
}

impl Variable {
    pub fn new(ident: String) -> Self {
        Self { ident }
    }

    pub fn boxed(self) -> Box<Self> {
        Box::new(self)
    }

    pub fn ident(&self) -> &str {
        &self.ident
    }
}

impl Expression for Variable {
    fn execute(&self, state: &mut state::Program, _: &mut dyn Object) -> Result<Value> {
        let value = state.variable(&self.ident).cloned().unwrap_or(Value::Null);

        Ok(value)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        state
            .variable_type(&self.ident)
            .cloned()
            // TODO: we can make it so this can never happen, by making it a
            // compile-time error to reference a variable before it is assigned.
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
                state.variable_types_mut().insert("foo".to_owned(), TypeDef::default());
                Variable::new("foo".to_owned())
            },
            def: TypeDef::default(),
        }

        exact_match {
            expr: |state: &mut state::Compiler| {
                state.variable_types_mut().insert("foo".to_owned(), TypeDef {
                    fallible: true,
                    kind: Kind::Bytes
                });

                Variable::new("foo".to_owned())
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Bytes,
            },
        }

        ident_mismatch {
            expr: |state: &mut state::Compiler| {
                state.variable_types_mut().insert("foo".to_owned(), TypeDef {
                    fallible: true,
                    ..Default::default()
                });

                Variable::new("bar".to_owned())
            },
            def: TypeDef {
                fallible: true,
                ..Default::default()
            },
        }

        empty_state {
            expr: |_| Variable::new("foo".to_owned()),
            def: TypeDef {
                fallible: true,
                ..Default::default()
            },
        }
    ];
}
