use std::{collections::BTreeMap, fmt, ops::Deref};

use value::Value;

use crate::{
    expression::{Expr, Resolved},
    state::{ExternalEnv, LocalEnv},
    Context, Expression, TypeDef,
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

    fn type_def(&self, state: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        let type_defs = self
            .inner
            .iter()
            .map(|expr| expr.type_def(state))
            .collect::<Vec<_>>();

        // If any of the stored expressions is fallible, the entire array is
        // fallible.
        let fallible = type_defs.iter().any(TypeDef::is_fallible);

        let collection = type_defs
            .into_iter()
            .enumerate()
            .map(|(index, type_def)| (index.into(), type_def.into()))
            .collect::<BTreeMap<_, _>>();

        TypeDef::array(collection).with_fallibility(fallible)
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
    use value::kind::Collection;

    use super::*;
    use crate::{expr, test_type_def, value::Kind, TypeDef};

    test_type_def![
        empty_array {
            expr: |_| expr!([]),
            want: TypeDef::array(Collection::empty()),
        }

        scalar_array {
            expr: |_| expr!([1, "foo", true]),
            want: TypeDef::array(BTreeMap::from([
                (0.into(), Kind::integer()),
                (1.into(), Kind::bytes()),
                (2.into(), Kind::boolean()),
            ])),
        }

        mixed_array {
            expr: |_| expr!([1, [true, "foo"], { "bar": null }]),
            want: TypeDef::array(BTreeMap::from([
                (0.into(), Kind::integer()),
                (1.into(), Kind::array(BTreeMap::from([
                    (0.into(), Kind::boolean()),
                    (1.into(), Kind::bytes()),
                ]))),
                (2.into(), Kind::object(BTreeMap::from([
                    ("bar".into(), Kind::null())
                ]))),
            ])),
        }
    ];
}
