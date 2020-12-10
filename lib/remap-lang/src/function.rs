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
pub struct ArgumentList(HashMap<&'static str, Expr>);

impl ArgumentList {
    pub fn optional(&mut self, keyword: &str) -> Option<Expr> {
        self.0.remove(keyword)
    }

    pub fn required(&mut self, keyword: &str) -> Result<Expr> {
        self.optional(keyword)
            .ok_or_else(|| Error::Required(keyword.to_owned()).into())
    }

    pub fn optional_regex(&mut self, keyword: &str) -> Result<Option<regex::Regex>> {
        self.optional(keyword)
            .map(Literal::try_from)
            .transpose()?
            .map(|v| v.into_value().try_regex().map_err(Into::into))
            .transpose()
    }

    pub fn required_regex(&mut self, keyword: &str) -> Result<regex::Regex> {
        self.optional_regex(keyword)?
            .ok_or_else(|| Error::Required(keyword.to_owned()).into())
    }

    pub fn optional_literal(&mut self, keyword: &str) -> Result<Option<Literal>> {
        let argument = match self.optional(keyword) {
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
        self.optional(keyword)
            .map(Path::try_from)
            .transpose()
            .map_err(Into::into)
    }

    pub fn required_path(&mut self, keyword: &str) -> Result<Path> {
        self.optional_path(keyword)?
            .ok_or_else(|| Error::Required(keyword.to_owned()).into())
    }

    pub fn optional_array(&mut self, keyword: &str) -> Result<Option<Array>> {
        self.optional(keyword)
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

    pub fn keywords(&self) -> Vec<&'static str> {
        self.0.keys().copied().collect::<Vec<_>>()
    }

    pub fn insert(&mut self, k: &'static str, v: Expr) {
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
