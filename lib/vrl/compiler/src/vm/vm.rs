use super::{state::VmState, Variable, VmArgumentList};
use crate::{expression::Literal, vm::argument_list::VmArgument, Context, Function, Value};
use std::collections::{BTreeMap, HashMap};

macro_rules! binary_op {
    ($self: ident, $($pat: pat => $expr: expr,)*) => {{
        let a = $self.stack.pop();
        let b = $self.stack.pop();
        match (b, a) {
            $($pat => $self.stack.push($expr),)*
            _ => {
                return Err(
                    "binary op invalid type".to_string()
                )
            }
        }
    }};
}

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
    Print,
    Not,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    NotEqual,
    Equal,
    Pop,
    JumpIfFalse,
    Jump,
    Loop,
    SetPath,
    GetPath,
    Call,
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
    pub(super) values: Vec<Literal>,
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

    pub fn add_constant(&mut self, object: Literal) -> usize {
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

    pub fn get_constant(&self, constant: &str) -> Option<usize> {
        self.values.iter().position(|obj| match obj {
            Literal::String(c) => c == constant,
            _ => false,
        })
    }

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

    pub fn interpret<'a>(&self, ctx: &mut Context<'a>) -> Result<Value, String> {
        let mut state: VmState = VmState::new(self);

        loop {
            let next = state.next();
            match next {
                OpCode::Return => {
                    return Ok(state.stack.pop().unwrap_or(Value::Null));
                }
                OpCode::Constant => {
                    let value = state.read_constant()?;
                    state.stack.push(value.to_value());
                }
                OpCode::Negate => match state.stack.pop() {
                    None => return Err("Negating nothing".to_string()),
                    Some(Value::Float(value)) => state.stack.push(Value::Float(value * -1.0)),
                    _ => return Err("Negating non number".to_string()),
                },
                OpCode::Not => match state.stack.pop() {
                    None => return Err("Notting nothing".to_string()),
                    Some(Value::Boolean(value)) => state.stack.push(Value::Boolean(!value)),
                    _ => return Err("Notting non boolean".to_string()),
                },
                OpCode::Add => {
                    binary_op!(state,
                                (Some(Value::Float(value1)), Some(Value::Float(value2))) => Value::Float(value1 + value2),
                                (Some(Value::Bytes(value1)), Some(Value::Bytes(value2))) => Value::Bytes({
                                    use bytes::{BytesMut, BufMut};
                                    let mut value = BytesMut::with_capacity(value1.len() + value2.len());
                                    value.put(value1);
                                    value.put(value2);
                                    value.into()
                                }),
                    )
                }
                OpCode::Subtract => binary_op!(state,
                    (Some(Value::Integer(value1)), Some(Value::Integer(value2))) => Value::Integer(value1 - value2),),
                OpCode::Multiply => binary_op!(state,
                    (Some(Value::Integer(value1)), Some(Value::Integer(value2))) => Value::Integer(value1 * value2),),
                OpCode::Divide => binary_op!(state,
                    (Some(Value::Integer(value1)), Some(Value::Integer(value2))) => Value::Integer(value1 / value2),),
                OpCode::Print => match state.stack.pop() {
                    None => return Err("Negating nothing".to_string()),
                    Some(value) => println!("{}", value),
                },
                OpCode::Greater => binary_op!(state,
                    (Some(Value::Float(value1)), Some(Value::Float(value2))) => Value::Boolean(value1 > value2),),
                OpCode::GreaterEqual => binary_op!(state,
                    (Some(Value::Float(value1)), Some(Value::Float(value2))) => Value::Boolean(value1 >= value2),),
                OpCode::Less => binary_op!(state,
                    (Some(Value::Float(value1)), Some(Value::Float(value2))) => Value::Boolean(value1 < value2),),
                OpCode::LessEqual => binary_op!(state,
                    (Some(Value::Float(value1)), Some(Value::Float(value2))) => Value::Boolean(value1 <= value2),),
                OpCode::NotEqual => binary_op!(state,
                    (Some(value1), Some(value2)) => Value::Boolean(value1 != value2),),
                OpCode::Equal => binary_op!(state,
                    (Some(value1), Some(value2)) => Value::Boolean(value1 == value2),),
                OpCode::Pop => {
                    let _ = state.stack.pop();
                }
                OpCode::GetLocal => {
                    let slot = state.next_primitive();
                    state.stack.push(state.stack[slot].clone());
                }
                OpCode::SetLocal => {
                    let slot = state.next_primitive();
                    state.stack[slot] = state.stack[state.stack.len() - 1].clone();
                }
                OpCode::JumpIfFalse => {
                    let jump = state.next_primitive();
                    if !is_truthy(&state.stack[state.stack.len() - 1]) {
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
                    let value = state.stack.pop().unwrap();

                    match variable {
                        Variable::Internal => unimplemented!("variables are rubbish"),
                        Variable::External(path) => ctx.target_mut().insert(path, value)?,
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
                        Variable::Internal => unimplemented!("variables are junk"),
                    }
                }
                OpCode::Call => {
                    let function_id = state.next_primitive();
                    let parameters = &self.fns[function_id].parameters();

                    let len = state.parameter_stack().len();
                    let args = state
                        .parameter_stack_mut()
                        .drain(len - parameters.len()..)
                        .collect();

                    let mut argumentlist = VmArgumentList::new(parameters, args);

                    match self.fns[function_id].call(ctx, &mut argumentlist) {
                        Ok(result) => state.stack.push(result),
                        Err(err) => {
                            println!("{:?}", err);
                            todo!()
                        }
                    }
                }
                OpCode::CreateObject => {
                    let count = state.next_primitive();
                    let mut object = BTreeMap::new();

                    for _ in 0..count {
                        let value = state.stack.pop().unwrap();
                        let key = state.stack.pop().unwrap();
                        let key = String::from_utf8_lossy(&key.try_bytes().unwrap()).to_string();

                        object.insert(key, value);
                    }

                    state.stack.push(Value::Object(object))
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

fn is_truthy(object: &Value) -> bool {
    match object {
        Value::Null => false,
        Value::Boolean(false) => false,
        _ => true,
    }
}
