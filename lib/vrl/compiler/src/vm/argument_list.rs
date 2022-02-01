use crate::{value::Kind, ExpressionError, Parameter, Value};
use std::any::Any;

pub enum VmArgument<'a> {
    Value(Value),
    Any(&'a Box<dyn Any + Send + Sync>),
}

impl<'a> VmArgument<'a> {
    fn into_value(self) -> Value {
        match self {
            VmArgument::Value(value) => value,
            _ => panic!(),
        }
    }

    fn into_any(self) -> &'a Box<dyn Any + Send + Sync> {
        match self {
            VmArgument::Any(any) => any,
            _ => panic!(),
        }
    }

    /// Returns the kind that this parameter is.
    /// If the parameter is an `Any`, we return `None` since the function that has created this parameter will
    /// have already done the required typechecking.
    fn kind(&self) -> Option<Kind> {
        match self {
            VmArgument::Value(value) => Some(value.kind()),
            VmArgument::Any(_) => None,
        }
    }
}

pub struct VmArgumentList<'a> {
    args: &'static [Parameter],
    values: Vec<Option<VmArgument<'a>>>,
}

impl<'a> VmArgumentList<'a> {
    pub fn new(args: &'static [Parameter], values: Vec<Option<VmArgument<'a>>>) -> Self {
        Self { args, values }
    }

    fn argument_pos(&self, name: &str) -> usize {
        self.args
            .iter()
            .position(|param| param.keyword == name)
            .expect("parameter doesn't exist")
    }

    /// Returns the parameter with the given name.
    /// Note that this can only be called once per parameter since the value is
    /// removed from the list.
    pub fn required(&mut self, name: &str) -> Value {
        // Get the position where the given argument is found in the parameter stack.
        let pos = self.argument_pos(name);

        // Return the parameter found at this position.
        self.values[pos].take().unwrap().into_value()
    }

    /// Returns the parameter with the given name.
    /// Note that this can only be called once per parameter since the value is
    /// removed from the list.
    pub fn optional(&mut self, name: &str) -> Option<Value> {
        // Get the position where the given argument is found in the parameter stack.
        let pos = self.argument_pos(name);

        // Return the parameter found at this position.
        self.values[pos].take().map(|v| v.into_value())
    }

    /// Returns the parameter with the given name.
    /// Note that this can only be called once per parameter since the value is
    /// removed from the list.
    pub fn required_any(&mut self, name: &str) -> &'a Box<dyn Any + Send + Sync> {
        // Get the position where the given argument is found in the parameter stack.
        let pos = self.argument_pos(name);

        // Return the parameter found at this position.
        self.values[pos].take().unwrap().into_any()
    }

    /// Returns the parameter with the given name.
    /// Note that this can only be called once per parameter since the value is
    /// removed from the list.
    pub fn optional_any(&mut self, name: &str) -> Option<&'a Box<dyn Any + Send + Sync>> {
        // Get the position where the given argument is found in the parameter stack.
        let pos = self.argument_pos(name);

        // Return the parameter found at this position.
        self.values[pos].take().map(|v| v.into_any())
    }

    /// Validates the arguments are correct.
    pub fn check_arguments(&self) -> Result<(), ExpressionError> {
        for (param, args) in self.args.iter().zip(self.values.iter()) {
            match args.as_ref() {
                None if param.required => return Err("parameter is required".into()),
                Some(arg) if matches!(arg.kind(), Some(kind) if !param.kind().intersects(kind)) => {
                    return Err("parameter type mismatch".into())
                }
                _ => (),
            }
        }
        Ok(())
    }
}
