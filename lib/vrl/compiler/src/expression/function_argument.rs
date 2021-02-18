use crate::expression::Expr;
use crate::parser::{Ident, Node};
use crate::{Parameter, Span};
use std::fmt;
use std::ops::Deref;

#[derive(Debug, PartialEq)]
pub struct FunctionArgument {
    ident: Option<Node<Ident>>,
    parameter: Option<Parameter>,
    expr: Node<Expr>,
}

impl FunctionArgument {
    pub(crate) fn new(ident: Option<Node<Ident>>, expr: Node<Expr>) -> Self {
        Self {
            ident,
            parameter: None,
            expr,
        }
    }

    pub(crate) fn keyword(&self) -> Option<&str> {
        self.ident.as_ref().map(|node| node.as_ref().as_ref())
    }

    pub(crate) fn keyword_span(&self) -> Option<Span> {
        self.ident.as_ref().map(|node| node.span())
    }

    pub(crate) fn parameter(&self) -> Option<Parameter> {
        self.parameter
    }

    pub(crate) fn into_inner(self) -> Expr {
        self.expr.into_inner()
    }
}

impl fmt::Display for FunctionArgument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.expr.fmt(f)
    }
}

impl Deref for FunctionArgument {
    type Target = Node<Expr>;

    fn deref(&self) -> &Self::Target {
        &self.expr
    }
}
