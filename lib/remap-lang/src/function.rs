use crate::{expression, Expr, Expression, Result, Value};
use core::convert::{TryFrom, TryInto};
use std::collections::HashMap;

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
    #[error(r#"expected expression argument, got regex"#)]
    ArgumentExprRegex,

    #[error(r#"expected regex argument, got expression"#)]
    ArgumentRegexExpr,

    #[error(r#"missing required argument "{0}""#)]
    Required(String),

    #[error("unknown enum variant: {0}, must be one of: {}", .1.join(", "))]
    UnknownEnumVariant(String, &'static [&'static str]),
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

    pub fn required(&mut self, keyword: &str) -> Result<Argument> {
        self.optional(keyword)
            .ok_or_else(|| Error::Required(keyword.to_owned()).into())
    }

    pub fn optional_expr(&mut self, keyword: &str) -> Result<Option<Box<dyn Expression>>> {
        self.optional(keyword)
            .map(|v| v.try_into().map_err(Into::into))
            .transpose()
    }

    pub fn required_expr(&mut self, keyword: &str) -> Result<Box<dyn Expression>> {
        self.optional_expr(keyword)?
            .ok_or_else(|| Error::Required(keyword.to_owned()).into())
    }

    pub fn optional_regex(&mut self, keyword: &str) -> Result<Option<regex::Regex>> {
        self.optional(keyword)
            .map(|v| v.try_into().map_err(Into::into))
            .transpose()
    }

    pub fn required_regex(&mut self, keyword: &str) -> Result<regex::Regex> {
        self.optional_regex(keyword)?
            .ok_or_else(|| Error::Required(keyword.to_owned()).into())
    }

    pub fn optional_enum(
        &mut self,
        keyword: &str,
        variants: &'static [&'static str],
    ) -> Result<Option<String>> {
        let expr = self
            .optional(keyword)
            .map(|v| Expr::try_from(v))
            .transpose()?;

        let argument = match expr {
            Some(expr) => expression::Argument::try_from(expr)?,
            None => return Ok(None),
        };

        let variant = expression::Literal::try_from(argument.into_expr())?
            .as_value()
            .clone()
            .try_string()
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())?;

        if variants.contains(&variant.as_str()) {
            Ok(Some(variant))
        } else {
            Err(Error::UnknownEnumVariant(variant.to_owned(), &variants).into())
        }
    }

    pub fn required_enum(
        &mut self,
        keyword: &str,
        variants: &'static [&'static str],
    ) -> Result<String> {
        self.optional_enum(keyword, variants)?
            .ok_or_else(|| Error::Required(keyword.to_owned()).into())
    }

    pub fn keywords(&self) -> Vec<&'static str> {
        self.0.keys().copied().collect::<Vec<_>>()
    }

    pub fn insert(&mut self, k: &'static str, v: Argument) {
        self.0.insert(k, v);
    }
}

#[derive(Debug, Clone)]
pub enum Argument {
    Expression(Expr),
    Regex(regex::Regex),
}

impl<T: Into<Expr>> From<T> for Argument {
    fn from(expr: T) -> Self {
        Argument::Expression(expr.into())
    }
}

impl From<regex::Regex> for Argument {
    fn from(regex: regex::Regex) -> Self {
        Argument::Regex(regex)
    }
}

impl TryFrom<Argument> for Expr {
    type Error = Error;

    fn try_from(arg: Argument) -> std::result::Result<Self, Self::Error> {
        match arg {
            Argument::Expression(expr) => Ok(expr),
            Argument::Regex(_) => Err(Error::ArgumentExprRegex),
        }
    }
}

impl TryFrom<Argument> for Box<dyn Expression> {
    type Error = Error;

    fn try_from(arg: Argument) -> std::result::Result<Self, Self::Error> {
        match arg {
            Argument::Expression(expr) => Ok(Box::new(expr) as _),
            Argument::Regex(_) => Err(Error::ArgumentExprRegex),
        }
    }
}

impl TryFrom<Argument> for regex::Regex {
    type Error = Error;

    fn try_from(arg: Argument) -> std::result::Result<Self, Self::Error> {
        match arg {
            Argument::Regex(regex) => Ok(regex),
            Argument::Expression(_) => Err(Error::ArgumentRegexExpr),
        }
    }
}

pub trait Function: std::fmt::Debug + Sync + CloneFunction {
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

pub trait CloneFunction {
    fn clone_function(&self) -> Box<dyn Function>;
}

impl<T> CloneFunction for T
where
    T: Function + Clone + 'static,
{
    fn clone_function(&self) -> Box<dyn Function> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn Function> {
    fn clone(&self) -> Self {
        self.clone_function()
    }
}
