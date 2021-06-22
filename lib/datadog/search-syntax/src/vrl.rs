use super::{
    field::{normalize_fields, Field},
    node::{BooleanType, Comparison, ComparisonValue, QueryNode},
};
use ordered_float::NotNan;
use vrl_parser::{
    ast::{self, Opcode},
    Span,
};

impl From<&BooleanType> for ast::Opcode {
    fn from(b: &BooleanType) -> Self {
        match b {
            BooleanType::And => ast::Opcode::And,
            BooleanType::Or => ast::Opcode::Or,
        }
    }
}

impl From<&Comparison> for ast::Opcode {
    fn from(c: &Comparison) -> Self {
        match c {
            Comparison::Gt => ast::Opcode::Gt,
            Comparison::Lt => ast::Opcode::Lt,
            Comparison::Gte => ast::Opcode::Ge,
            Comparison::Lte => ast::Opcode::Le,
        }
    }
}

impl From<ComparisonValue> for ast::Literal {
    fn from(cv: ComparisonValue) -> Self {
        match cv {
            ComparisonValue::String(value) => ast::Literal::String(value),
            ComparisonValue::Integer(value) => ast::Literal::Integer(value),
            ComparisonValue::Float(value) => {
                ast::Literal::Float(NotNan::new(value).expect("should be a float"))
            }
            ComparisonValue::Unbounded => panic!("unbounded values have no equivalent literal"),
        }
    }
}

/// Wrapper for a comparison value to be converted to a literal, with wrapped nodes.
impl From<ComparisonValue> for ast::Node<ast::Expr> {
    fn from(cv: ComparisonValue) -> Self {
        make_node(ast::Expr::Literal(make_node(cv.into())))
    }
}

/// Creates a VRL node with a default span.
pub(super) fn make_node<T>(node: T) -> ast::Node<T> {
    ast::Node::new(Span::default(), node)
}

/// An `Expr::Op` from two expressions, and a separating operator.
fn make_op(expr1: ast::Node<ast::Expr>, op: Opcode, expr2: ast::Node<ast::Expr>) -> ast::Expr {
    ast::Expr::Op(make_node(ast::Op(
        Box::new(expr1),
        make_node(op),
        Box::new(expr2),
    )))
}

/// An `Expr::Query`, converting a string field to a lookup path.
fn make_queries<T: AsRef<str>>(field: T) -> Vec<(Field, ast::Expr)> {
    normalize_fields(field)
        .into_iter()
        .map(|field| {
            let query = ast::Expr::Query(make_node(ast::Query {
                target: make_node(ast::QueryTarget::External),
                path: make_node(
                    lookup::parser::parse_lookup(field.as_str())
                        .expect("should parse lookup")
                        .into(),
                ),
            }));

            (field, query)
        })
        .collect()
}

/// Makes a Regex string to be used with the `match` function for word boundary matching.
fn make_word_regex<T: AsRef<str>>(value: T) -> ast::Expr {
    ast::Expr::Literal(make_node(ast::Literal::Regex(format!(
        "\\b{}\\b",
        regex::escape(value.as_ref()).replace("\\*", ".*")
    ))))
}

/// Makes a Regex string to be used with the `match` function for arbitrary wildcard matching
fn make_wildcard_regex<T: AsRef<str>>(value: T) -> ast::Expr {
    ast::Expr::Literal(make_node(ast::Literal::Regex(format!(
        "^{}$",
        regex::escape(value.as_ref()).replace("\\*", ".*")
    ))))
}

/// Makes a string comparison expression.
fn make_string_comparison<T: AsRef<str>>(expr: ast::Expr, op: Opcode, value: T) -> ast::Expr {
    make_op(
        make_node(expr),
        op,
        make_node(ast::Expr::Literal(make_node(ast::Literal::String(
            String::from(value.as_ref()),
        )))),
    )
}

/// Makes a string literal.
fn make_string<T: AsRef<str>>(value: T) -> ast::Expr {
    ast::Expr::Literal(make_node(ast::Literal::String(value.as_ref().to_owned())))
}

/// Makes a boolean literal.
fn make_bool(value: bool) -> ast::Expr {
    ast::Expr::Literal(make_node(ast::Literal::Boolean(value)))
}

/// Makes a container group, for wrapping logic for easier negation.
pub(super) fn make_container_group(expr: ast::Expr) -> ast::Expr {
    ast::Expr::Container(make_node(ast::Container::Group(Box::new(make_node(
        ast::Group(make_node(expr)),
    )))))
}

/// Makes a negation wrapper for an inner expression.
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
        abort_on_error: false,
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

/// Makes a literal expression from something that converts to an `ast::Literal`.
fn make_literal<T: Into<ast::Literal>>(literal: T) -> ast::Expr {
    ast::Expr::Literal(make_node(literal.into()))
}

