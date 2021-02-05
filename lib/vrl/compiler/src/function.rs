use crate::value::Kind;
use crate::{
    expression::{Expr, FunctionArgument},
    parser::Node,
    Expression,
};
use diagnostic::DiagnosticError;
use std::collections::HashMap;
use std::fmt;

pub type Compiled = Result<Box<dyn Expression>, Box<dyn DiagnosticError>>;

pub trait Function: Sync + fmt::Debug {
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
    fn compile(&self, arguments: ArgumentList) -> Compiled;

    /// An optional list of parameters the function accepts.
    ///
    /// This list is used at compile-time to check function arity, keyword names
    /// and argument type definition.
    fn parameters(&self) -> &'static [Parameter] {
        &[]
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Parameter {
    /// The keyword of the parameter.
    ///
    /// Arguments can be passed in both using the keyword, or as a positional
    /// argument.
    pub keyword: &'static str,

    /// The type kind(s) this parameter expects to receive.
    ///
    /// If an invalid kind is provided, the compiler will return a compile-time
    /// error.
    pub kind: u16,

    /// Whether or not this is a required parameter.
    ///
    /// If it isn't, the function can be called without errors, even if the
    /// argument matching this parameter is missing.
    pub required: bool,
}

impl Parameter {
    pub fn kind(&self) -> Kind {
        Kind::new(self.kind)
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct ArgumentList(HashMap<&'static str, Expr>);

impl ArgumentList {
    pub fn optional(&mut self, keyword: &str) -> Option<Box<dyn Expression>> {
        self.0.remove(keyword).map(|v| Box::new(v) as _)
    }

    pub fn required(&mut self, keyword: &str) -> Result<Box<dyn Expression>, Error> {
        self.optional(keyword)
            .ok_or_else(|| Error::Required(keyword.to_owned()).into())
    }

    pub(crate) fn keywords(&self) -> Vec<&'static str> {
        self.0.keys().copied().collect::<Vec<_>>()
    }

    pub(crate) fn insert(&mut self, k: &'static str, v: Expr) {
        self.0.insert(k, v);
    }
}

impl From<Vec<Node<FunctionArgument>>> for ArgumentList {
    fn from(arguments: Vec<Node<FunctionArgument>>) -> Self {
        let arguments = arguments
            .into_iter()
            .map(|arg| {
                let arg = arg.into_inner();
                // TODO: find a better API design that doesn't require unwrapping.
                let key = arg.parameter().expect("exists").keyword;
                let expr = arg.into_inner();

                (key, expr)
            })
            .collect::<HashMap<_, _>>();

        Self(arguments)
    }
}

// -----------------------------------------------------------------------------

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
    #[error(r#"missing required argument "{0}""#)]
    Required(String),
}

impl diagnostic::DiagnosticError for Error {}

impl From<Error> for Box<dyn diagnostic::DiagnosticError> {
    fn from(error: Error) -> Self {
        Box::new(error) as _
    }
}
