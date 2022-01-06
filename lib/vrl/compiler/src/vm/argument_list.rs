use crate::{Parameter, Value};
use std::any::Any;

pub enum VmArgument<'a> {
    Value(Value),
    Any(&'a Box<dyn Any + Send + Sync>),
}

impl<'a> VmArgument<'a> {
    fn as_value(self) -> Value {
        match self {
            VmArgument::Value(value) => value,
            _ => panic!(),
        }
    }

    fn as_any(self) -> &'a Box<dyn Any + Send + Sync> {
        match self {
            VmArgument::Any(any) => any,
            _ => panic!(),
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

    /// Returns the parameter with the given name.
    /// Note the this can only be called once per parameter since the value is
    /// removed from the list.
    pub fn required(&mut self, name: &str) -> Value {
        // Get the position the given argument is found in the parameter stack.
        let pos = self
            .args
            .iter()
            .position(|param| param.keyword == name)
            .expect("parameter doesn't exist");

        // Return the parameter found at this position.
        self.values[pos].take().unwrap().as_value()
    }

    /// Returns the parameter with the given name.
    /// Note the this can only be called once per parameter since the value is
    /// removed from the list.
    pub fn optional(&mut self, name: &str) -> Option<Value> {
        // Get the position the given argument is found in the parameter stack.
        let pos = self
            .args
            .iter()
            .position(|param| param.keyword == name)
            .expect("parameter doesn't exist");

        // Return the parameter found at this position.
        self.values[pos].take().map(|v| v.as_value())
    }

    /// Returns the parameter with the given name.
    /// Note the this can only be called once per parameter since the value is
    /// removed from the list.
    pub fn required_any(&mut self, name: &str) -> &'a Box<dyn Any + Send + Sync> {
        // Get the position the given argument is found in the parameter stack.
        let pos = self
            .args
            .iter()
            .position(|param| param.keyword == name)
            .expect("parameter doesn't exist");

        // Return the parameter found at this position.
        self.values[pos].take().unwrap().as_any()
    }

    /// Returns the parameter with the given name.
    /// Note the this can only be called once per parameter since the value is
    /// removed from the list.
    pub fn optional_any(&mut self, name: &str) -> Option<&'a Box<dyn Any + Send + Sync>> {
        // Get the position the given argument is found in the parameter stack.
        let pos = self
            .args
            .iter()
            .position(|param| param.keyword == name)
            .expect("parameter doesn't exist");

        // Return the parameter found at this position.
        self.values[pos].take().map(|v| v.as_any())
    }
}
