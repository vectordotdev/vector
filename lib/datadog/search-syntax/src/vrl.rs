use super::{
    grammar,
    node::{Comparison, ComparisonValue, QueryNode},
};
use ordered_float::NotNan;
use vrl_parser::ast::Opcode;
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
            } => make_function_call("match", vec![make_variable(attr), make_regex(value)]),
            // Comparison
            QueryNode::AttributeComparison {
                attr,
                comparator,
                value,
            } => make_op(make_variable(attr), comparator.into(), value.into()),
            // Wildcard suffix
            QueryNode::AttributePrefix { attr, prefix } => make_function_call(
                "match",
                vec![make_variable(attr), make_regex(format!("{}*", prefix))],
            ),
            // Arbitrary wildcard
            QueryNode::AttributeWildcard { attr, wildcard } => {
                make_function_call("match", vec![make_variable(attr), make_regex(wildcard)])
            }
            // Range
            QueryNode::AttributeRange {
                attr,
                lower,
                lower_inclusive,
                upper,
                upper_inclusive,
            } => make_block(vec![
                make_node(make_op(
                    make_variable(attr.clone()),
                    if lower_inclusive {
                        Opcode::Ge
                    } else {
                        Opcode::Gt
                    },
                    lower.into(),
                )),
                make_node(make_op(
                    make_variable(attr),
                    if upper_inclusive {
                        Opcode::Le
                    } else {
                        Opcode::Lt
                    },
                    upper.into(),
                )),
            ]),
            _ => panic!("unsupported query"),
        }
    }
}

/// Creates a VRL node with a default span
fn make_node<T>(node: T) -> ast::Node<T> {
    ast::Node::new(Span::default(), node)
}

/// Transforms a "tag" or "@tag" to the equivalent VRL field.
fn format_tag(value: String) -> String {
    // If the value matches the default, tagless field, this should be mapped to
    // `.message`, which is the VRL equivalent
    if value == grammar::DEFAULT_FIELD {
        return ".message".to_string();
    }

    // If the value starts with an "@", it's a Datadog facet type that's hosted on the
    // `.custom.*` field
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

/// An `Expr::Op` from two expressions, and a separating operator
fn make_op(expr1: ast::Node<ast::Expr>, op: Opcode, expr2: ast::Node<ast::Expr>) -> ast::Expr {
    ast::Expr::Op(make_node(ast::Op(
        Box::new(expr1),
        make_node(op),
        Box::new(expr2),
    )))
}

/// An `Expr::Variable` formatted as a tag.
fn make_variable(value: String) -> ast::Node<ast::Expr> {
    make_node(ast::Expr::Variable(make_tag(value)))
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
        // Vanilla keyword
        ("bla", r#"match(.message, r'\bbla\b')"#),
        // Quoted vanilla keyword
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
        ("[1 TO 10]", ".message > 1 && .message < 10"),
    ];

    use crate::parse;
    use vrl_parser::ast;

    #[test]
    fn to_vrl() {
        for (dd, vrl) in TESTS.iter() {
            let dd = parse(dd).expect(&format!("invalid Datadog search syntax: {}", dd));
            let vrl = vrl_parser::parse(vrl).expect(&format!("invalid VRL: {}", vrl));

            assert_eq!(ast::Expr::from(dd), *vrl);
        }
    }
}