/// Makes a field expression that contains a runtime check on the field type where the field
/// type is a facet or a non-`tags` reserved field.
fn make_field_op<T: Into<ast::Literal> + std::fmt::Display + Clone>(
    field: Field,
    query: ast::Expr,
    op: ast::Opcode,
    value: T,
) -> ast::Expr {
    // Facets and non-`tags` reserved fields operate on numerals if the field type is float
    // or integer. Otherwise, they're treated as strings.
    match field {
        Field::Facet(f) | Field::Reserved(f) if f != "tags" => {
            // Check that the number is either an integer or a float.
            let num_check = make_container_group(make_op(
                make_node(make_function_call("is_integer", vec![query.clone()])),
                ast::Opcode::Or,
                make_node(make_function_call("is_float", vec![query.clone()])),
            ));

            // If we're dealing with a number, the range comparison should be numberic.
            let num_eq = make_op(
                make_node(query.clone()),
                op,
                make_node(make_literal(value.clone())),
            );

            // Final number expression, including int/float and range check.
            let num_expr = make_container_group(make_op(
                make_node(num_check),
                ast::Opcode::And,
                make_node(num_eq),
            ));

            // String comparison fallback.
            let string_expr = make_string_comparison(query, op, value.to_string());

            // Wire up the expressions, separated by `||`.
            recurse_op(vec![num_expr, string_expr].into_iter(), ast::Opcode::Or)
        }
        // If the field type doesn't support numeric operations, just compare by string.
        _ => make_string_comparison(query, op, value.to_string()),
    }
}

/// Recursive, nested expressions, ultimately returning a single `ast::Expr`.
fn recurse_op<I: ExactSizeIterator<Item = impl Into<ast::Expr>>, O: Into<ast::Opcode>>(
    mut exprs: I,
    op: O,
) -> ast::Expr {
    let expr = exprs.next().expect("must contain expression").into();
    let op = op.into();

    match exprs.len() {
        // If this is the last expression, just return it.
        0 => expr,
        // If there's one expression remaining, use it as the RHS; no need to wrap.
        1 => make_container_group(make_op(
            make_node(expr),
            op,
            make_node(exprs.next().expect("must contain expression").into()),
        )),
        // For 2+ expressions, recurse over the RHS, and wrap in a container group for atomicity.
        _ => make_container_group(make_op(
            make_node(expr),
            op,
            make_node(recurse_op(exprs, op)),
        )),
    }
}

/// Default recursion, using the `OR` operator.
fn recurse<I: ExactSizeIterator<Item = impl Into<ast::Expr>>>(exprs: I) -> ast::Expr {
    recurse_op(exprs, ast::Opcode::Or)
}

/// Coalesces an expression to <query> ?? false to avoid fallible states.
fn coalesce<T: Into<ast::Expr>>(expr: T) -> ast::Expr {
    make_container_group(make_op(
        make_node(expr.into()),
        Opcode::Err,
        make_node(ast::Expr::Literal(make_node(ast::Literal::Boolean(false)))),
    ))
}

/// Parse the provided Datadog `QueryNode`. This will return a vector of VRL expressions,
/// in order to accommodate expansion to multiple fields where relevant.
fn parse_node(node: &QueryNode) -> Vec<ast::Expr> {
    match node {
        // Match everything.
        QueryNode::MatchAllDocs => vec![make_bool(true)],
        // Matching nothing.
        QueryNode::MatchNoDocs => vec![make_bool(false)],
        // Field existence.
        QueryNode::AttributeExists { attr } => make_queries(attr)
            .into_iter()
            .map(|(_, query)| make_function_call("exists", vec![query]))
            .collect(),
        QueryNode::AttributeMissing { attr } => make_queries(attr)
            .into_iter()
            .map(|(_, query)| make_not(make_function_call("exists", vec![query])))
            .collect(),
        // Equality.
        QueryNode::AttributeTerm { attr, value }
        | QueryNode::QuotedAttribute {
            attr,
            phrase: value,
        } => make_queries(attr)
            .into_iter()
            .map(|(field, query)| match field {
                Field::Default(_) => coalesce(make_function_call(
                    "match",
                    vec![query, make_word_regex(&value)],
                )),
                // Special case for tags, which should be an array.
                Field::Reserved(f) if f == "tags" => coalesce(make_function_call(
                    "includes",
                    vec![query, make_string(value)],
                )),
                _ => make_string_comparison(query, ast::Opcode::Eq, &value),
            })
            .collect(),
        // Comparison.
        QueryNode::AttributeComparison {
            attr,
            comparator,
            value,
        } => make_queries(attr)
            .into_iter()
            .map(|(_, query)| make_op(make_node(query), comparator.into(), value.clone().into()))
            .collect(),
        // Wildcard suffix.
        QueryNode::AttributePrefix { attr, prefix } => make_queries(attr)
            .into_iter()
            .map(|(field, query)| match field {
                Field::Default(_) => coalesce(make_function_call(
                    "match",
                    vec![query, make_word_regex(&format!("{}*", &prefix))],
                )),
                _ => coalesce(make_function_call(
                    "starts_with",
                    vec![query, make_string(prefix)],
                )),
            })
            .collect(),
        // Arbitrary wildcard.
        QueryNode::AttributeWildcard { attr, wildcard } => make_queries(attr)
            .into_iter()
            .map(|(field, query)| {
                match field {
                    // Default fields use word boundary matching.
                    Field::Default(_) => coalesce(make_function_call(
                        "match",
                        vec![query, make_word_regex(&wildcard)],
                    )),
                    // If there's only one `*` and it's at the beginning, `ends_with` is faster.
                    _ if wildcard.starts_with('*') && wildcard.matches('*').count() == 1 => {
                        coalesce(make_function_call(
                            "ends_with",
                            vec![query, make_string(wildcard.replace('*', ""))],
                        ))
                    }
                    // Otherwise, default to non word boundary matching.
                    _ => coalesce(make_function_call(
                        "match",
                        vec![query, make_wildcard_regex(&wildcard)],
                    )),
                }
            })
            .collect(),
        // Range.
        QueryNode::AttributeRange {
            attr,
            lower,
            lower_inclusive,
            upper,
            upper_inclusive,
        } => make_queries(&attr)
            .into_iter()
            .map(|(field, query)| {
                match (lower, upper) {
                    // If both bounds are wildcards, it'll match everything; just check the field exists.
                    (ComparisonValue::Unbounded, ComparisonValue::Unbounded) => {
                        make_function_call("exists", vec![query])
                    }
                    // Unbounded lower. Wrapped in a container group for negation compatibility.
                    (ComparisonValue::Unbounded, _) => {
                        let op = if *upper_inclusive {
                            ast::Opcode::Le
                        } else {
                            ast::Opcode::Lt
                        };

                        coalesce(make_field_op(field, query, op, upper.clone()))
                    }
                    // Unbounded upper. Wrapped in a container group for negation compatibility.
                    (_, ComparisonValue::Unbounded) => {
                        let op = if *lower_inclusive {
                            ast::Opcode::Ge
                        } else {
                            ast::Opcode::Gt
                        };

                        coalesce(make_field_op(field, query, op, lower.clone()))
                    }
                    // Definitive range.
                    _ => {
                        let lower_op = if *lower_inclusive {
                            ast::Opcode::Ge
                        } else {
                            ast::Opcode::Gt
                        };

                        let upper_op = if *upper_inclusive {
                            ast::Opcode::Le
                        } else {
                            ast::Opcode::Lt
                        };

                        coalesce(make_container_group(make_op(
                            make_node(make_field_op(
                                field.clone(),
                                query.clone(),
                                lower_op,
                                lower.clone(),
                            )),
                            ast::Opcode::And,
                            make_node(make_field_op(field, query, upper_op, upper.clone())),
                        )))
                    }
                }
            })
            .collect(),
        // Negation. If the node is an operation type, wrap in a container before negating.
        QueryNode::NegatedNode { node } => {
            let expr = recurse(parse_node(node).into_iter());

            let node = match expr {
                ast::Expr::Op(_) => make_container_group(expr),
                _ => expr,
            };

            vec![make_not(node)]
        }
        // Compound.
        QueryNode::Boolean { oper, nodes } => {
            let exprs = nodes
                .iter()
                .map(|node| recurse(parse_node(node).into_iter()));

            vec![recurse_op(exprs, oper)]
        }
    }
}

