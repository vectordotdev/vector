use crate::{
    expression::{self, Array, Literal, Path},
    Expr, Expression, Result, Value,
};
use core::convert::{TryFrom, TryInto};
use std::collections::HashMap;

// workaround for missing variable argument length.
//
// We'll come up with a nicer solution at some point. It took Rust five
// years to support [0; 34].
#[macro_export]
macro_rules! generate_param_list {
    (accepts = $accepts:expr, required = $required:expr, keywords = [$($k:literal),+ $(,)?] $(,)?) => (
        &[
            $(Parameter {
                keyword: $k,
                accepts: $accepts,
                required: $required,
            }),+
        ]
    );
}

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
    #[error(r#"expected expression argument, got regex"#)]
    ArgumentExprRegex,

    #[error(r#"expected expression argument, got array"#)]
    ArgumentExprArray,

    #[error(r#"expected regex argument, got expression"#)]
    ArgumentRegexExpr,

    #[error(r#"expected regex argument, got array"#)]
    ArgumentRegexArray,

    #[error(r#"expected array literal argument, got expression"#)]
    ArgumentArrayExpr,

    #[error(r#"expected array literal argument, got regex"#)]
    ArgumentArrayRegex,

    #[error(r#"expected expression or regex argument, got array literal"#)]
    ArgumentExprOrRegexArray,

    #[error(r#"missing required argument "{0}""#)]
    Required(String),

    #[error("unknown enum variant: {0}, must be one of: {}", .1.join(", "))]
    UnknownEnumVariant(String, Vec<&'static str>),
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

    pub fn optional_expr(&mut self, keyword: &str) -> Result<Option<Expr>> {
        self.optional(keyword)
            .map(|v| v.try_into().map_err(Into::into))
            .transpose()
    }

    pub fn required_expr(&mut self, keyword: &str) -> Result<Expr> {
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

    pub fn optional_literal(&mut self, keyword: &str) -> Result<Option<Literal>> {
        let expr = self.optional(keyword).map(Expr::try_from).transpose()?;

        let argument = match expr {
            Some(expr) => expression::Argument::try_from(expr)?,
            None => return Ok(None),
        };

        let variant = Literal::try_from(argument.into_expr())?;
        Ok(Some(variant))
    }

    pub fn required_literal(&mut self, keyword: &str) -> Result<Literal> {
        self.optional_literal(keyword)?
            .ok_or_else(|| Error::Required(keyword.to_owned()).into())
    }

    pub fn optional_enum(
        &mut self,
        keyword: &str,
        variants: &[&'static str],
    ) -> Result<Option<String>> {
        self.optional_literal(keyword)?
            .map(|lit| literal_to_enum_variant(lit, variants))
            .transpose()
    }

    pub fn required_enum(&mut self, keyword: &str, variants: &[&'static str]) -> Result<String> {
        self.optional_enum(keyword, variants)?
            .ok_or_else(|| Error::Required(keyword.to_owned()).into())
    }

    pub fn optional_path(&mut self, keyword: &str) -> Result<Option<Path>> {
        self.optional_expr(keyword)?
            .map(Path::try_from)
            .transpose()
            .map_err(Into::into)
    }

    pub fn required_path(&mut self, keyword: &str) -> Result<Path> {
        self.optional_path(keyword)?
            .ok_or_else(|| Error::Required(keyword.to_owned()).into())
    }

    pub fn optional_array(&mut self, keyword: &str) -> Result<Option<Array>> {
        self.optional_expr(keyword)?
            .map(|v| v.try_into().map_err(Into::into))
            .transpose()
    }

    pub fn required_array(&mut self, keyword: &str) -> Result<Array> {
        self.optional_array(keyword)?
            .ok_or_else(|| Error::Required(keyword.to_owned()).into())
    }

    pub fn optional_enum_list(
        &mut self,
        keyword: &str,
        variants: &[&'static str],
    ) -> Result<Option<Vec<String>>> {
        self.optional_array(keyword)?
            .map(|array| {
                array
                    .into_iter()
                    .map(|expr| Literal::try_from(expr).map_err(Into::into))
                    .map(|lit: Result<Literal>| literal_to_enum_variant(lit?, variants))
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()
    }

    pub fn required_enum_list(
        &mut self,
        keyword: &str,
        variants: &[&'static str],
    ) -> Result<Vec<String>> {
        self.optional_enum_list(keyword, variants)?
            .ok_or_else(|| Error::Required(keyword.to_owned()).into())
    }

    pub fn optional_expr_or_regex(&mut self, keyword: &str) -> Result<Option<Argument>> {
        self.optional(keyword)
            .map(|arg| match arg {
                Argument::Array(_) => Err(Error::ArgumentExprOrRegexArray),
                _ => Ok(arg),
            })
            .transpose()
            .map_err(Into::into)
    }

    pub fn required_expr_or_regex(&mut self, keyword: &str) -> Result<Argument> {
        self.optional_expr_or_regex(keyword)?
            .ok_or_else(|| Error::Required(keyword.to_owned()).into())
    }

    pub fn keywords(&self) -> Vec<&'static str> {
        self.0.keys().copied().collect::<Vec<_>>()
    }

    pub fn insert(&mut self, k: &'static str, v: Argument) {
        self.0.insert(k, v);
    }
}

fn literal_to_enum_variant(literal: Literal, variants: &[&'static str]) -> Result<String> {
    let variant = literal
        .into_value()
        .try_bytes()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())?;

    if variants.contains(&variant.as_str()) {
        Ok(variant)
    } else {
        Err(Error::UnknownEnumVariant(variant, variants.to_owned()).into())
    }
}

#[derive(Debug, Clone)]
pub enum Argument {
    Expression(Expr),
    Regex(regex::Regex),
    Array(Vec<Argument>),
}

impl<T: Into<Expr>> From<T> for Argument {
    fn from(expr: T) -> Self {
        Argument::Expression(expr.into())
    }
}

impl From<Vec<Argument>> for Argument {
    fn from(args: Vec<Argument>) -> Self {
        Argument::Array(args)
    }
}

impl TryFrom<Argument> for Expr {
    type Error = Error;

    fn try_from(arg: Argument) -> std::result::Result<Self, Self::Error> {
        match arg {
            Argument::Expression(expr) => Ok(expr),
            Argument::Regex(_) => Err(Error::ArgumentExprRegex),
            Argument::Array(_) => Err(Error::ArgumentExprArray),
        }
    }
}

impl TryFrom<Argument> for Box<dyn Expression> {
    type Error = Error;

    fn try_from(arg: Argument) -> std::result::Result<Self, Self::Error> {
        match arg {
            Argument::Expression(expr) => Ok(Box::new(expr) as _),
            Argument::Regex(_) => Err(Error::ArgumentExprRegex),
            Argument::Array(_) => Err(Error::ArgumentExprArray),
        }
    }
}

impl TryFrom<Argument> for regex::Regex {
    type Error = Error;

    fn try_from(arg: Argument) -> std::result::Result<Self, Self::Error> {
        match arg {
            Argument::Regex(regex) => Ok(regex),
            Argument::Expression(_) => Err(Error::ArgumentRegexExpr),
            Argument::Array(_) => Err(Error::ArgumentRegexArray),
        }
    }
}

impl TryFrom<Argument> for Vec<Argument> {
    type Error = Error;

    fn try_from(arg: Argument) -> std::result::Result<Self, Self::Error> {
        match arg {
            Argument::Array(args) => Ok(args),
            Argument::Regex(_) => Err(Error::ArgumentArrayRegex),
            Argument::Expression(_) => Err(Error::ArgumentArrayExpr),
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
