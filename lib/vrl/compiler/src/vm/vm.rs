use super::{state::VmState, Variable, VmArgumentList};
use crate::{
    expression::Literal, vm::argument_list::VmArgument, Context, ExpressionError, Function, Value,
};
use std::collections::BTreeMap;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum OpCode {
    Return,
    GetLocal,
    SetLocal,
    Constant,
    Negate,
    Add,
    Subtract,
    Multiply,
    Divide,
    Rem,
    Merge,
    Not,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    NotEqual,
    Equal,
    Pop,
    ClearError,
    JumpIfFalse,
    JumpIfTrue,
    JumpIfNotErr,
    Jump,
    Loop,
    SetPathInfallible,
    SetPath,
    GetPath,
    Call,
    CreateArray,
    CreateObject,
    EmptyParameter,
    MoveParameter,
    MoveStatic,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Instruction {
    OpCode(OpCode),
    Primitive(usize),
}

#[derive(Debug, Default)]
pub struct Vm {
    fns: Vec<Box<dyn Function + Send + Sync>>,
    pub(super) instructions: Vec<Instruction>,
    pub(super) values: Vec<Value>,
    targets: Vec<Variable>,
    static_params: Vec<Box<dyn std::any::Any + Send + Sync>>,
}

impl Vm {
    pub fn new(fns: Vec<Box<dyn Function + Send + Sync>>) -> Self {
        Self {
            fns,
            ..Default::default()
        }
    }

    pub fn add_constant(&mut self, object: Value) -> usize {
        self.values.push(object);
        self.values.len() - 1
    }

    pub fn write_chunk(&mut self, code: OpCode) {
        self.instructions.push(Instruction::OpCode(code));
    }

    pub fn write_chunk_at(&mut self, pos: usize, code: OpCode) {
        self.instructions[pos] = Instruction::OpCode(code);
    }

    pub fn instructions(&self) -> &Vec<Instruction> {
        &self.instructions
    }

    pub fn write_primitive(&mut self, code: usize) {
        self.instructions.push(Instruction::Primitive(code));
    }

    pub fn write_primitive_at(&mut self, pos: usize, code: usize) {
        self.instructions[pos] = Instruction::Primitive(code);
    }

    pub fn function(&self, function_id: usize) -> Option<&Box<dyn Function + Send + Sync>> {
        self.fns.get(function_id)
    }

    /*
    pub fn get_constant(&self, constant: &str) -> Option<usize> {
        self.values.iter().position(|obj| match obj {
            Value::Bytes(c) => c == constant,
            _ => false,
        })
    }
    */

    /// Gets a target from the list of targets used, if it hasn't already been added then add it.
    pub fn get_target(&mut self, target: &Variable) -> usize {
        match self.targets.iter().position(|t| t == target) {
            Some(pos) => pos,
            None => {
                self.targets.push(target.clone());
                self.targets.len() - 1
            }
        }
    }

    /// Adds a static argument to the list and returns the position of this in the list.
    pub fn add_static(&mut self, stat: Box<dyn std::any::Any + Send + Sync>) -> usize {
        self.static_params.push(stat);
        self.static_params.len() - 1
    }

    /// For debugging purposes, returns a list of strings representing the instructions and primitives.
    pub fn dissassemble(&self) -> Vec<String> {
        self.instructions
            .iter()
            .enumerate()
            .map(|(idx, inst)| match inst {
                Instruction::OpCode(opcode) => format!("{:04}: {:?}", idx, opcode),
                Instruction::Primitive(primitive) => format!("{:04}: {}", idx, primitive),
            })
            .collect()
    }

    pub fn emit_jump(&mut self, instruction: OpCode) -> usize {
        self.write_chunk(instruction);

        // Insert placeholder
        self.write_primitive(usize::MAX);

        self.instructions().len() - 1
    }

    pub fn patch_jump(&mut self, offset: usize) {
        let jump = self.instructions.len() - offset - 1;
        self.write_primitive_at(offset, jump);
    }

    pub fn interpret<'a>(&self, ctx: &mut Context<'a>) -> Result<Value, ExpressionError> {
        let mut state: VmState = VmState::new(self);

        loop {
            let next = state.next();

            match next {
                OpCode::Return => {
                    return Ok(state.stack.pop().unwrap_or(Value::Null));
                }
                OpCode::Constant => {
                    let value = state.read_constant()?;
                    state.stack.push(value);
                }
                OpCode::Negate => match state.stack.pop() {
                    None => return Err("Negating nothing".into()),
                    Some(Value::Float(value)) => state.stack.push(Value::Float(value * -1.0)),
                    _ => return Err("Negating non number".into()),
                },
                OpCode::Not => match state.stack.pop() {
                    None => return Err("Notting nothing".into()),
                    Some(Value::Boolean(value)) => state.stack.push(Value::Boolean(!value)),
                    _ => return Err("Notting non boolean".into()),
                },
                OpCode::Add => binary_op(&mut state, Value::try_add)?,
                OpCode::Subtract => binary_op(&mut state, Value::try_sub)?,
                OpCode::Multiply => binary_op(&mut state, Value::try_mul)?,
                OpCode::Divide => binary_op(&mut state, Value::try_div)?,
                OpCode::Rem => binary_op(&mut state, Value::try_rem)?,
                OpCode::Merge => binary_op(&mut state, Value::try_merge)?,
                OpCode::Greater => binary_op(&mut state, Value::try_gt)?,
                OpCode::GreaterEqual => binary_op(&mut state, Value::try_ge)?,
                OpCode::Less => binary_op(&mut state, Value::try_lt)?,
                OpCode::LessEqual => binary_op(&mut state, Value::try_le)?,
                OpCode::NotEqual => {
                    let rhs = state.pop_stack()?;
                    let lhs = state.pop_stack()?;
                    state.stack.push((!lhs.eq_lossy(&rhs)).into());
                }
                OpCode::Equal => {
                    let rhs = state.pop_stack()?;
                    let lhs = state.pop_stack()?;
                    state.stack.push(lhs.eq_lossy(&rhs).into());
                }
                OpCode::Pop => {
                    let _ = state.stack.pop();
                }
                OpCode::ClearError => {
                    state.error = None;
                }
                OpCode::GetLocal => {
                    let slot = state.next_primitive();
                    state.stack.push(state.stack[slot].clone());
                }
                OpCode::SetLocal => {
                    let slot = state.next_primitive();
                    state.stack[slot] = state.peek_stack()?.clone();
                }
                OpCode::JumpIfFalse => {
                    let jump = state.next_primitive();
                    if !is_truthy(state.peek_stack()?) {
                        state.ip += jump;
                    }
                }
                OpCode::JumpIfTrue => {
                    let jump = state.next_primitive();
                    if is_truthy(state.peek_stack()?) {
                        state.ip += jump;
                    }
                }
                OpCode::JumpIfNotErr => {
                    let jump = state.next_primitive();
                    if state.error.is_none() {
                        state.ip += jump;
                    }
                }
                OpCode::Jump => {
                    let jump = state.next_primitive();
                    state.ip += jump;
                }
                OpCode::Loop => {
                    let jump = state.next_primitive();
                    state.ip -= jump;
                }
                OpCode::SetPath => {
                    let variable = state.next_primitive();
                    let variable = &self.targets[variable];
                    let value = state.pop_stack()?;

                    match variable {
                        Variable::Internal(ident, path) => {
                            let path = match path {
                                Some(path) => path,
                                None => {
                                    ctx.state_mut().insert_variable(ident.clone(), value);
                                    continue;
                                }
                            };

                            // Update existing variable using the provided path, or create a
                            // new value in the store.
                            match ctx.state_mut().variable_mut(ident) {
                                Some(stored) => stored.insert_by_path(path, value),
                                None => ctx
                                    .state_mut()
                                    .insert_variable(ident.clone(), value.at_path(path)),
                            }
                        }
                        Variable::External(path) => ctx.target_mut().insert(path, value)?,
                        Variable::None => (),
                    }
                }
                OpCode::SetPathInfallible => {
                    let variable = state.next_primitive();
                    let variable = &self.targets[variable];

                    let error = state.next_primitive();
                    let error = &self.targets[error];

                    let default = state.next_primitive();
                    let default = &self.values[default];

                    match state.error.take() {
                        Some(err) => {
                            let err = Value::from(err.to_string());
                            set_variable(ctx, variable, default.clone())?;
                            set_variable(ctx, error, err)?;
                        }
                        None => {
                            let value = state.pop_stack()?;
                            set_variable(ctx, variable, value)?;
                            set_variable(ctx, error, Value::Null)?;
                        }
                    }
                }
                OpCode::GetPath => {
                    let variable = state.next_primitive();
                    let variable = &self.targets[variable];

                    match &variable {
                        Variable::External(path) => {
                            let value = ctx.target().get(path)?.unwrap_or(Value::Null);
                            state.stack.push(value);
                        }
                        Variable::Internal(ident, path) => {
                            let value = match ctx.state().variable(ident) {
                                Some(value) => match path {
                                    Some(path) => {
                                        value.get_by_path(path).cloned().unwrap_or(Value::Null)
                                    }
                                    None => value.clone(),
                                },
                                None => Value::Null,
                            };

                            state.stack.push(value);
                        }
                        Variable::None => state.stack.push(Value::Null),
                    }
                }
                OpCode::Call => {
                    let function_id = state.next_primitive();
                    let span_start = state.next_primitive();
                    let span_end = state.next_primitive();
                    let parameters = &self.fns[function_id].parameters();

                    let len = state.parameter_stack().len();
                    let args = state
                        .parameter_stack_mut()
                        .drain(len - parameters.len()..)
                        .collect();

                    let mut argumentlist = VmArgumentList::new(parameters, args);
                    let function = &self.fns[function_id];

                    match function.call(ctx, &mut argumentlist) {
                        Ok(result) => state.stack.push(result),
                        Err(err) => match err {
                            ExpressionError::Abort { .. } => {
                                panic!("abort errors must only be defined by `abort` statement")
                            }
                            ExpressionError::Error {
                                message,
                                labels,
                                notes,
                            } => {
                                // labels.push(Label::primary(message.clone(), self.span));
                                state.error = Some(ExpressionError::Error {
                                    message: format!(
                                        r#"function call error for "{}" at ({}:{}): {}"#,
                                        function.identifier(),
                                        span_start,
                                        span_end,
                                        message
                                    ),
                                    labels,
                                    notes,
                                });
                            }
                        },
                    }
                }
                OpCode::CreateArray => {
                    let count = state.next_primitive();
                    let mut arr = Vec::new();

                    for _ in 0..count {
                        arr.push(state.pop_stack()?);
                    }
                    arr.reverse();

                    state.stack.push(Value::Array(arr));
                }
                OpCode::CreateObject => {
                    let count = state.next_primitive();
                    let mut object = BTreeMap::new();

                    for _ in 0..count {
                        let value = state.pop_stack()?;
                        let key = state.pop_stack()?;
                        let key = String::from_utf8_lossy(&key.try_bytes().unwrap()).to_string();

                        object.insert(key, value);
                    }

                    state.stack.push(Value::Object(object));
                }
                OpCode::EmptyParameter => state.parameter_stack.push(None),
                OpCode::MoveParameter => state
                    .parameter_stack
                    .push(state.stack.pop().map(VmArgument::Value)),
                OpCode::MoveStatic => {
                    let idx = state.next_primitive();
                    state
                        .parameter_stack
                        .push(Some(VmArgument::Any(&self.static_params[idx])));
                }
            }
        }
    }
}

