use std::fmt;

use crate::{
    expression::{ExpressionError, Resolved},
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

    fn dump(&self, vm: &mut crate::vm::Vm) -> Result<(), String> {
        use crate::vm::OpCode;
        vm.write_chunk(OpCode::Abort);
        vm.write_primitive(self.span.start());
        vm.write_primitive(self.span.end());
        Ok(())
    }

    fn as_value(&self) -> Option<crate::Value> {
        None
    }

    fn update_state(&mut self, _state: &mut crate::State) -> Result<(), ExpressionError> {
        Ok(())
    }

    fn format(&self) -> Option<String> {
        None
    }
}

impl fmt::Display for Abort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "abort")
    }
}
