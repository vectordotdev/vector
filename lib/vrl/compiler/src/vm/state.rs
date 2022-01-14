use super::{argument_list::VmArgument, machine::Instruction, OpCode, Vm};
use crate::{ExpressionError, Value};

/// `VmState` contains the mutable state used to run the Vm.
pub struct VmState<'a> {
    vm: &'a Vm,
    pub(super) ip: usize,
    pub(super) stack: Vec<Value>,
    pub(super) parameter_stack: Vec<Option<VmArgument<'a>>>,
    pub(super) error: Option<ExpressionError>,
}

impl<'a> VmState<'a> {
    pub(super) fn new(vm: &'a Vm) -> Self {
        Self {
            vm,
            ip: 0,
            stack: Vec::new(),
            parameter_stack: Vec::new(),
            error: None,
        }
    }

    pub(super) fn next(&mut self) -> Result<OpCode, ExpressionError> {
        let byte = self.vm.instructions[self.ip];
        self.ip += 1;
        match byte {
            Instruction::OpCode(opcode) => Ok(opcode),
            _ => Err(format!("Expecting opcode at {}", self.ip - 1).into()),
        }
    }

    pub(super) fn next_primitive(&mut self) -> Result<usize, ExpressionError> {
        let byte = self.vm.instructions[self.ip];
        self.ip += 1;
        match byte {
            Instruction::Primitive(primitive) => Ok(primitive),
            _ => Err("Expecting primitive".into()),
        }
    }

    /// Pushes the given value onto the stack.
    pub fn push_stack(&mut self, value: Value) {
        self.stack.push(value);
    }

    pub fn pop_stack(&mut self) -> Result<Value, ExpressionError> {
        self.stack.pop().ok_or_else(|| "stack underflow".into())
    }

    pub fn peek_stack(&self) -> Result<&Value, ExpressionError> {
        if self.stack.is_empty() {
            return Err("peeking empty stack".into());
        }

        Ok(&self.stack[self.stack.len() - 1])
    }

    pub fn parameter_stack(&self) -> &Vec<Option<VmArgument<'a>>> {
        &self.parameter_stack
    }

    pub fn parameter_stack_mut(&mut self) -> &mut Vec<Option<VmArgument<'a>>> {
        &mut self.parameter_stack
    }

    pub(super) fn read_constant(&mut self) -> Result<Value, ExpressionError> {
        let idx = self.next_primitive()?;
        Ok(self.vm.values[idx].clone())
    }
}
