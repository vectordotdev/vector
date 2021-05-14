use lazy_static::lazy_static;
use regex::Regex;
use std::fmt::{Display, Formatter, Result};
use vrl_diagnostic::Span;
use vrl_parser::ast as v_ast;

lazy_static! {
    pub static ref TAG_REGEX: Regex = Regex::new(r"@(([a-zA-Z0-9]+.?)[a-zA-Z0-9]+)").unwrap();
}

#[derive(Debug)]
pub enum Expr {
    Tag(String),
    Value(String),
    Op(Box<Expr>, Opcode, Box<Expr>),
}

impl Expr {
    pub fn to_vrl(self) -> v_ast::Program {
        v_ast::Program(vec![make_node(v_ast::RootExpr::Expr(make_node(
            self.into(),
        )))])
    }
}

impl Display for Expr {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Self::Tag(v) => write!(f, "@{}", v),
            Self::Value(v) => write!(f, r#""{}""#, v),
            Self::Op(exp1, op, exp2) => write!(f, "{}{}{}", exp1, op, exp2),
        }
    }
}

impl From<Expr> for v_ast::Expr {
    fn from(expr: Expr) -> Self {
        match expr {
            Expr::Tag(t) => Self::Variable(make_node(v_ast::Ident::new(t))),
            Expr::Value(v) => Self::Literal(make_node(v_ast::Literal::String(v))),
            Expr::Op(exp1, op, exp2) => Self::Op(make_node(v_ast::Op(
                Box::new(make_node(v_ast::Expr::from(*exp1))),
                make_node(op.into()),
                Box::new(make_node(v_ast::Expr::from(*exp2))),
            ))),
        }
    }
}

#[derive(Debug)]
pub enum Opcode {
    Eq,
}

impl Display for Opcode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Self::Eq => write!(f, ":"),
        }
    }
}

impl From<Opcode> for v_ast::Opcode {
    fn from(op: Opcode) -> Self {
        match op {
            Opcode::Eq => Self::Eq,
        }
    }
}

fn make_node<T>(node: T) -> v_ast::Node<T> {
    v_ast::Node::new(Span::default(), node)
}
