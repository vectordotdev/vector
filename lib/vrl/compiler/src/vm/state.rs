use super::{argument_list::VmArgument, OpCode, Vm};
use crate::{expression::Literal, vm::vm::Instruction, Value};

/// `VmState` contains the mutable state used to run the Vm.
pub struct VmState<'a> {
    vm: &'a Vm,
    pub(super) ip: usize,
    pub(super) stack: Vec<Value>,
    pub(super) parameter_stack: Vec<Option<VmArgument<'a>>>,
}

impl<'a> VmState<'a> {
    pub(super) fn new(vm: &'a Vm) -> Self {
        Self {
            vm,
            ip: 0,
            stack: Vec::new(),
            parameter_stack: Vec::new(),
        }
    }

    pub(super) fn next(&mut self) -> OpCode {
        let byte = self.vm.instructions[self.ip];
        self.ip += 1;
        match byte {
            Instruction::OpCode(opcode) => opcode,
            _ => panic!("Expecting opcode"),
        }
    }

    pub(super) fn next_primitive(&mut self) -> usize {
        let byte = self.vm.instructions[self.ip];
        self.ip += 1;
        match byte {
            Instruction::Primitive(primitive) => primitive,
            _ => panic!("Expecting primitive"),
        }
    }

    pub fn stack_mut(&mut self) -> &mut Vec<Value> {
        &mut self.stack
    }

    pub fn parameter_stack(&self) -> &Vec<Option<VmArgument<'a>>> {
        &self.parameter_stack
    }

    pub fn parameter_stack_mut(&mut self) -> &mut Vec<Option<VmArgument<'a>>> {
        &mut self.parameter_stack
    }

    pub(super) fn read_constant(&mut self) -> Result<Literal, String> {
        let idx = self.next_primitive();
        Ok(self.vm.values[idx].clone())
    }
}
