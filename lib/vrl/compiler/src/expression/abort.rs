use std::fmt;

use crate::{
    expression::{ExpressionError, Resolved},
    vm::OpCode,
    Context, Expression, Span, State, TypeDef,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Abort {
    span: Span,
}

impl Abort {
    pub fn new(span: Span) -> Abort {
        Abort { span }
    }
}

impl Expression for Abort {
    fn resolve(&self, _: &mut Context) -> Resolved {
        Err(ExpressionError::Abort { span: self.span })
    }

    fn type_def(&self, _: &State) -> TypeDef {
        TypeDef::new().infallible().null()
    }

    fn compile_to_vm(&self, vm: &mut crate::vm::Vm) -> Result<(), String> {
        vm.write_opcode(OpCode::Abort);

        // The Abort OpCode needs the span of the expression to return in the abort error.
        vm.write_primitive(self.span.start());
        vm.write_primitive(self.span.end());
        Ok(())
    }
}

impl fmt::Display for Abort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "abort")
    }
}
