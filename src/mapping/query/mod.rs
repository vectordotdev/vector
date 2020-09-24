use crate::{
    event::{Event, Value},
    mapping::Result,
};
use std::collections::HashMap;

pub mod arithmetic;
pub mod functions;
pub mod path;

/// A parameter definition accepted by a function.
#[derive(Clone)]
pub(in crate::mapping) struct Parameter {
    /// The keyword of the parameter.
    ///
    /// Arguments can be passed in both using the keyword, or as a positional
    /// argument.
    pub keyword: &'static str,

    /// The parser calls this method to determine if a given argument value is
    /// accepted by the parameter.
    pub accepts: fn(&Value) -> bool,

    /// Whether or not this is a required parameter.
    ///
    /// If it isn't, the function can be called without errors, even if the
    /// argument matching this parameter is missing.
    pub required: bool,
}

impl std::fmt::Debug for Parameter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Parameter")
            .field("keyword", &self.keyword)
            .field("required", &self.required)
            .field("accepts", &"fn(&Value) -> bool".to_owned())
            .finish()
    }
}

pub(in crate::mapping) trait Function: Send + core::fmt::Debug {
    /// Run the function to produce a [`Value`].
    fn execute(&self, context: &Event) -> Result<Value>;

    /// Return the static set of parameters this function accepts.
    fn parameters() -> &'static [Parameter]
    where
        Self: Sized,
    {
        &[]
    }
}

#[derive(Debug, Default)]
pub(in crate::mapping) struct ArgumentList {
    /// The list of arguments provided to a function.
    arguments: Vec<Argument>,

    /// An optional mapping from argument keyword to position, if a keyword was
    /// provided for the given argument.
    keywords: HashMap<String, usize>,
}

impl ArgumentList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, argument: Argument, keyword: Option<String>) {
        self.arguments.push(argument);

        if let Some(keyword) = keyword {
            self.keywords.insert(keyword, self.arguments.len() - 1);
        }
    }

    pub fn optional(&mut self, keyword: &str) -> Option<Box<dyn Function>> {
        self.take(keyword)
    }

    pub fn required(&mut self, keyword: &str) -> Result<Box<dyn Function>> {
        self.take(keyword)
            .ok_or(format!("unknown keyword: {}", keyword))
    }

    pub fn keywords(&self) -> Vec<&str> {
        self.keywords.keys().map(String::as_str).collect()
    }

    pub fn len(&self) -> usize {
        self.arguments.len()
    }

    fn take(&mut self, keyword: &str) -> Option<Box<dyn Function>> {
        self.arguments
            .iter()
            .position(|a| a.parameter.keyword == keyword)
            .map(|i| self.arguments.remove(i))
            .map(|v| Box::new(v) as _)
    }
}

pub(in crate::mapping) struct Argument {
    resolver: Box<dyn Function>,
    parameter: Parameter,
}

// delegates to resolver to satisfy tests in `mapping::parser`.
impl std::fmt::Debug for Argument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.resolver.fmt(f)
    }
}

impl Argument {
    pub fn new(resolver: Box<dyn Function>, parameter: Parameter) -> Self {
        Self {
            resolver,
            parameter,
        }
    }
}

impl Function for Argument {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        let value = self.resolver.execute(ctx)?;

        // Ask the parameter if it accepts the given value.
        if !(self.parameter.accepts)(&value) {
            return Err(format!(
                "invalid argument type '{}' for parameter '{}'",
                value.kind(),
                self.parameter.keyword
            ));
        }

        Ok(value)
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct Literal {
    value: Value,
}

impl From<Value> for Literal {
    fn from(value: Value) -> Self {
        Self { value }
    }
}

impl Function for Literal {
    fn execute(&self, _: &Event) -> Result<Value> {
        Ok(self.value.clone())
    }
}