/// Build a VRL expression from a `&QueryNode`. Will recurse through each leaf element
/// as required.
pub fn build(node: &QueryNode) -> ast::Expr {
    recurse(parse_node(&node).into_iter())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{compile, parse};
    use vrl_parser::ast;

    // Lhs = Datadog syntax. Rhs = VRL equivalent.
    static TESTS: &[(&str, &str)] = &[
        // Match everything (empty).
        ("", "true"),
        // Match everything.
        ("*:*", "true"),
        // Match everything (negate).
        ("NOT(*:*)", "false"),
        // Match nothing.
        ("-*:*", "false"),
        // Tag exists.
        ("_exists_:a", "exists(.__datadog_tags.a)"),
        // Tag exists (negate).
        ("NOT _exists_:a", "!exists(.__datadog_tags.a)"),
        // Tag exists (negate w/-).
        ("-_exists_:a", "!exists(.__datadog_tags.a)"),
        // Facet exists.
        ("_exists_:@b", "exists(.custom.b)"),
        // Facet exists (negate).
        ("NOT _exists_:@b", "!exists(.custom.b)"),
        // Facet exists (negate w/-).
        ("-_exists_:@b", "!exists(.custom.b)"),
        // Tag doesn't exist.
        ("_missing_:a", "!exists(.__datadog_tags.a)"),
        // Tag doesn't exist (negate).
        ("NOT _missing_:a", "!!exists(.__datadog_tags.a)"),
        // Tag doesn't exist (negate w/-).
        ("-_missing_:a", "!!exists(.__datadog_tags.a)"),
        // Facet doesn't exist.
        ("_missing_:@b", "!exists(.custom.b)"),
        // Facet doesn't exist (negate).
        ("NOT _missing_:@b", "!!exists(.custom.b)"),
        // Facet doesn't exist (negate w/-).
        ("-_missing_:@b", "!!exists(.custom.b)"),
        // Keyword.
        ("bla", r#"((match(.message, r'\bbla\b') ?? false) || ((match(.custom.error.message, r'\bbla\b') ?? false) || ((match(.custom.error.stack, r'\bbla\b') ?? false) || ((match(.custom.title, r'\bbla\b') ?? false) || (match(._default_, r'\bbla\b') ?? false)))))"#),
        // Keyword (negate).
        ("NOT bla", r#"!((match(.message, r'\bbla\b') ?? false) || ((match(.custom.error.message, r'\bbla\b') ?? false) || ((match(.custom.error.stack, r'\bbla\b') ?? false) || ((match(.custom.title, r'\bbla\b') ?? false) || (match(._default_, r'\bbla\b') ?? false)))))"#),
        // Keyword (negate w/-).
        ("-bla", r#"!((match(.message, r'\bbla\b') ?? false) || ((match(.custom.error.message, r'\bbla\b') ?? false) || ((match(.custom.error.stack, r'\bbla\b') ?? false) || ((match(.custom.title, r'\bbla\b') ?? false) || (match(._default_, r'\bbla\b') ?? false)))))"#),
        // Quoted keyword.
        (r#""bla""#, r#"((match(.message, r'\bbla\b') ?? false) || ((match(.custom.error.message, r'\bbla\b') ?? false) || ((match(.custom.error.stack, r'\bbla\b') ?? false) || ((match(.custom.title, r'\bbla\b') ?? false) || (match(._default_, r'\bbla\b') ?? false)))))"#),
        // Quoted keyword (negate).
        (r#"NOT "bla""#, r#"!((match(.message, r'\bbla\b') ?? false) || ((match(.custom.error.message, r'\bbla\b') ?? false) || ((match(.custom.error.stack, r'\bbla\b') ?? false) || ((match(.custom.title, r'\bbla\b') ?? false) || (match(._default_, r'\bbla\b') ?? false)))))"#),
        // Quoted keyword (negate w/-).
        (r#"-"bla""#, r#"!((match(.message, r'\bbla\b') ?? false) || ((match(.custom.error.message, r'\bbla\b') ?? false) || ((match(.custom.error.stack, r'\bbla\b') ?? false) || ((match(.custom.title, r'\bbla\b') ?? false) || (match(._default_, r'\bbla\b') ?? false)))))"#),
        // Tag match.
        ("a:bla", r#".__datadog_tags.a == "bla""#),
        // Reserved tag match.
        ("host:foo", r#".host == "foo""#),
        // Tag match (negate).
        ("NOT a:bla", r#"!(.__datadog_tags.a == "bla")"#),
        // Reserved tag match (negate).
        ("NOT host:foo", r#"!(.host == "foo")"#),
        // Tag match (negate w/-).
        ("-a:bla", r#"!(.__datadog_tags.a == "bla")"#),
        // Reserved tag match (negate w/-).
        ("-trace_id:foo", r#"!(.trace_id == "foo")"#),
        // Quoted tag match.
        (r#"a:"bla""#, r#".__datadog_tags.a == "bla""#),
        // Quoted tag match (negate).
        (r#"NOT a:"bla""#, r#"!(.__datadog_tags.a == "bla")"#),
        // Quoted tag match (negate w/-).
        (r#"-a:"bla""#, r#"!(.__datadog_tags.a == "bla")"#),
        // Facet match.
        ("@a:bla", r#".custom.a == "bla""#),
        // Facet match (negate).
        ("NOT @a:bla", r#"!(.custom.a == "bla")"#),
        // Facet match (negate w/-).
        ("-@a:bla", r#"!(.custom.a == "bla")"#),
        // Quoted facet match.
        (r#"@a:"bla""#, r#".custom.a == "bla""#),
        // Quoted facet match (negate).
        (r#"NOT @a:"bla""#, r#"!(.custom.a == "bla")"#),
        // Quoted facet match (negate w/-).
        (r#"-@a:"bla""#, r#"!(.custom.a == "bla")"#),
        // Wildcard prefix.
        ("*bla", r#"((match(.message, r'\b.*bla\b') ?? false) || ((match(.custom.error.message, r'\b.*bla\b') ?? false) || ((match(.custom.error.stack, r'\b.*bla\b') ?? false) || ((match(.custom.title, r'\b.*bla\b') ?? false) || (match(._default_, r'\b.*bla\b') ?? false)))))"#),
        // Wildcard prefix (negate).
        ("NOT *bla", r#"!((match(.message, r'\b.*bla\b') ?? false) || ((match(.custom.error.message, r'\b.*bla\b') ?? false) || ((match(.custom.error.stack, r'\b.*bla\b') ?? false) || ((match(.custom.title, r'\b.*bla\b') ?? false) || (match(._default_, r'\b.*bla\b') ?? false)))))"#),
        // Wildcard prefix (negate w/-).
        ("-*bla", r#"!((match(.message, r'\b.*bla\b') ?? false) || ((match(.custom.error.message, r'\b.*bla\b') ?? false) || ((match(.custom.error.stack, r'\b.*bla\b') ?? false) || ((match(.custom.title, r'\b.*bla\b') ?? false) || (match(._default_, r'\b.*bla\b') ?? false)))))"#),
        // Wildcard suffix.
        ("bla*", r#"((match(.message, r'\bbla.*\b') ?? false) || ((match(.custom.error.message, r'\bbla.*\b') ?? false) || ((match(.custom.error.stack, r'\bbla.*\b') ?? false) || ((match(.custom.title, r'\bbla.*\b') ?? false) || (match(._default_, r'\bbla.*\b') ?? false)))))"#),
        // Wildcard suffix (negate).
        ("NOT bla*", r#"!((match(.message, r'\bbla.*\b') ?? false) || ((match(.custom.error.message, r'\bbla.*\b') ?? false) || ((match(.custom.error.stack, r'\bbla.*\b') ?? false) || ((match(.custom.title, r'\bbla.*\b') ?? false) || (match(._default_, r'\bbla.*\b') ?? false)))))"#),
        // Wildcard suffix (negate w/-).
        ("-bla*", r#"!((match(.message, r'\bbla.*\b') ?? false) || ((match(.custom.error.message, r'\bbla.*\b') ?? false) || ((match(.custom.error.stack, r'\bbla.*\b') ?? false) || ((match(.custom.title, r'\bbla.*\b') ?? false) || (match(._default_, r'\bbla.*\b') ?? false)))))"#),
        // Multiple wildcards.
        ("*b*la*", r#"((match(.message, r'\b.*b.*la.*\b') ?? false) || ((match(.custom.error.message, r'\b.*b.*la.*\b') ?? false) || ((match(.custom.error.stack, r'\b.*b.*la.*\b') ?? false) || ((match(.custom.title, r'\b.*b.*la.*\b') ?? false) || (match(._default_, r'\b.*b.*la.*\b') ?? false)))))"#),
        // Multiple wildcards (negate).
        ("NOT *b*la*", r#"!((match(.message, r'\b.*b.*la.*\b') ?? false) || ((match(.custom.error.message, r'\b.*b.*la.*\b') ?? false) || ((match(.custom.error.stack, r'\b.*b.*la.*\b') ?? false) || ((match(.custom.title, r'\b.*b.*la.*\b') ?? false) || (match(._default_, r'\b.*b.*la.*\b') ?? false)))))"#),
        // Multiple wildcards (negate w/-).
        ("-*b*la*", r#"!((match(.message, r'\b.*b.*la.*\b') ?? false) || ((match(.custom.error.message, r'\b.*b.*la.*\b') ?? false) || ((match(.custom.error.stack, r'\b.*b.*la.*\b') ?? false) || ((match(.custom.title, r'\b.*b.*la.*\b') ?? false) || (match(._default_, r'\b.*b.*la.*\b') ?? false)))))"#),
        // Wildcard prefix - tag.
        ("a:*bla", r#"(ends_with(.__datadog_tags.a, "bla") ?? false)"#),
        // Wildcard prefix - tag (negate).
        ("NOT a:*bla", r#"!(ends_with(.__datadog_tags.a, "bla") ?? false)"#),
        // Wildcard prefix - tag (negate w/-).
        ("-a:*bla", r#"!(ends_with(.__datadog_tags.a, "bla") ?? false)"#),
        // Wildcard suffix - tag.
        ("b:bla*", r#"(starts_with(.__datadog_tags.b, "bla") ?? false)"#),
        // Wildcard suffix - tag (negate).
        ("NOT b:bla*", r#"!(starts_with(.__datadog_tags.b, "bla") ?? false)"#),
        // Wildcard suffix - tag (negate w/-).
        ("-b:bla*", r#"!(starts_with(.__datadog_tags.b, "bla") ?? false)"#),
        // Multiple wildcards - tag.
        ("c:*b*la*", r#"(match(.__datadog_tags.c, r'^.*b.*la.*$') ?? false)"#),
        // Multiple wildcards - tag (negate).
        ("NOT c:*b*la*", r#"!(match(.__datadog_tags.c, r'^.*b.*la.*$') ?? false)"#),
        // Multiple wildcards - tag (negate w/-).
        ("-c:*b*la*", r#"!(match(.__datadog_tags.c, r'^.*b.*la.*$') ?? false)"#),
        // Wildcard prefix - facet.
        ("@a:*bla", r#"(ends_with(.custom.a, "bla") ?? false)"#),
        // Wildcard prefix - facet (negate).
        ("NOT @a:*bla", r#"!(ends_with(.custom.a, "bla") ?? false)"#),
        // Wildcard prefix - facet (negate w/-).
        ("-@a:*bla", r#"!(ends_with(.custom.a, "bla") ?? false)"#),
        // Wildcard suffix - facet.
        ("@b:bla*", r#"(starts_with(.custom.b, "bla") ?? false)"#),
        // Wildcard suffix - facet (negate).
        ("NOT @b:bla*", r#"!(starts_with(.custom.b, "bla") ?? false)"#),
        // Wildcard suffix - facet (negate w/-).
        ("-@b:bla*", r#"!(starts_with(.custom.b, "bla") ?? false)"#),
        // Multiple wildcards - facet.
        ("@c:*b*la*", r#"(match(.custom.c, r'^.*b.*la.*$') ?? false)"#),
        // Multiple wildcards - facet (negate).
        ("NOT @c:*b*la*", r#"!(match(.custom.c, r'^.*b.*la.*$') ?? false)"#),
        // Multiple wildcards - facet (negate w/-).
        ("-@c:*b*la*", r#"!(match(.custom.c, r'^.*b.*la.*$') ?? false)"#),
        // Special case for tags.
        ("tags:a", r#"(includes(.tags, "a") ?? false)"#),
        // Special case for tags (negate).
        ("NOT tags:a", r#"!(includes(.tags, "a") ?? false)"#),
        // Special case for tags (negate w/-).
        ("-tags:a", r#"!(includes(.tags, "a") ?? false)"#),
        // Range - numeric, inclusive.
        ("[1 TO 10]", r#"(((.message >= "1" && .message <= "10") ?? false) || (((.custom.error.message >= "1" && .custom.error.message <= "10") ?? false) || (((.custom.error.stack >= "1" && .custom.error.stack <= "10") ?? false) || (((.custom.title >= "1" && .custom.title <= "10") ?? false) || ((._default_ >= "1" && ._default_ <= "10") ?? false)))))"#),
        // Range - numeric, inclusive (negate).
        ("NOT [1 TO 10]", r#"!(((.message >= "1" && .message <= "10") ?? false) || (((.custom.error.message >= "1" && .custom.error.message <= "10") ?? false) || (((.custom.error.stack >= "1" && .custom.error.stack <= "10") ?? false) || (((.custom.title >= "1" && .custom.title <= "10") ?? false) || ((._default_ >= "1" && ._default_ <= "10") ?? false)))))"#),
        // Range - numeric, inclusive (negate w/-).
        ("-[1 TO 10]", r#"!(((.message >= "1" && .message <= "10") ?? false) || (((.custom.error.message >= "1" && .custom.error.message <= "10") ?? false) || (((.custom.error.stack >= "1" && .custom.error.stack <= "10") ?? false) || (((.custom.title >= "1" && .custom.title <= "10") ?? false) || ((._default_ >= "1" && ._default_ <= "10") ?? false)))))"#),
        // Range - numeric, inclusive, unbounded (upper).
        ("[50 TO *]", r#"((.message >= "50" ?? false) || ((.custom.error.message >= "50" ?? false) || ((.custom.error.stack >= "50" ?? false) || ((.custom.title >= "50" ?? false) || (._default_ >= "50" ?? false)))))"#),
        // Range - numeric, inclusive, unbounded (upper) (negate).
        ("NOT [50 TO *]", r#"!((.message >= "50" ?? false) || ((.custom.error.message >= "50" ?? false) || ((.custom.error.stack >= "50" ?? false) || ((.custom.title >= "50" ?? false) || (._default_ >= "50" ?? false)))))"#),
        // Range - numeric, inclusive, unbounded (upper) (negate w/-).
        ("-[50 TO *]", r#"!((.message >= "50" ?? false) || ((.custom.error.message >= "50" ?? false) || ((.custom.error.stack >= "50" ?? false) || ((.custom.title >= "50" ?? false) || (._default_ >= "50" ?? false)))))"#),
        // Range - numeric, inclusive, unbounded (lower).
        ("[* TO 50]", r#"((.message <= "50" ?? false) || ((.custom.error.message <= "50" ?? false) || ((.custom.error.stack <= "50" ?? false) || ((.custom.title <= "50" ?? false) || (._default_ <= "50" ?? false)))))"#),
        // Range - numeric, inclusive, unbounded (lower) (negate).
        ("NOT [* TO 50]", r#"!((.message <= "50" ?? false) || ((.custom.error.message <= "50" ?? false) || ((.custom.error.stack <= "50" ?? false) || ((.custom.title <= "50" ?? false) || (._default_ <= "50" ?? false)))))"#),
        // Range - numeric, inclusive, unbounded (lower) (negate w/-).
        ("-[* TO 50]", r#"!((.message <= "50" ?? false) || ((.custom.error.message <= "50" ?? false) || ((.custom.error.stack <= "50" ?? false) || ((.custom.title <= "50" ?? false) || (._default_ <= "50" ?? false)))))"#),
        // Range - numeric, inclusive, unbounded (both).
        ("[* TO *]", "(exists(.message) || (exists(.custom.error.message) || (exists(.custom.error.stack) || (exists(.custom.title) || exists(._default_)))))"),
        // Range - numeric, inclusive, unbounded (both) (negate).
        ("NOT [* TO *]", "!(exists(.message) || (exists(.custom.error.message) || (exists(.custom.error.stack) || (exists(.custom.title) || exists(._default_)))))"),
        // Range - numeric, inclusive, unbounded (both) (negate w/-).
        ("-[* TO *]", "!(exists(.message) || (exists(.custom.error.message) || (exists(.custom.error.stack) || (exists(.custom.title) || exists(._default_)))))"),
        // Range - numeric, inclusive, tag.
        ("a:[1 TO 10]", r#"((.__datadog_tags.a >= "1" && .__datadog_tags.a <= "10") ?? false)"#),
        // Range - numeric, inclusive, tag (negate).
        ("NOT a:[1 TO 10]", r#"!((.__datadog_tags.a >= "1" && .__datadog_tags.a <= "10") ?? false)"#),
        // Range - numeric, inclusive, tag (negate w/-).
        ("-a:[1 TO 10]", r#"!((.__datadog_tags.a >= "1" && .__datadog_tags.a <= "10") ?? false)"#),
        // Range - numeric, inclusive, unbounded (upper), tag.
        ("a:[50 TO *]", r#"(.__datadog_tags.a >= "50" ?? false)"#),
        // Range - numeric, inclusive, unbounded (upper), tag (negate).
        ("NOT a:[50 TO *]", r#"!(.__datadog_tags.a >= "50" ?? false)"#),
        // Range - numeric, inclusive, unbounded (upper), tag (negate w/-).
        ("-a:[50 TO *]", r#"!(.__datadog_tags.a >= "50" ?? false)"#),
        // Range - numeric, inclusive, unbounded (lower), tag.
        ("a:[* TO 50]", r#"(.__datadog_tags.a <= "50" ?? false)"#),
        // Range - numeric, inclusive, unbounded (lower), tag (negate).
        ("NOT a:[* TO 50]", r#"!(.__datadog_tags.a <= "50" ?? false)"#),
        // Range - numeric, inclusive, unbounded (lower), tag (negate w/-).
        ("-a:[* TO 50]", r#"!(.__datadog_tags.a <= "50" ?? false)"#),
        // Range - numeric, inclusive, unbounded (both), tag.
        ("a:[* TO *]", "exists(.__datadog_tags.a)"),
        // Range - numeric, inclusive, unbounded (both), tag (negate).
        ("NOT a:[* TO *]", "!exists(.__datadog_tags.a)"),
        // Range - numeric, inclusive, unbounded (both), tag (negate).
        ("-a:[* TO *]", "!exists(.__datadog_tags.a)"),
        // Range - numeric, inclusive, facet.
        ("@b:[1 TO 10]", r#"(((((is_integer(.custom.b) || is_float(.custom.b)) && .custom.b >= 1) || .custom.b >= "1") && (((is_integer(.custom.b) || is_float(.custom.b)) && .custom.b <= 10) || .custom.b <= "10")) ?? false)"#),
        // Range - numeric, inclusive, facet (negate).
        ("NOT @b:[1 TO 10]", r#"!(((((is_integer(.custom.b) || is_float(.custom.b)) && .custom.b >= 1) || .custom.b >= "1") && (((is_integer(.custom.b) || is_float(.custom.b)) && .custom.b <= 10) || .custom.b <= "10")) ?? false)"#),
        // Range - numeric, inclusive, facet (negate w/-).
        ("-@b:[1 TO 10]", r#"!(((((is_integer(.custom.b) || is_float(.custom.b)) && .custom.b >= 1) || .custom.b >= "1") && (((is_integer(.custom.b) || is_float(.custom.b)) && .custom.b <= 10) || .custom.b <= "10")) ?? false)"#),
        // Range - numeric, inclusive, unbounded (upper), facet.
        ("@b:[50 TO *]", r#"((((is_integer(.custom.b) || is_float(.custom.b)) && .custom.b >= 50) || .custom.b >= "50") ?? false)"#),
        // Range - numeric, inclusive, unbounded (upper), facet (negate).
        ("NOT @b:[50 TO *]", r#"!((((is_integer(.custom.b) || is_float(.custom.b)) && .custom.b >= 50) || .custom.b >= "50") ?? false)"#),
        // Range - numeric, inclusive, unbounded (upper), facet (negate w/-).
        ("-@b:[50 TO *]", r#"!((((is_integer(.custom.b) || is_float(.custom.b)) && .custom.b >= 50) || .custom.b >= "50") ?? false)"#),
        // Range - numeric, inclusive, unbounded (lower), facet.
        ("@b:[* TO 50]", r#"((((is_integer(.custom.b) || is_float(.custom.b)) && .custom.b <= 50) || .custom.b <= "50") ?? false)"#),
        // Range - numeric, inclusive, unbounded (lower), facet (negate).
        ("NOT @b:[* TO 50]", r#"!((((is_integer(.custom.b) || is_float(.custom.b)) && .custom.b <= 50) || .custom.b <= "50") ?? false)"#),
        // Range - numeric, inclusive, unbounded (lower), facet (negate w/-).
        ("-@b:[* TO 50]", r#"!((((is_integer(.custom.b) || is_float(.custom.b)) && .custom.b <= 50) || .custom.b <= "50") ?? false)"#),
        // Range - numeric, inclusive, unbounded (both), facet.
        ("@b:[* TO *]", "exists(.custom.b)"),
        // Range - numeric, inclusive, unbounded (both), facet (negate).
        ("NOT @b:[* TO *]", "!exists(.custom.b)"),
        // Range - numeric, inclusive, unbounded (both), facet (negate w/-).
        ("-@b:[* TO *]", "!exists(.custom.b)"),
        // Range - tag, exclusive
        ("f:{1 TO 10}", r#"((.__datadog_tags.f > "1" && .__datadog_tags.f < "10") ?? false)"#),
        // Range - facet, exclusive
        ("@f:{1 TO 10}", r#"(((((is_integer(.custom.f) || is_float(.custom.f)) && .custom.f > 1) || .custom.f > "1") && (((is_integer(.custom.f) || is_float(.custom.f)) && .custom.f < 10) || .custom.f < "10")) ?? false)"#),
        // Range - alpha, inclusive
        (r#"g:[a TO z]"#, r#"((.__datadog_tags.g >= "a" && .__datadog_tags.g <= "z") ?? false)"#),
        // Range - alpha, exclusive
        (r#"g:{a TO z}"#, r#"((.__datadog_tags.g > "a" && .__datadog_tags.g < "z") ?? false)"#),
        // Range - alpha, inclusive (quoted)
        (r#"g:["a" TO "z"]"#, r#"((.__datadog_tags.g >= "a" && .__datadog_tags.g <= "z") ?? false)"#),
        // Range - alpha, exclusive (quoted)
        (r#"g:{"a" TO "z"}"#, r#"((.__datadog_tags.g > "a" && .__datadog_tags.g < "z") ?? false)"#),
        // AND match, known tags.
        (
            "message:this AND @title:that",
            r#"((match(.message, r'\bthis\b') ?? false) && (match(.custom.title, r'\bthat\b') ?? false))"#
        ),
        // OR match, known tags.
        (
            "message:this OR @title:that",
            r#"((match(.message, r'\bthis\b') ?? false) || (match(.custom.title, r'\bthat\b') ?? false))"#
        ),
        // AND + OR match, nested, known tags.
        (
            "message:this AND (@title:that OR @title:the_other)",
            r#"((match(.message, r'\bthis\b') ?? false) && ((match(.custom.title, r'\bthat\b') ?? false) || (match(.custom.title, r'\bthe_other\b') ?? false)))"#
        ),
        // AND match, keyword.
        (
            "this AND that",
            r#"(((match(.message, r'\bthis\b') ?? false) || ((match(.custom.error.message, r'\bthis\b') ?? false) || ((match(.custom.error.stack, r'\bthis\b') ?? false) || ((match(.custom.title, r'\bthis\b') ?? false) || (match(._default_, r'\bthis\b') ?? false))))) && ((match(.message, r'\bthat\b') ?? false) || ((match(.custom.error.message, r'\bthat\b') ?? false) || ((match(.custom.error.stack, r'\bthat\b') ?? false) || ((match(.custom.title, r'\bthat\b') ?? false) || (match(._default_, r'\bthat\b') ?? false))))))"#,
        ),
        // AND match, keyword (negate last).
        (
            "this AND NOT that",
            r#"(((match(.message, r'\bthis\b') ?? false) || ((match(.custom.error.message, r'\bthis\b') ?? false) || ((match(.custom.error.stack, r'\bthis\b') ?? false) || ((match(.custom.title, r'\bthis\b') ?? false) || (match(._default_, r'\bthis\b') ?? false))))) && !((match(.message, r'\bthat\b') ?? false) || ((match(.custom.error.message, r'\bthat\b') ?? false) || ((match(.custom.error.stack, r'\bthat\b') ?? false) || ((match(.custom.title, r'\bthat\b') ?? false) || (match(._default_, r'\bthat\b') ?? false))))))"#,
        ),
        // AND match, keyword (negate last w/-).
        (
            "this AND -that",
            r#"(((match(.message, r'\bthis\b') ?? false) || ((match(.custom.error.message, r'\bthis\b') ?? false) || ((match(.custom.error.stack, r'\bthis\b') ?? false) || ((match(.custom.title, r'\bthis\b') ?? false) || (match(._default_, r'\bthis\b') ?? false))))) && !((match(.message, r'\bthat\b') ?? false) || ((match(.custom.error.message, r'\bthat\b') ?? false) || ((match(.custom.error.stack, r'\bthat\b') ?? false) || ((match(.custom.title, r'\bthat\b') ?? false) || (match(._default_, r'\bthat\b') ?? false))))))"#,
        ),
        // OR match, keyword, explicit.
        (
            "this OR that",
            r#"(((match(.message, r'\bthis\b') ?? false) || ((match(.custom.error.message, r'\bthis\b') ?? false) || ((match(.custom.error.stack, r'\bthis\b') ?? false) || ((match(.custom.title, r'\bthis\b') ?? false) || (match(._default_, r'\bthis\b') ?? false))))) || ((match(.message, r'\bthat\b') ?? false) || ((match(.custom.error.message, r'\bthat\b') ?? false) || ((match(.custom.error.stack, r'\bthat\b') ?? false) || ((match(.custom.title, r'\bthat\b') ?? false) || (match(._default_, r'\bthat\b') ?? false))))))"#,
        ),
        // AND and OR match.
        (
            "this AND (that OR the_other)",
            r#"(((match(.message, r'\bthis\b') ?? false) || ((match(.custom.error.message, r'\bthis\b') ?? false) || ((match(.custom.error.stack, r'\bthis\b') ?? false) || ((match(.custom.title, r'\bthis\b') ?? false) || (match(._default_, r'\bthis\b') ?? false))))) && (((match(.message, r'\bthat\b') ?? false) || ((match(.custom.error.message, r'\bthat\b') ?? false) || ((match(.custom.error.stack, r'\bthat\b') ?? false) || ((match(.custom.title, r'\bthat\b') ?? false) || (match(._default_, r'\bthat\b') ?? false))))) || ((match(.message, r'\bthe_other\b') ?? false) || ((match(.custom.error.message, r'\bthe_other\b') ?? false) || ((match(.custom.error.stack, r'\bthe_other\b') ?? false) || ((match(.custom.title, r'\bthe_other\b') ?? false) || (match(._default_, r'\bthe_other\b') ?? false)))))))"#,
        ),
        // OR and AND match.
        (
            "this OR (that AND the_other)",
            r#"(((match(.message, r'\bthis\b') ?? false) || ((match(.custom.error.message, r'\bthis\b') ?? false) || ((match(.custom.error.stack, r'\bthis\b') ?? false) || ((match(.custom.title, r'\bthis\b') ?? false) || (match(._default_, r'\bthis\b') ?? false))))) || (((match(.message, r'\bthat\b') ?? false) || ((match(.custom.error.message, r'\bthat\b') ?? false) || ((match(.custom.error.stack, r'\bthat\b') ?? false) || ((match(.custom.title, r'\bthat\b') ?? false) || (match(._default_, r'\bthat\b') ?? false))))) && ((match(.message, r'\bthe_other\b') ?? false) || ((match(.custom.error.message, r'\bthe_other\b') ?? false) || ((match(.custom.error.stack, r'\bthe_other\b') ?? false) || ((match(.custom.title, r'\bthe_other\b') ?? false) || (match(._default_, r'\bthe_other\b') ?? false)))))))"#,
        ),
        // A bit of everything.
        (
            "host:this OR ((@b:test* AND c:that) AND d:the_other @e:[1 TO 5])",
            r#"(.host == "this" || (((starts_with(.custom.b, "test") ?? false) && .__datadog_tags.c == "that") && (.__datadog_tags.d == "the_other" && (((((is_integer(.custom.e) || is_float(.custom.e)) && .custom.e >= 1) || .custom.e >= "1") && (((is_integer(.custom.e) || is_float(.custom.e)) && .custom.e <= 5) || .custom.e <= "5")) ?? false))))"#,
        ),
    ];

    #[test]
    /// Compile each Datadog search query -> VRL, and do the same with the equivalent direct
    /// VRL syntax, and then compare the results. Each expression should match identically to
    /// the debugging output.
    fn to_vrl() {
        for (dd, vrl) in TESTS.iter() {
            let node =
                parse(dd).unwrap_or_else(|_| panic!("invalid Datadog search syntax: {}", dd));

            let root = ast::RootExpr::Expr(make_node(build(&node)));

            let program = vrl_parser::parse(vrl).unwrap_or_else(|_| panic!("invalid VRL: {}", vrl));

            assert_eq!(
                format!("{:?}", vec![make_node(root)]),
                format!("{:?}", program.0),
                "Failed: DD= {}, VRL= {}",
                dd,
                vrl
            );
        }
    }

    #[test]
    /// Test that the program compiles, and has the right number of expressions (which should
    /// be initial Datadog tags parsing, and a subsequent query against tags and other fields.)
    fn compiles() {
        for (dd, _) in TESTS.iter() {
            let node = parse(dd).unwrap();

            let program = compile(build(&node))
                .unwrap_or_else(|e| panic!("failed to compile: '{}'. Errors: {:?}", dd, e));

            assert!(program.into_iter().len() == 2);
        }
    }
}
