use std::fmt;

use value::Value;

use crate::{
    expression::Resolved,
    state::{ExternalEnv, LocalEnv},
    vm::{OpCode, Vm},
    Context, Expression, TypeDef,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Noop;

impl Expression for Noop {
    fn resolve(&self, _: &mut Context) -> Resolved {
        Ok(Value::Null)
    }

    fn type_def(&self, _: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        TypeDef::null().infallible()
    }

    fn compile_to_vm(
        &self,
        vm: &mut Vm,
        _state: (&mut LocalEnv, &mut ExternalEnv),
    ) -> Result<(), String> {
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
