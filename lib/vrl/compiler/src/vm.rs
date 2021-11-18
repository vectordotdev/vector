use crate::{expression::Literal, function::VmArgumentList, Context, Function, Value};
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

pub const RETURN: u8 = 1;
pub const DEFINEGLOBAL: u8 = 2;
pub const GETGLOBAL: u8 = 3;
pub const SETGLOBAL: u8 = 4;
pub const GETLOCAL: u8 = 5;
pub const SETLOCAL: u8 = 6;
pub const CONSTANT: u8 = 7;
pub const NEGATE: u8 = 8;
pub const ADD: u8 = 9;
pub const SUBTRACT: u8 = 10;
pub const MULTIPLY: u8 = 11;
pub const DIVIDE: u8 = 12;
pub const PRINT: u8 = 13;
pub const NOT: u8 = 14;
pub const GREATER: u8 = 15;
pub const GREATEREQUAL: u8 = 16;
pub const LESS: u8 = 17;
pub const LESSEQUAL: u8 = 18;
pub const NOTEQUAL: u8 = 19;
pub const EQUAL: u8 = 20;
pub const POP: u8 = 21;
pub const JUMPIFFALSE: u8 = 22;
pub const JUMP: u8 = 23;
pub const LOOP: u8 = 24;
pub const SETPATH: u8 = 25;
pub const GETPATH: u8 = 29;
pub const CALL: u8 = 30;
pub const CREATEOBJECT: u8 = 31;
pub const EMPTYPARAMETER: u8 = 32;
pub const MOVEPARAMETER: u8 = 33;

#[derive(Clone, Debug, PartialEq)]
pub enum Variable {
    Internal,
    External(lookup::LookupBuf),
}

#[derive(Clone, Debug, Default)]
pub struct Vm {
    instructions: Vec<u8>,
    globals: HashMap<String, Value>,
    values: Vec<Literal>,
    targets: Vec<Variable>,
    stack: Vec<Value>,
    parameter_stack: Vec<Option<Value>>,
    ip: usize,
}

impl Vm {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn add_constant(&mut self, object: Literal) -> usize {
        self.values.push(object);
        self.values.len() - 1
    }

    pub fn write_chunk(&mut self, code: u8) {
        self.instructions.push(code);
    }

    pub fn write_chunk_at(&mut self, pos: usize, code: u8) {
        self.instructions[pos] = code;
    }

    pub fn instructions(&self) -> &Vec<u8> {
        &self.instructions
    }

    pub fn write_primitive(&mut self, code: usize) {
        self.instructions.push(code as u8);
    }

