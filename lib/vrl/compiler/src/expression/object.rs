use crate::expression::{Expr, Resolved};
use crate::{Context, Expression, State, TypeDef, Value};
use std::fmt;
use vrl_structures::Map;

#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    inner: Map<String, Expr>,
}

impl Object {
    pub fn new(inner: Map<String, Expr>) -> Self {
        Self { inner }
    }
}

impl Expression for Object {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.inner
            .iter()
            .map(|(key, expr)| expr.resolve(ctx).map(|v| (key.to_owned(), v)))
            .collect::<Result<Map<_, _>, _>>()
            .map(Value::Object)
    }

    fn type_def(&self, state: &State) -> TypeDef {
        let type_defs = self
            .inner
            .iter()
            .map(|(k, expr)| (k.to_owned(), expr.type_def(state)))
            .collect::<Map<_, _>>();

        // If any of the stored expressions is fallible, the entire object is
        // fallible.
        let fallible = type_defs.values().any(TypeDef::is_fallible);

        TypeDef::new().object(type_defs).with_fallibility(fallible)
    }
}

impl fmt::Display for Object {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let exprs = self
            .inner
            .iter()
            .map(|(k, v)| format!(r#""{}": {}"#, k, v))
            .collect::<Vec<_>>()
            .join(", ");

        write!(f, "{{ {} }}", exprs)
    }
}

impl From<Map<String, Expr>> for Object {
    fn from(inner: Map<String, Expr>) -> Self {
        Self { inner }
    }
}
