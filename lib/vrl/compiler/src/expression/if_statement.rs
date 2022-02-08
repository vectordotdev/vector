use std::fmt;

use crate::{
    expression::{Block, Expr, Literal, Predicate, Resolved},
    vm::OpCode,
    Context, Expression, State, TypeDef, Value,
};

#[derive(Debug, Clone, PartialEq)]
pub struct IfStatement {
    pub predicate: Predicate,
    pub consequent: Block,
    pub alternative: Option<Block>,
}

impl IfStatement {
    pub(crate) fn noop() -> Self {
        let literal = Literal::Boolean(false);
        let predicate = Predicate::new_unchecked(vec![Expr::Literal(literal)]);

        let literal = Literal::Null;
        let consequent = Block::new(vec![Expr::Literal(literal)]);

        Self {
            predicate,
            consequent,
            alternative: None,
        }
    }
}

impl Expression for IfStatement {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let predicate = self.predicate.resolve(ctx)?.try_boolean()?;

        match predicate {
            true => self.consequent.resolve(ctx),
            false => self
                .alternative
                .as_ref()
                .map(|block| block.resolve(ctx))
                .unwrap_or(Ok(Value::Null)),
        }
    }

    fn type_def(&self, state: &State) -> TypeDef {
        let type_def = self.consequent.type_def(state);

        match &self.alternative {
            None => type_def,
            Some(alternative) => type_def.merge(alternative.type_def(state)),
        }
    }

    fn compile_to_vm(&self, vm: &mut crate::vm::Vm) -> Result<(), String> {
        // Write the predicate which will leave the result on the stack.
        self.predicate.compile_to_vm(vm)?;

        // If the value is false, we want to jump to the alternative block.
        // We need to store this jump as it will need updating when we know where
        // the alternative block actually starts.
        let else_jump = vm.emit_jump(OpCode::JumpIfFalse);
        vm.write_opcode(OpCode::Pop);

        // Write the consequent block.
        self.consequent.compile_to_vm(vm)?;

        // After the consequent block we want to jump over the alternative.
        let continue_jump = vm.emit_jump(OpCode::Jump);

        // Update the initial if jump to jump to the current position.
        vm.patch_jump(else_jump);
        vm.write_opcode(OpCode::Pop);

        if let Some(alternative) = &self.alternative {
            // Write the alternative block.
            alternative.compile_to_vm(vm)?;
        }

        // Update the continue jump to jump to the current position after the else block.
        vm.patch_jump(continue_jump);

        Ok(())
    }
}

impl fmt::Display for IfStatement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("if ")?;
        self.predicate.fmt(f)?;
        f.write_str(" ")?;
        self.consequent.fmt(f)?;

        if let Some(alt) = &self.alternative {
            f.write_str(" else")?;
            alt.fmt(f)?;
        }

        Ok(())
    }
}
