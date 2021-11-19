use crate::{expression::Literal, function::VmArgumentList, Context, Function, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

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
    DefineGlobal,
    GetGlobal,
    SetGlobal,
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
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Instruction {
    OpCode(OpCode),
    Primitive(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Variable {
    Internal,
    External(lookup::LookupBuf),
}

#[derive(Clone, Debug, Default)]
pub struct Vm {
    fns: Arc<Vec<Box<dyn Function + Send + Sync>>>,
    instructions: Vec<Instruction>,
    globals: HashMap<String, Value>,
    values: Vec<Literal>,
    targets: Vec<Variable>,
    stack: Vec<Value>,
    parameter_stack: Vec<Option<Value>>,
    ip: usize,
}

impl Vm {
    pub fn new(fns: Vec<Box<dyn Function + Send + Sync>>) -> Self {
        Self {
            fns: Arc::new(fns),
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

    pub fn stack_mut(&mut self) -> &mut Vec<Value> {
        &mut self.stack
    }

    pub fn parameter_stack(&self) -> &Vec<Option<Value>> {
        &self.parameter_stack
    }

    pub fn parameter_stack_mut(&mut self) -> &mut Vec<Option<Value>> {
        &mut self.parameter_stack
    }

    fn next(&mut self) -> OpCode {
        let byte = self.instructions[self.ip];
        self.ip += 1;
        match byte {
            Instruction::OpCode(opcode) => opcode,
            _ => panic!("Expecting opcode"),
        }
    }

    fn next_primitive(&mut self) -> usize {
        let byte = self.instructions[self.ip];
        self.ip += 1;
        match byte {
            Instruction::Primitive(primitive) => primitive,
            _ => panic!("Expecting primitive"),
        }
    }

    pub fn get_constant(&self, constant: &str) -> Option<usize> {
        self.values.iter().position(|obj| match obj {
            Literal::String(c) => c == constant,
            _ => false,
        })
    }

    fn read_constant(&mut self) -> Result<Literal, String> {
        let idx = self.next_primitive();
        Ok(self.values[idx].clone())
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

    /// Resets the VM back to it's original state.
    pub fn reset(&mut self) {
        self.stack.clear();
        self.ip = 0;
    }

    pub fn interpret<'a>(&mut self, ctx: &mut Context<'a>) -> Result<Value, String> {
        loop {
            let next = self.next();
            match next {
                OpCode::Return => {
                    return Ok(self.stack.pop().unwrap_or(Value::Null));
                }
                OpCode::Constant => {
                    let value = self.read_constant()?;
                    self.stack.push(value.to_value());
                }
                OpCode::Negate => match self.stack.pop() {
                    None => return Err("Negating nothing".to_string()),
                    Some(Value::Float(value)) => self.stack.push(Value::Float(value * -1.0)),
                    _ => return Err("Negating non number".to_string()),
                },
                OpCode::Not => match self.stack.pop() {
                    None => return Err("Notting nothing".to_string()),
                    Some(Value::Boolean(value)) => self.stack.push(Value::Boolean(!value)),
                    _ => return Err("Notting non boolean".to_string()),
                },
                OpCode::Add => {
                    binary_op!(self,
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
                OpCode::Subtract => binary_op!(self,
                    (Some(Value::Integer(value1)), Some(Value::Integer(value2))) => Value::Integer(value1 - value2),),
                OpCode::Multiply => binary_op!(self,
                    (Some(Value::Integer(value1)), Some(Value::Integer(value2))) => Value::Integer(value1 * value2),),
                OpCode::Divide => binary_op!(self,
                    (Some(Value::Integer(value1)), Some(Value::Integer(value2))) => Value::Integer(value1 / value2),),
                OpCode::Print => match self.stack.pop() {
                    None => return Err("Negating nothing".to_string()),
                    Some(value) => println!("{}", value),
                },
                OpCode::Greater => binary_op!(self,
                    (Some(Value::Float(value1)), Some(Value::Float(value2))) => Value::Boolean(value1 > value2),),
                OpCode::GreaterEqual => binary_op!(self,
                    (Some(Value::Float(value1)), Some(Value::Float(value2))) => Value::Boolean(value1 >= value2),),
                OpCode::Less => binary_op!(self,
                    (Some(Value::Float(value1)), Some(Value::Float(value2))) => Value::Boolean(value1 < value2),),
                OpCode::LessEqual => binary_op!(self,
                    (Some(Value::Float(value1)), Some(Value::Float(value2))) => Value::Boolean(value1 <= value2),),
                OpCode::NotEqual => binary_op!(self,
                    (Some(value1), Some(value2)) => Value::Boolean(value1 != value2),),
                OpCode::Equal => binary_op!(self,
                    (Some(value1), Some(value2)) => Value::Boolean(value1 == value2),),
                OpCode::Pop => {
                    let _ = self.stack.pop();
                }
                OpCode::DefineGlobal => match self.read_constant()? {
                    Literal::String(name) => {
                        self.globals.insert(
                            String::from_utf8_lossy(&name).to_string(),
                            self.stack
                                .pop()
                                .ok_or_else(|| "No global to set".to_string())?,
                        );
                    }
                    _ => panic!("oooooo"),
                },
                OpCode::GetGlobal => match self.read_constant()? {
                    Literal::String(name) => {
                        let name = String::from_utf8_lossy(&name).to_string();
                        match self.globals.get(&name) {
                            Some(value) => self.stack.push(value.clone()),
                            None => return Err(format!("Undefined variable {}", name)),
                        }
                    }
                    _ => panic!("errr"),
                },
                OpCode::SetGlobal => match self.stack.pop() {
                    Some(obj) => match self.read_constant()? {
                        Literal::String(name) => {
                            self.globals
                                .insert(String::from_utf8_lossy(&name).to_string(), obj);
                        }
                        _ => panic!("arg"),
                    },
                    None => panic!("No var"),
                },
                OpCode::GetLocal => {
                    let slot = self.next_primitive();
                    self.stack.push(self.stack[slot].clone());
                }
                OpCode::SetLocal => {
                    let slot = self.next_primitive();
                    self.stack[slot] = self.stack[self.stack.len() - 1].clone();
                }
                OpCode::JumpIfFalse => {
                    let jump = self.next_primitive();
                    if !is_truthy(&self.stack[self.stack.len() - 1]) {
                        self.ip += jump;
                    }
                }
                OpCode::Jump => {
                    let jump = self.next_primitive();
                    self.ip += jump;
                }
                OpCode::Loop => {
                    let jump = self.next_primitive();
                    self.ip -= jump;
                }
                OpCode::SetPath => {
                    let variable = self.next_primitive();
                    let variable = &self.targets[variable];
                    let value = self.stack.pop().unwrap();

                    match variable {
                        Variable::Internal => unimplemented!("variables are rubbish"),
                        Variable::External(path) => ctx.target_mut().insert(path, value)?,
                    }
                }
                OpCode::GetPath => {
                    let variable = self.next_primitive();
                    let variable = &self.targets[variable];

                    match &variable {
                        Variable::External(path) => {
                            let value = ctx.target().get(path)?.unwrap_or(Value::Null);
                            self.stack.push(value);
                        }
                        Variable::Internal => unimplemented!("variables are junk"),
                    }
                }
                OpCode::Call => {
                    let function_id = self.next_primitive();
                    let parameters = &self.fns[function_id].parameters();

                    let len = self.parameter_stack().len();
                    let args = self
                        .parameter_stack_mut()
                        .drain(len - parameters.len()..)
                        .collect();
                    let argumentlist = VmArgumentList::new(parameters, args);

                    // TODO Handle errors
                    self.stack.push(self.fns[function_id].call(argumentlist));
                }
                OpCode::CreateObject => {
                    let count = self.next_primitive();
                    let mut object = BTreeMap::new();

                    for _ in 0..count {
                        let value = self.stack.pop().unwrap();
                        let key = self.stack.pop().unwrap();
                        let key = String::from_utf8_lossy(&key.try_bytes().unwrap()).to_string();

                        object.insert(key, value);
                    }

                    self.stack.push(Value::Object(object))
                }
                OpCode::EmptyParameter => self.parameter_stack.push(None),
                OpCode::MoveParameter => self.parameter_stack.push(self.stack.pop()),
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
