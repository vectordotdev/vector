use crate::expression::Block;
use crate::parser::{Ident, Node};
use crate::{Context, Expression, ExpressionError, Value};
use std::collections::vec_deque::VecDeque;
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionClosure {
    variables: Vec<Node<Ident>>,
    block: Block,
}

impl FunctionClosure {
    pub(crate) fn new(variables: Vec<Node<Ident>>, block: Block) -> Self {
        Self { variables, block }
    }

    // pub(crate) fn variables(&self) -> &[Node<Ident>] {
    //     &self.variables
    // }

    pub fn resolve(
        &self,
        ctx: &mut Context,
        value: Value,
        mut func: impl FnMut(&Context, Value, &mut Value) -> Result<(), ExpressionError>,
        recursive: bool,
        mut result: Value,
    ) -> Result<Value, ExpressionError> {
        match value {
            Value::Object(object) => {
                let key_ident = self
                    .variables
                    .get(0)
                    .expect("checked at compile-time")
                    .clone()
                    .into_inner();
                let value_ident = self
                    .variables
                    .get(1)
                    .expect("checked at compile-time")
                    .clone()
                    .into_inner();

                for (key, value) in ObjectRecursive::create_iter(object, recursive) {
                    let state = ctx.state_mut();
                    state.insert_variable(key_ident.clone(), key.into());
                    state.insert_variable(value_ident.clone(), value);

                    let output = self.block.resolve(ctx)?;

                    func(ctx, output, &mut result)?;

                    let state = ctx.state_mut();
                    state.remove_variable(&key_ident);
                    state.remove_variable(&value_ident);
                }
                Ok(result)
            }
            Value::Array(array) => {
                let index_ident = self
                    .variables
                    .get(0)
                    .expect("checked at compile-time")
                    .clone()
                    .into_inner();
                let value_ident = self
                    .variables
                    .get(1)
                    .expect("checked at compile-time")
                    .clone()
                    .into_inner();

                for (index, value) in ArrayRecursive::create_iter(array, recursive) {
                    let state = ctx.state_mut();
                    state.insert_variable(index_ident.clone(), index.into());
                    state.insert_variable(value_ident.clone(), value);

                    let output = self.block.resolve(ctx)?;

                    func(ctx, output, &mut result)?;

                    let state = ctx.state_mut();
                    state.remove_variable(&index_ident);
                    state.remove_variable(&value_ident);
                }
                Ok(result)
            }
            _ => unimplemented!(),
        }
    }
}

impl fmt::Display for FunctionClosure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO
        f.write_str("{ |")?;

        let mut iter = self.variables.iter().peekable();
        while let Some(var) = iter.next() {
            var.fmt(f)?;

            if iter.peek().is_some() {
                f.write_str(", ")?;
            }
        }

        f.write_str("|\n")?;
        self.block.fmt(f)?;

        f.write_str("\n}")
    }
}

struct ObjectRecursive {
    queue: VecDeque<(String, Value)>,
}

impl ObjectRecursive {
    fn new(map: BTreeMap<String, Value>) -> Self {
        let queue = map.into_iter().collect();
        Self { queue }
    }

    fn create_iter(
        map: BTreeMap<String, Value>,
        recursive: bool,
    ) -> Box<dyn Iterator<Item = (String, Value)>> {
        if recursive {
            Box::new(ObjectRecursive::new(map).into_iter())
        } else {
            Box::new(map.into_iter())
        }
    }
}

impl Iterator for ObjectRecursive {
    type Item = (String, Value);

    fn next(&mut self) -> Option<Self::Item> {
        let (key, value) = self.queue.pop_front()?;
        match value {
            Value::Object(ref map) => {
                self.queue.extend(map.clone().into_iter());
                Some((key, value))
            }
            _ => Some((key, value)),
        }
    }
}

struct ArrayRecursive {
    queue: VecDeque<(usize, Value)>,
}

impl ArrayRecursive {
    fn new(array: Vec<Value>) -> Self {
        let queue = array.into_iter().enumerate().collect();
        Self { queue }
    }

    fn create_iter(array: Vec<Value>, recursive: bool) -> Box<dyn Iterator<Item = (usize, Value)>> {
        if recursive {
            Box::new(ArrayRecursive::new(array).into_iter())
        } else {
            Box::new(array.into_iter().enumerate())
        }
    }
}

impl Iterator for ArrayRecursive {
    type Item = (usize, Value);

    fn next(&mut self) -> Option<Self::Item> {
        let (index, value) = self.queue.pop_front()?;
        match value {
            Value::Array(ref array) => {
                self.queue.extend(array.clone().into_iter().enumerate());
                Some((index, value))
            }
            _ => Some((index, value)),
        }
    }
}
// impl crate::Expression for FunctionClosure {
//     fn resolve(&self, ctx: &mut crate::Context) -> Result<crate::Value, crate::ExpressionError> {
//         Ok(crate::value!(null))
//     }

//     fn type_def(&self, state: &crate::state::Compiler) -> crate::TypeDef {
//         crate::TypeDef::new().null()
//     }
// }
