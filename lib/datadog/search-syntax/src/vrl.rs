use super::{
    grammar,
    node::{Comparison, ComparisonValue, QueryNode},
};
use ordered_float::NotNan;
use vrl_parser::{
    ast::{self, Opcode},
    Span,
};

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
            ComparisonValue::String(value) => value
                .parse::<i64>()
                .map(|num| ast::Literal::Integer(num))
                .unwrap_or_else(|_| ast::Literal::String(value)),

            ComparisonValue::Numeric(value) => {
                ast::Literal::Float(NotNan::new(value).expect("should be float"))
            }
            _ => panic!("unknown comparision value"),
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
            } => make_function_call("match", vec![make_query(attr), make_regex(value)]),
            // Comparison
            QueryNode::AttributeComparison {
                attr,
                comparator,
                value,
            } => make_op(make_query(attr), comparator.into(), value.into()),
            // Wildcard suffix
            QueryNode::AttributePrefix { attr, prefix } => make_function_call(
                "match",
                vec![make_query(attr), make_regex(format!("{}*", prefix))],
            ),
            // Arbitrary wildcard
            QueryNode::AttributeWildcard { attr, wildcard } => {
                make_function_call("match", vec![make_query(attr), make_regex(wildcard)])
            }
            // Range
            QueryNode::AttributeRange {
                attr,
                lower,
                lower_inclusive,
                upper,
                upper_inclusive,
            } => make_op(
                make_node(make_op(
                    make_query(attr.clone()),
                    if lower_inclusive {
                        ast::Opcode::Ge
                    } else {
                        ast::Opcode::Gt
                    },
                    make_node(ast::Expr::Literal(make_node(lower.into()))),
                )),
                ast::Opcode::And,
                make_node(make_op(
                    make_query(attr),
                    if upper_inclusive {
                        ast::Opcode::Le
                    } else {
                        ast::Opcode::Lt
                    },
                    make_node(ast::Expr::Literal(make_node(upper.into()))),
                )),
            ),
            _ => panic!("unsupported query"),
        }
    }
}

/// Creates a VRL node with a default span
fn make_node<T>(node: T) -> ast::Node<T> {
    ast::Node::new(Span::default(), node)
}

fn normalize_tag<T: AsRef<str>>(value: T) -> String {
    let value = value.as_ref();
    if value.eq(grammar::DEFAULT_FIELD) {
        return "message".to_string();
    }

    value.replace("@", "custom.")
}

/// An `Expr::Op` from two expressions, and a separating operator
fn make_op(expr1: ast::Node<ast::Expr>, op: Opcode, expr2: ast::Node<ast::Expr>) -> ast::Expr {
    ast::Expr::Op(make_node(ast::Op(
        Box::new(expr1),
        make_node(op),
        Box::new(expr2),
    )))
}

fn make_query(field: String) -> ast::Node<ast::Expr> {
    make_node(ast::Expr::Query(make_node(ast::Query {
        target: make_node(ast::QueryTarget::External),
        path: make_node(
            lookup::parser::parse_lookup(&normalize_tag(field))
                .expect("should parse")
                .into(),
        ),
    })))
}

/// Makes a Regex string to be used with the `match`.
fn make_regex(value: String) -> ast::Node<ast::Expr> {
    make_node(ast::Expr::Literal(make_node(ast::Literal::Regex(format!(
        "\\b{}\\b",
        regex::escape(&value).replace("\\*", ".*")
    )))))
}

/// Makes a container block of expressions.
fn make_block(exprs: Vec<ast::Node<ast::Expr>>) -> ast::Expr {
    ast::Expr::Container(make_node(ast::Container::Block(make_node(ast::Block(
        exprs,
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

#[cfg(test)]
mod tests {
    // Datadog search syntax -> VRL
    static TESTS: &[(&str, &str)] = &[
        // Keyword
        ("bla", r#"match(.message, r'\bbla\b')"#),
        // Quoted keyword
        (r#""bla""#, r#"match(.message, r'\bbla\b')"#),
        // Tag match
        ("a:bla", r#"match(.a, r'\bbla\b')"#),
        // Quoted tag match
        (r#"a:"bla""#, r#"match(.a, r'\bbla\b')"#),
        // Facet match
        ("@a:bla", r#"match(.custom.a, r'\bbla\b')"#),
        // Quoted facet match
        (r#"@a:"bla""#, r#"match(.custom.a, r'\bbla\b')"#),
        // Wildcard prefix
        ("*bla", r#"match(.message, r'\b.*bla\b')"#),
        // Wildcard suffix
        ("bla*", r#"match(.message, r'\bbla.*\b')"#),
        // Multiple wildcards
        ("*b*la*", r#"match(.message, r'\b.*b.*la.*\b')"#),
        // Wildcard prefix - tag
        ("a:*bla", r#"match(.a, r'\b.*bla\b')"#),
        // Wildcard suffix - tag
        ("b:bla*", r#"match(.b, r'\bbla.*\b')"#),
        // Multiple wildcards - tag
        ("c:*b*la*", r#"match(.c, r'\b.*b.*la.*\b')"#),
        // Wildcard prefix - facet
        ("@a:*bla", r#"match(.custom.a, r'\b.*bla\b')"#),
        // Wildcard suffix - facet
        ("@b:bla*", r#"match(.custom.b, r'\bbla.*\b')"#),
        // Multiple wildcards - facet
        ("@c:*b*la*", r#"match(.custom.c, r'\b.*b.*la.*\b')"#),
        // Range - numeric, exclusive
        ("[1 TO 10]", ".message >= 1 && .message <= 10"),
    ];

    use super::make_node;
    use crate::parse;
    use vrl_parser::ast;

    #[test]
    /// Compile each Datadog search query -> VRL, and do the same with the equivalent direct
    /// VRL syntax, and then compare the results.
    fn to_vrl() {
        for (dd, vrl) in TESTS.iter() {
            let node = parse(dd).expect(&format!("invalid Datadog search syntax: {}", dd));
            let root = ast::RootExpr::Expr(make_node(ast::Expr::from(node)));

            let program = vrl_parser::parse(vrl).expect(&format!("invalid VRL: {}", vrl));

            assert_eq!(
                format!("{:?}", vec![make_node(root)]),
                format!("{:?}", program.0),
                "Failed: DD= {}, VRL= {}",
                dd,
                vrl
            );
        }
    }
}
