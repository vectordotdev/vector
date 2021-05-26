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
                ast::Literal::Float(NotNan::new(value).expect("should be a float"))
            }
            ComparisonValue::Unbounded => ast::Literal::Null,
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
            // Match everything
            QueryNode::MatchAllDocs => make_function_call(
                "exists",
                vec![make_query(grammar::DEFAULT_FIELD.to_owned())],
            ),
            // Matching nothing
            QueryNode::MatchNoDocs => make_not(make_function_call(
                "exists",
                vec![make_query(grammar::DEFAULT_FIELD.to_owned())],
            )),
            // Field existence
            QueryNode::AttributeExists { attr } => {
                make_function_call("exists", vec![make_query(attr)])
            }
            QueryNode::AttributeMissing { attr } => {
                make_not(make_function_call("exists", vec![make_query(attr)]))
            }
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
            } => make_op(make_node(make_query(attr)), comparator.into(), value.into()),
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
            } => match (&lower, &upper) {
                // If both bounds are wildcards, it'll match everything; just check the field exists.
                (ComparisonValue::Unbounded, ComparisonValue::Unbounded) => {
                    make_function_call("exists", vec![make_query(attr)])
                }
                (ComparisonValue::Unbounded, _) => make_op(
                    make_node(make_query(attr)),
                    if upper_inclusive {
                        ast::Opcode::Le
                    } else {
                        ast::Opcode::Lt
                    },
                    make_node(ast::Expr::Literal(make_node(upper.into()))),
                ),
                (_, ComparisonValue::Unbounded) => make_op(
                    make_node(make_query(attr)),
                    if lower_inclusive {
                        ast::Opcode::Ge
                    } else {
                        ast::Opcode::Gt
                    },
                    make_node(ast::Expr::Literal(make_node(lower.into()))),
                ),
                _ => make_op(
                    make_node(make_op(
                        make_node(make_query(attr.clone())),
                        if lower_inclusive {
                            ast::Opcode::Ge
                        } else {
                            ast::Opcode::Gt
                        },
                        make_node(ast::Expr::Literal(make_node(lower.into()))),
                    )),
                    ast::Opcode::And,
                    make_node(make_op(
                        make_node(make_query(attr)),
                        if upper_inclusive {
                            ast::Opcode::Le
                        } else {
                            ast::Opcode::Lt
                        },
                        make_node(ast::Expr::Literal(make_node(upper.into()))),
                    )),
                ),
            },
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

fn make_query(field: String) -> ast::Expr {
    ast::Expr::Query(make_node(ast::Query {
        target: make_node(ast::QueryTarget::External),
        path: make_node(
            lookup::parser::parse_lookup(&normalize_tag(field))
                .expect("should parse")
                .into(),
        ),
    }))
}

/// Makes a Regex string to be used with the `match`.
fn make_regex(value: String) -> ast::Expr {
    ast::Expr::Literal(make_node(ast::Literal::Regex(format!(
        "\\b{}\\b",
        regex::escape(&value).replace("\\*", ".*")
    ))))
}

/// Makes a container block of expressions.
fn make_block(exprs: Vec<ast::Node<ast::Expr>>) -> ast::Expr {
    ast::Expr::Container(make_node(ast::Container::Block(make_node(ast::Block(
        exprs,
    )))))
}

fn make_not(expr: ast::Expr) -> ast::Expr {
    ast::Expr::Unary(make_node(ast::Unary::Not(make_node(ast::Not(
        make_node(()),
        Box::new(make_node(expr)),
    )))))
}

/// A `Expr::FunctionCall` based on a tag and arguments.
fn make_function_call<T: IntoIterator<Item = ast::Expr>>(tag: &str, arguments: T) -> ast::Expr {
    ast::Expr::FunctionCall(make_node(ast::FunctionCall {
        ident: make_node(ast::Ident::new(tag.to_string())),
        abort_on_error: true,
        arguments: arguments
            .into_iter()
            .map(|expr| {
                make_node(ast::FunctionArgument {
                    ident: None,
                    expr: make_node(expr),
                })
            })
            .collect(),
    }))
}

#[cfg(test)]
mod tests {
    // Datadog search syntax -> VRL
    static TESTS: &[(&str, &str)] = &[
        // Match everything (empty)
        ("", "exists(.message)"),
        // Match everything
        ("*:*", "exists(.message)"),
        // Match nothing
        ("-*:*", "!exists(.message)"),
        // Tag exists
        ("_exists_:a", "exists(.a)"),
        // Facet exists
        ("_exists_:@b", "exists(.custom.b)"),
        // Tag doesn't exist
        ("_missing_:a", "!exists(.a)"),
        // Facet doesn't exist
        ("_missing_:@b", "!exists(.custom.b)"),
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
        // Range - numeric, inclusive
        ("[1 TO 10]", ".message >= 1 && .message <= 10"),
        // Range - numeric, inclusive, unbounded (upper)
        ("[50 TO *]", ".message >= 50"),
        // Range - numeric, inclusive, unbounded (lower)
        ("[* TO 50]", ".message <= 50"),
        // Range - numeric, inclusive, unbounded (both)
        ("[* TO *]", "exists(.message)"),
        // Range - numeric, inclusive, tag
        ("a:[1 TO 10]", ".a >= 1 && .a <= 10"),
        // Range - numeric, inclusive, unbounded (upper), tag
        ("a:[50 TO *]", ".a >= 50"),
        // Range - numeric, inclusive, unbounded (lower), tag
        ("a:[* TO 50]", ".a <= 50"),
        // Range - numeric, inclusive, unbounded (both), tag
        ("a:[* TO *]", "exists(.a)"),
        // Range - numeric, inclusive, facet
        ("@b:[1 TO 10]", ".custom.b >= 1 && .custom.b <= 10"),
        // Range - numeric, inclusive, unbounded (upper), facet
        ("@b:[50 TO *]", ".custom.b >= 50"),
        // Range - numeric, inclusive, unbounded (lower), facet
        ("@b:[* TO 50]", ".custom.b <= 50"),
        // Range - numeric, inclusive, unbounded (both), facet
        ("@b:[* TO *]", "exists(.custom.b)"),
        // TODO: CURRENTLY FAILING TESTS -- needs work in the main grammar and/or VRL to support!
        // Range - alpha, inclusive
        //(r#"["a" TO "z"]"#, r#".message >= "a" && .message <= "z""#),
        // Range - numeric, exclusive
        //("{1 TO 10}", ".message > 1 && .message < 10"),
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
