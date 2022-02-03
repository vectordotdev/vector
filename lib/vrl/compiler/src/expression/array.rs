use std::{fmt, ops::Deref};

use crate::{
    expression::{Expr, Resolved},
    vm::OpCode,
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

    fn compile_to_vm(&self, vm: &mut crate::vm::Vm) -> Result<(), String> {
        // Evaluate each of the elements of the array, the result of each
        // will be added to the stack.
        for value in self.inner.iter().rev() {
            value.compile_to_vm(vm)?;
        }

        vm.write_opcode(OpCode::CreateArray);

        // Add the length of the array as a primitive so the VM knows how
        // many elements to move into the array.
        vm.write_primitive(self.inner.len());

        Ok(())
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
