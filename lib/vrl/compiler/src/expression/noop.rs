use std::fmt;

use crate::{
    expression::Resolved,
    vm::{OpCode, Vm},
    Context, Expression, State, TypeDef, Value,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Noop;

impl Expression for Noop {
    fn resolve(&self, _: &mut Context) -> Resolved {
        Ok(Value::Null)
    }

    fn type_def(&self, _: &State) -> TypeDef {
        TypeDef::new().null().infallible()
    }

    fn compile_to_vm(&self, vm: &mut Vm) -> Result<(), String> {
        // Noop just adds a Null to the stack.
        let constant = vm.add_constant(Value::Null);
        vm.write_opcode(OpCode::Constant);
        vm.write_primitive(constant);
        Ok(())
    }
}

impl fmt::Display for Noop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("null")
    }
}
