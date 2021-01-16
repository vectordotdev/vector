use crate::{
    expression::Path, state, value::Kind, Expression, InnerTypeDef, Object, Result, Segment,
    TypeDef, Value,
};
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
        let mut typedef = state.variable_type(&self.ident).cloned();

        // If there are multiple segments for the variable we need to walk
        // down the inner typedefs to get the final type.
        if let Some(path) = self.path.as_ref() {
            let path: crate::path::Path = path.clone().into();

            for segment in path.segments() {
                typedef = typedef.and_then(|td| match (segment, &td.inner_type_def) {
                    (Segment::Field(field), InnerTypeDef::Map(td)) => {
                        td.get(field.as_str()).cloned()
                    }
                    (Segment::Index(_), InnerTypeDef::Array(db)) => Some(*db.clone()),
                    (Segment::Coalesce(fields), InnerTypeDef::Map(td)) => Some(fields.iter().fold(
                        TypeDef::new_with_kind(Kind::empty()),
                        |accum, field| match td.get(field.as_str()) {
                            Some(val) => accum | val.clone(),
                            None => accum,
                        },
                    )),
                    _ => None,
                })
            }
        }

        typedef.unwrap_or_else(|| TypeDef::new_with_kind(Kind::Null))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_type_def, type_def_map};

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

        coalesce {
            expr: |state: &mut state::Compiler| {
                state.variable_types_mut().insert("foo".to_owned(), TypeDef {
                    kind: Kind::Map,
                    inner_type_def: InnerTypeDef::Map(
                        type_def_map! [
                            "bar": TypeDef::new_with_kind(Kind::Bytes)
                        ]),
                    ..Default::default()
                });
                Variable::new("foo".to_owned(),
                              Some({
                                  let path: crate::path::Path = std::str::FromStr::from_str(".(zonk | bar)").unwrap();
                                  path.into()
                              }))
            },
            def: TypeDef::new_with_kind(Kind::Bytes),
        }
    ];
}