    pub fn write_primitive_at(&mut self, pos: usize, code: usize) {
        self.instructions[pos] = code as u8;
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

    fn next(&mut self) -> u8 {
        let byte = self.instructions[self.ip];
        self.ip += 1;
        byte
    }

    fn next_primitive(&mut self) -> usize {
        let byte = self.instructions[self.ip];
        self.ip += 1;
        byte as usize
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
        /*
        self.instructions
            .iter()
            .enumerate()
            .map(|(idx, inst)| match inst {
                Instruction::OpCode(opcode) => format!("{:04}: {:?}", idx, opcode),
                Instruction::Primitive(primitive) => format!("{:04}: {}", idx, primitive),
            })
            .collect()
        */
        Vec::new()
    }

    pub fn emit_jump(&mut self, instruction: u8) -> usize {
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

    pub fn interpret<'a>(
        &mut self,
        fns: &[Box<dyn Function>],
        ctx: &mut Context<'a>,
    ) -> Result<Value, String> {
        loop {
            let next = self.next();
            match next {
                RETURN => {
                    return Ok(self.stack.pop().unwrap_or(Value::Null));
                }
                CONSTANT => {
                    let value = self.read_constant()?;
                    self.stack.push(value.to_value());
                }
                NEGATE => match self.stack.pop() {
                    None => return Err("Negating nothing".to_string()),
                    Some(Value::Float(value)) => self.stack.push(Value::Float(value * -1.0)),
                    _ => return Err("Negating non number".to_string()),
                },
                NOT => match self.stack.pop() {
                    None => return Err("Notting nothing".to_string()),
                    Some(Value::Boolean(value)) => self.stack.push(Value::Boolean(!value)),
                    _ => return Err("Notting non boolean".to_string()),
                },
                ADD => {
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
                SUBTRACT => binary_op!(self,
                    (Some(Value::Integer(value1)), Some(Value::Integer(value2))) => Value::Integer(value1 - value2),),
                MULTIPLY => binary_op!(self,
                    (Some(Value::Integer(value1)), Some(Value::Integer(value2))) => Value::Integer(value1 * value2),),
                DIVIDE => binary_op!(self,
                    (Some(Value::Integer(value1)), Some(Value::Integer(value2))) => Value::Integer(value1 / value2),),
                PRINT => match self.stack.pop() {
                    None => return Err("Negating nothing".to_string()),
                    Some(value) => println!("{}", value),
                },
                GREATER => binary_op!(self,
                    (Some(Value::Float(value1)), Some(Value::Float(value2))) => Value::Boolean(value1 > value2),),
                GREATEREQUAL => binary_op!(self,
                    (Some(Value::Float(value1)), Some(Value::Float(value2))) => Value::Boolean(value1 >= value2),),
                LESS => binary_op!(self,
                    (Some(Value::Float(value1)), Some(Value::Float(value2))) => Value::Boolean(value1 < value2),),
                LESSEQUAL => binary_op!(self,
                    (Some(Value::Float(value1)), Some(Value::Float(value2))) => Value::Boolean(value1 <= value2),),
                NOTEQUAL => binary_op!(self,
                    (Some(value1), Some(value2)) => Value::Boolean(value1 != value2),),
                EQUAL => binary_op!(self,
                    (Some(value1), Some(value2)) => Value::Boolean(value1 == value2),),
                POP => {
                    let _ = self.stack.pop();
                }
                DEFINEGLOBAL => match self.read_constant()? {
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
                GETGLOBAL => match self.read_constant()? {
                    Literal::String(name) => {
                        let name = String::from_utf8_lossy(&name).to_string();
                        match self.globals.get(&name) {
                            Some(value) => self.stack.push(value.clone()),
                            None => return Err(format!("Undefined variable {}", name)),
                        }
                    }
                    _ => panic!("errr"),
                },
                SETGLOBAL => match self.stack.pop() {
                    Some(obj) => match self.read_constant()? {
                        Literal::String(name) => {
                            self.globals
                                .insert(String::from_utf8_lossy(&name).to_string(), obj);
                        }
                        _ => panic!("arg"),
                    },
                    None => panic!("No var"),
                },
                GETLOCAL => {
                    let slot = self.next_primitive();
                    self.stack.push(self.stack[slot].clone());
                }
                SETLOCAL => {
                    let slot = self.next_primitive();
                    self.stack[slot] = self.stack[self.stack.len() - 1].clone();
                }
                JUMPIFFALSE => {
                    let jump = self.next_primitive();
                    if !is_truthy(&self.stack[self.stack.len() - 1]) {
                        self.ip += jump;
                    }
                }
                JUMP => {
                    let jump = self.next_primitive();
                    self.ip += jump;
                }
                LOOP => {
                    let jump = self.next_primitive();
                    self.ip -= jump;
                }
                SETPATH => {
                    let variable = self.next_primitive();
                    let variable = &self.targets[variable];
                    let value = self.stack.pop().unwrap();

                    match variable {
                        Variable::Internal => unimplemented!("variables are rubbish"),
                        Variable::External(path) => ctx.target_mut().insert(path, value)?,
                    }
                }
                GETPATH => {
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
                CALL => {
                    let function_id = self.next_primitive();
                    let function = &fns[function_id];

                    let argumentlist = VmArgumentList::new(function.parameters(), self);

                    // TODO Handle errors
                    self.stack.push(function.call(argumentlist));
                }
                CREATEOBJECT => {
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
                EMPTYPARAMETER => self.parameter_stack.push(None),
                MOVEPARAMETER => self.parameter_stack.push(self.stack.pop()),
                _ => panic!("Dodgy instruction"),
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
