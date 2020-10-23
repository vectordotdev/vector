use crate::{Expression, Result, Value};
use core::convert::TryInto;
use std::collections::HashMap;

mod split;

pub(crate) use split::Split;

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error(r#"expected expression argument, got regex"#)]
    ArgumentExprRegex,

    #[error(r#"missing required argument "{0}""#)]
    Required(String),
}

#[derive(Copy, Clone)]
pub struct Parameter {
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

#[derive(Debug, Default)]
pub struct ArgumentList(HashMap<&'static str, Argument>);

impl ArgumentList {
    pub fn optional(&mut self, keyword: &str) -> Option<Argument> {
        self.0.remove(keyword)
    }

    pub fn optional_expr(&mut self, keyword: &str) -> Result<Option<Box<dyn Expression>>> {
        self.optional(keyword)
            .map(|v| v.try_into().map_err(Into::into))
            .transpose()
    }

    pub fn required(&mut self, keyword: &str) -> Result<Argument> {
        self.0
            .remove(keyword)
            .ok_or_else(|| Error::Required(keyword.to_owned()).into())
    }

    pub fn required_expr(&mut self, keyword: &str) -> Result<Box<dyn Expression>> {
        self.required(keyword)
            .and_then(|v| v.try_into().map_err(Into::into))
    }

    pub fn keywords(&self) -> Vec<&'static str> {
        self.0.keys().copied().collect::<Vec<_>>()
    }

    pub fn insert(&mut self, k: &'static str, v: Argument) {
        self.0.insert(k, v);
    }
}

#[derive(Debug)]
pub enum Argument {
    Expression(Box<dyn Expression>),
    Regex(regex::Regex),
}

impl TryInto<Box<dyn Expression>> for Argument {
    type Error = Error;

    fn try_into(self) -> std::result::Result<Box<dyn Expression>, Self::Error> {
        match self {
            Argument::Expression(expr) => Ok(expr),
            Argument::Regex(_) => Err(Error::ArgumentExprRegex),
        }
    }
}

pub trait Function: std::fmt::Debug {
    /// The identifier by which the function can be called.
    fn identifier(&self) -> &'static str;

    /// Compile a [`Function`] into a type that can be resolved to an
    /// [`Expression`].
    ///
    /// This function is called at compile-time for any `Function` used in the
    /// program.
    ///
    /// At runtime, the `Expression` returned by this function is executed and
    /// resolved to its final [`Value`].
    fn compile(&self, arguments: ArgumentList) -> Result<Box<dyn Expression>>;

    /// An optional list of parameters the function accepts.
    ///
    /// This list is used at compile-time to check function arity and keyword
    /// names. The parameter also defines which variants of the [`Argument`]
    /// enum the function accepts.
    ///
    /// At runtime, if the parameter accepts `Argument::Expression`, the
    /// resolved `Value` type is checked against the parameter properties.
    fn parameters(&self) -> &'static [Parameter] {
        &[]
    }
}
