use std::fmt;

use diagnostic::Span;

use crate::ast::{Expr, Ident, Literal::RawString, Node, Op, Opcode};

#[derive(Clone, PartialEq, Eq, Ord, PartialOrd, Debug, Hash)]
pub enum StringSegment {
    Literal(String, Span),
    Template(String, Span),
}

impl fmt::Display for StringSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StringSegment::Literal(s, _) => write!(f, "{s}"),
            StringSegment::Template(s, _) => write!(f, "{{{{ {s} }}}}"),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Ord, PartialOrd, Debug, Hash)]
pub struct TemplateString(pub Vec<StringSegment>);

impl TemplateString {
    /// Rewrites the ast for the template string to be a series of string concatenations
    pub fn rewrite_to_concatenated_strings(&self) -> Expr {
        self.0
            .iter()
            .map(|node| match node {
                StringSegment::Literal(s, span) => {
                    (*span, Expr::Literal(Node::new(*span, RawString(s.clone()))))
                }
                StringSegment::Template(s, span) => {
                    (*span, Expr::Variable(Node::new(*span, Ident::new(s))))
                }
            })
            .reduce(|accum, item| {
                let (item_span, item) = item;
                let (accum_span, accum) = accum;
                let total_span = Span::new(accum_span.start(), item_span.end());
                (
                    total_span,
                    Expr::Op(Node::new(
                        total_span,
                        Op(
                            Box::new(Node::new(accum_span, accum)),
                            Node::new(item_span, Opcode::Add),
                            Box::new(Node::new(item_span, item)),
                        ),
                    )),
                )
            })
            .map_or_else(
                || {
                    Expr::Literal(Node::new(
                        diagnostic::Span::default(),
                        RawString(String::new()),
                    ))
                },
                |(_span, expr)| expr,
            )
    }

    /// If the template string is just a single literal string return that string
    /// as we can just represent it in the ast as a single literal, otherwise return
    /// None as we will need to rewrite it into an expression.
    pub fn as_literal_string(&self) -> Option<&str> {
        match self.0.as_slice() {
            [StringSegment::Literal(s, _)] => Some(s),
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
