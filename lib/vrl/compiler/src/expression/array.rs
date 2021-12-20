use std::{fmt, ops::Deref};

use crate::{
    expression::{Expr, Resolved},
    Context, Expression, State, TypeDef, Value,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Array {
    inner: Vec<Expr>,
}

impl Array {
    pub(crate) fn new(inner: Vec<Expr>) -> Self {
        Self { inner }
    }
}

impl Deref for Array {
    type Target = Vec<Expr>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl Expression for Array {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.inner
            .iter()
            .map(|expr| expr.resolve(ctx))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array)
    }

    fn as_value(&self) -> Option<Value> {
        self.inner
            .iter()
            .map(|expr| expr.as_value())
            .collect::<Option<Vec<_>>>()
            .map(Value::Array)
    }

    fn type_def(&self, state: &State) -> TypeDef {
        let type_defs = self
            .inner
            .iter()
            .map(|expr| expr.type_def(state))
            .collect::<Vec<_>>();

        // If any of the stored expressions is fallible, the entire array is
        // fallible.
        let fallible = type_defs.iter().any(TypeDef::is_fallible);

        TypeDef::new().array(type_defs).with_fallibility(fallible)
    }
}

impl fmt::Display for Array {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let exprs = self
            .inner
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join(", ");

        write!(f, "[{}]", exprs)
    }
}

impl From<Vec<Expr>> for Array {
    fn from(inner: Vec<Expr>) -> Self {
        Self { inner }
    }
}

#[cfg(test)]
mod tests {
    use crate::{expr, map, test_type_def, value::Kind, TypeDef};

    test_type_def![
        empty_array {
            expr: |_| expr!([]),
            want: TypeDef::new().array::<TypeDef>(vec![]),
        }

        scalar_array {
            expr: |_| expr!([1, "foo", true]),
            want: TypeDef::new().array_mapped::<i32, TypeDef>(map! {
                0: Kind::Integer,
                1: Kind::Bytes,
                2: Kind::Boolean,
            }),
        }

        mixed_array {
            expr: |_| expr!([1, [true, "foo"], { "bar": null }]),
            want: TypeDef::new().array_mapped::<i32, TypeDef>(map! {
                0: Kind::Integer,
                1: TypeDef::new().array_mapped::<i32, TypeDef>(map! {
                    0: Kind::Boolean,
                    1: Kind::Bytes,
                }),
                2: TypeDef::new().object::<&str, TypeDef>(map! {
                    "bar": Kind::Null,
                }),
            }),
        }
    ];
}
