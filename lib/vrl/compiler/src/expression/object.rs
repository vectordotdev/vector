use std::{collections::BTreeMap, fmt, ops::Deref};

use value::Value;

use crate::{
    expression::{Expr, Resolved},
    state::{TypeInfo, TypeState},
    Context, Expression, TypeDef,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    inner: BTreeMap<String, Expr>,
}

impl Object {
    #[must_use]
    pub fn new(inner: BTreeMap<String, Expr>) -> Self {
        Self { inner }
    }
}

impl Deref for Object {
    type Target = BTreeMap<String, Expr>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl Expression for Object {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.inner
            .iter()
            .map(|(key, expr)| expr.resolve(ctx).map(|v| (key.clone(), v)))
            .collect::<Result<BTreeMap<_, _>, _>>()
            .map(Value::Object)
    }

    fn as_value(&self) -> Option<Value> {
        self.inner
            .iter()
            .map(|(key, expr)| expr.as_value().map(|v| (key.clone(), v)))
            .collect::<Option<BTreeMap<_, _>>>()
            .map(Value::Object)
    }

    fn type_info(&self, state: &TypeState) -> TypeInfo {
        let mut state = state.clone();
        let mut fallible = false;

        let mut type_defs = BTreeMap::new();
        for (k, expr) in &self.inner {
            let type_def = expr.apply_type_info(&mut state);

            // If any expression is fallible, the entire object is fallible.
            fallible |= type_def.is_fallible();

            // If any expression aborts, the entire object aborts
            if type_def.is_never() {
                return TypeInfo::new(state, TypeDef::never().with_fallibility(fallible));
            }
            type_defs.insert(k.clone(), type_def);
        }

        let collection = type_defs
            .into_iter()
            .map(|(field, type_def)| (field.into(), type_def.into()))
            .collect::<BTreeMap<_, _>>();

        let result = TypeDef::object(collection).with_fallibility(fallible);
        TypeInfo::new(state, result)
    }
}

impl fmt::Display for Object {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let exprs = self
            .inner
            .iter()
            .map(|(k, v)| format!(r#""{k}": {v}"#))
            .collect::<Vec<_>>()
            .join(", ");

        write!(f, "{{ {exprs} }}")
    }
}

impl From<BTreeMap<String, Expr>> for Object {
    fn from(inner: BTreeMap<String, Expr>) -> Self {
        Self { inner }
    }
}
