use std::{collections::BTreeMap, fmt, ops::Deref};

use crate::{
    expression::{Expr, Literal, Resolved},
    state::{ExternalEnv, LocalEnv},
    value::VrlValueConvert,
    vm::OpCode,
    Context, Expression, TypeDef, Value,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    inner: Vec<(Expr, Expr)>,
}

impl Object {
    pub fn new(inner: Vec<(Expr, Expr)>) -> Self {
        Self { inner }
    }
}

impl Deref for Object {
    type Target = Vec<(Expr, Expr)>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl Expression for Object {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.inner
            .iter()
            .map(|(key, expr)| {
                expr.resolve(ctx)
                    .and_then(|v| Ok((key.resolve(ctx)?.try_bytes_utf8_lossy()?.to_string(), v)))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()
            .map(Value::Object)
    }

    fn as_value(&self) -> Option<Value> {
        self.inner
            .iter()
            .map(|(key, expr)| {
                expr.as_value().and_then(|v| {
                    Some((key.as_value()?.try_bytes_utf8_lossy().ok()?.to_string(), v))
                })
            })
            .collect::<Option<BTreeMap<_, _>>>()
            .map(Value::Object)
    }

    fn type_def(&self, state: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        let type_defs = self
            .inner
            .iter()
            .map(|(k, expr)| {
                Some((
                    k.as_value()?.try_bytes_utf8_lossy().ok()?.to_string(),
                    expr.type_def(state),
                ))
            })
            .collect::<Option<BTreeMap<_, _>>>();

        match type_defs {
            None => {
                // We can't get the full typedef of the object as the keys aren't fully deterimined,
                // but we can at least still get the fallibility from the value expressions.
                let fallible = self
                    .inner
                    .iter()
                    .any(|(_, expr)| expr.type_def(state).is_fallible());

                TypeDef::object(BTreeMap::default()).with_fallibility(fallible)
            }
            Some(type_defs) => {
                // If any of the stored expressions is fallible, the entire object is
                // fallible.
                let fallible = type_defs.values().any(TypeDef::is_fallible);

                let collection = type_defs
                    .into_iter()
                    .map(|(field, type_def)| (field.into(), type_def.into()))
                    .collect::<BTreeMap<_, _>>();

                TypeDef::object(collection).with_fallibility(fallible)
            }
        }
    }

    fn compile_to_vm(
        &self,
        vm: &mut crate::vm::Vm,
        state: (&mut LocalEnv, &mut ExternalEnv),
    ) -> Result<(), String> {
        let (local, external) = state;

        for (key, value) in &self.inner {
            // Write the key
            key.compile_to_vm(vm, (local, external))?;

            // Write the value
            value.compile_to_vm(vm, (local, external))?;
        }

        vm.write_opcode(OpCode::CreateObject);

        // Write the number of key/value pairs in the object so the machine knows
        // how many pairs to suck into the created object.
        vm.write_primitive(self.inner.len());

        Ok(())
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

impl From<BTreeMap<String, Expr>> for Object {
    fn from(inner: BTreeMap<String, Expr>) -> Self {
        let inner = inner
            .into_iter()
            .map(|(key, value)| (Literal::from(key).into(), value))
            .collect::<Vec<_>>();

        Self { inner }
    }
}
