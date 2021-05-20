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

impl From<ComparisonValue> for ast::Node<ast::Expr> {
    fn from(cv: ComparisonValue) -> Self {
        make_node(ast::Expr::Literal(make_node(cv.into())))
    }
}

impl From<QueryNode> for ast::Expr {
    fn from(q: QueryNode) -> Self {
        match q {
            // Equality
            QueryNode::AttributeTerm { attr, value }
            | QueryNode::QuotedAttribute {
                attr,
                phrase: value,
            } => Self::Op(make_node(ast::Op(
                Box::new(make_variable(attr)),
                make_node(ast::Opcode::Eq),
                Box::new(make_value(value)),
            ))),
            // Comparison
            QueryNode::AttributeComparison {
                attr,
                comparator,
                value,
            } => Self::Op(make_node(ast::Op(
                Box::new(make_variable(attr)),
                make_node(comparator.into()),
                Box::new(value.into()),
            ))),
            // Wildcard suffix
            QueryNode::AttributePrefix { attr, prefix } => {
                make_function_call("starts_with", vec![make_variable(attr), make_regex(prefix)])
            }
            // Arbitrary wildcard
            QueryNode::AttributeWildcard { attr, wildcard } => {
                make_function_call("match", vec![make_variable(attr), make_regex(wildcard)])
            }
            _ => panic!("at the disco"),
        }
    }
}

/// Creates a VRL node with a default span
fn make_node<T>(node: T) -> ast::Node<T> {
    ast::Node::new(Span::default(), node)
}

/// Transforms a "tag" or "@tag" to the equivalent VRL field.
fn format_tag(value: String) -> String {
    if value.starts_with("@") {
        format!(".custom.{}", &value[1..])
    } else {
        format!(".{}", value)
    }
}

/// A tag is an `ast::Ident` formatted to point to a formatted VRL field.
fn make_tag(value: String) -> ast::Node<ast::Ident> {
    make_node(ast::Ident::new(format_tag(value)))
}

/// An `Expr::Variable` formatted as a tag.
fn make_variable(value: String) -> ast::Node<ast::Expr> {
    make_node(ast::Expr::Variable(make_tag(value)))
}

/// A `Expr::Literal` string literal value.
fn make_value(value: String) -> ast::Node<ast::Expr> {
    make_node(ast::Expr::Literal(make_node(ast::Literal::String(value))))
}

/// Makes a Regex string to be used with the `match`
fn make_regex(value: String) -> ast::Node<ast::Expr> {
    make_node(ast::Expr::Literal(make_node(ast::Literal::Regex(format!(
        "^{}$",
        value.replace("*", ".*")
    )))))
}

/// A `Expr::FunctionCall` based on a tag and arguments.
fn make_function_call<T: IntoIterator<Item = ast::Node<ast::Expr>>>(
    tag: &str,
    arguments: T,
) -> ast::Expr {
    ast::Expr::FunctionCall(make_node(ast::FunctionCall {
        ident: make_node(ast::Ident::new(tag.to_string())),
        abort_on_error: true,
        arguments: arguments
            .into_iter()
            .map(|expr| make_node(ast::FunctionArgument { ident: None, expr }))
            .collect(),
    }))
}