fn binary_op<F, E>(state: &mut VmState, fun: F) -> Result<(), ExpressionError>
where
    E: Into<ExpressionError>,
    F: Fn(Value, Value) -> Result<Value, E>,
{
    // If we are in an error state we don't want to perform the operation
    // so we pass the error along.
    if state.error.is_none() {
        let rhs = state.pop_stack()?;
        let lhs = state.pop_stack()?;
        match fun(lhs, rhs) {
            Ok(value) => state.stack.push(value),
            Err(err) => state.error = Some(err.into()),
        }
    }

    Ok(())
}

/// Sets the value of the given variable to the provided value.
fn set_variable<'a>(
    ctx: &mut Context<'a>,
    variable: &Variable,
    value: Value,
) -> Result<(), ExpressionError> {
    match variable {
        Variable::Internal(ident, path) => {
            let path = match path {
                Some(path) => path,
                None => {
                    ctx.state_mut().insert_variable(ident.clone(), value);
                    return Ok(());
                }
            };

            // Update existing variable using the provided path, or create a
            // new value in the store.
            match ctx.state_mut().variable_mut(ident) {
                Some(stored) => stored.insert_by_path(path, value),
                None => ctx
                    .state_mut()
                    .insert_variable(ident.clone(), value.at_path(path)),
            }
        }
        Variable::External(path) => ctx.target_mut().insert(path, value)?,
        Variable::None => (),
    }

    Ok(())
}

fn is_truthy(object: &Value) -> bool {
    !matches!(object, Value::Null | Value::Boolean(false))
}
