use diagnostic::Span;
use std::fmt;

use crate::ast::{Expr, Ident, Literal::RawString, Node, Op, Opcode};

#[derive(Clone, PartialEq, Eq, Ord, PartialOrd, Debug, Hash)]
pub struct TemplateString(pub Vec<StringSegment>);

impl TemplateString {
    /// Rewrites the ast for the template string to be a series of string concatenations
    pub fn rewrite(&self) -> Expr {
        self.0
            .iter()
            .map(|node| -> Expr {
                match node {
                    StringSegment::Literal(s) => {
                        Expr::Literal(Node::new(diagnostic::Span::default(), RawString(s.clone())))
                    }
                    StringSegment::Template(s, span) => {
                        Expr::Variable(Node::new(*span, Ident::new(s)))
                    }
                }
            })
            .reduce(|accum, item| {
                Expr::Op(Node::new(
                    diagnostic::Span::default(),
                    Op(
                        Box::new(Node::new(diagnostic::Span::default(), accum)),
                        Node::new(diagnostic::Span::default(), Opcode::Add),
                        Box::new(Node::new(diagnostic::Span::default(), item)),
                    ),
                ))
            })
            .unwrap_or_else(|| {
                Expr::Literal(Node::new(
                    diagnostic::Span::default(),
                    RawString("".to_string()),
                ))
            })
    }

    /// If the template string is just a single literal string return that string
    /// as we can just represent it in the ast as a single literal, otherwise return
    /// None as we will need to rewrite it into an expression.
    pub fn literal_string(&self) -> Option<String> {
        match self.0.as_slice() {
            [StringSegment::Literal(s)] => Some(s.clone()),
            _ => None,
        }
    }
}

impl fmt::Display for TemplateString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for segment in &self.0 {
            segment.fmt(f)?;
        }

        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, Ord, PartialOrd, Debug, Hash)]
pub enum StringSegment {
    Literal(String),
    Template(String, Span),
}

impl fmt::Display for StringSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StringSegment::Literal(s) => write!(f, "{}", s),
            StringSegment::Template(s, _) => write!(f, "{}", s),
        }
    }
}
