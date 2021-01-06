use super::Error as E;
use crate::{expression::Variable, state, value, Expr, Expression, Object, Result, TypeDef, Value};

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
    #[error("collection error")]
    Collection(#[from] value::Error),

    #[error("iterating over a map requires two variables, one for the key and one for the value")]
    MissingMapVariable,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForLoop {
    collection: Box<Expr>,

    // map key or array value
    var1: Variable,

    // map value or array index
    var2: Option<Variable>,

    // block
    expression: Box<Expr>,
}

impl ForLoop {
    pub fn new(
        collection: Expr,
        var1: Variable,
        var2: Option<Variable>,
        expression: Expr,
        state: &state::Compiler,
    ) -> Result<Self> {
        let type_def = collection.type_def(state);

        // We expect this to resolve to exactly an array or a map, so that we
        // know what to assign to the first (and optional second) variable.
        if !type_def.kind.is_array() && !type_def.kind.is_map() {
            return Err(E::from(Error::Collection(value::Error::Expected(
                value::Kind::Array | value::Kind::Map,
                type_def.kind,
            )))
            .into());
        }

        // When dealing with a map, the second variable identifier is required.
        if type_def.kind.is_map() && var2.is_none() {
            return Err(E::from(Error::MissingMapVariable).into());
        }

        let collection = Box::new(collection);
        let expression = Box::new(expression);

        Ok(Self {
            collection,
            var1,
            var2,
            expression,
        })
    }
}

impl Expression for ForLoop {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let collection = self.collection.execute(state, object)?;

        match collection {
            Value::Array(array) => {
                array
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| {
                        // If we have a second variable, the first becomes the
                        // index, otherwise it's the value.
                        let var_value = match &self.var2 {
                            Some(var2) => var2,
                            None => &self.var1,
                        };

                        state
                            .variables_mut()
                            .insert(var_value.ident().to_owned(), value);

                        if self.var2.is_some() {
                            state
                                .variables_mut()
                                .insert(self.var1.ident().to_owned(), (index as i64).into());
                        }

                        self.expression.execute(state, object)
                    })
                    .collect::<Result<Vec<_>>>()?;

                // purge variables after loop ends
                state.variables_mut().remove(&self.var1.ident().to_owned());
                if let Some(var2) = &self.var2 {
                    state.variables_mut().remove(&var2.ident().to_owned());
                }
            }

            Value::Map(map) => {
                map.into_iter()
                    .map(|(key, value)| {
                        state
                            .variables_mut()
                            .insert(self.var1.ident().to_owned(), key.into());

                        if let Some(var2) = &self.var2 {
                            state.variables_mut().insert(var2.ident().to_owned(), value);
                        }

                        self.expression.execute(state, object)
                    })
                    .collect::<Result<Vec<_>>>()?;
            }

            _ => unreachable!("value is map or array"),
        }

        Ok(().into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.expression
            .type_def(state)
            .with_constraint(value::Kind::Null)
    }
}
