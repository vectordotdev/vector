use super::node::{Comparison, ComparisonValue, QueryNode};
use ordered_float::NotNan;
use vrl_parser::{ast, Span};

impl From<Comparison> for ast::Opcode {
    fn from(c: Comparison) -> Self {
        match c {
            Comparison::GT => ast::Opcode::Gt,
            Comparison::LT => ast::Opcode::Lt,
            Comparison::GTE => ast::Opcode::Ge,
            Comparison::LTE => ast::Opcode::Le,
        }
    }
}

impl From<ComparisonValue> for ast::Literal {
    fn from(cv: ComparisonValue) -> Self {
        match cv {
            ComparisonValue::String(value) => ast::Literal::String(value),
            ComparisonValue::Numeric(value) => {
                ast::Literal::Float(NotNan::new(value).expect("should be float"))
            }
            _ => panic!("at the disco"),
        }
    }
}

impl From<QueryNode> for ast::Expr {
    fn from(q: QueryNode) -> Self {
        match q {
            QueryNode::AttributeTerm { attr, value } => Self::Op(make_node(ast::Op(
                Box::new(make_node(ast::Expr::Variable(make_node(ast::Ident::new(
                    attr,
                ))))),
                make_node(ast::Opcode::Eq),
                Box::new(make_node(ast::Expr::Literal(make_node(
                    ast::Literal::String(value),
                )))),
            ))),
            QueryNode::AttributeComparison {
                attr,
                comparator,
                value,
            } => Self::Op(make_node(ast::Op(
                Box::new(make_node(ast::Expr::Variable(make_node(ast::Ident::new(
                    attr,
                ))))),
                make_node(comparator.into()),
                Box::new(make_node(ast::Expr::Literal(make_node(value.into())))),
            ))),
            _ => panic!("at the disco"),
        }
    }
}

/// Helper function to make a VRL node
fn make_node<T>(node: T) -> ast::Node<T> {
    ast::Node::new(Span::default(), node)
}
