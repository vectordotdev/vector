use super::{
    grammar,
    node::{BooleanType, Comparison, ComparisonValue, QueryNode},
};
use ordered_float::NotNan;
use vrl_parser::{
    ast::{self, Opcode},
    Span,
};

/// Default fields that represent the search path when a Datadog tag/facet is not provided.
static DEFAULT_FIELDS: &[&str] = &[
    "message",
    "custom.error.message",
    "custom.error.stack",
    "custom.title",
    "_default_",
];

/// Attributes that represent special fields in Datadog.
static RESERVED_ATTRIBUTES: &[&str] = &[
    "host",
    "source",
    "status",
    "service",
    "trace_id",
    "message",
    "timestamp",
    "tags",
];

/// Describes a field to search on.
enum Field {
    /// Default field (when tag/facet isn't provided)
    Default(String),

    /// Reserved field that receives special treatment in Datadog.
    Reserved(String),

    /// Custom field type not described elsewhere.
    Custom(String),
}

impl Field {
    fn as_str(&self) -> &str {
        match self {
            Self::Default(ref s) => s,
            Self::Reserved(ref s) => s,
            Self::Custom(ref s) => s,
        }
    }
}

impl From<BooleanType> for ast::Opcode {
    fn from(b: BooleanType) -> Self {
        match b {
            BooleanType::And => ast::Opcode::And,
            BooleanType::Or => ast::Opcode::Or,
        }
    }
}

impl From<Comparison> for ast::Opcode {
    fn from(c: Comparison) -> Self {
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
            ComparisonValue::String(value) => value
                .parse::<i64>()
                .map(ast::Literal::Integer)
                .unwrap_or_else(|_| ast::Literal::String(value)),

            ComparisonValue::Numeric(value) => {
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

/// Convert Datadog grammar to VRL.
impl From<QueryNode> for ast::Expr {
    fn from(q: QueryNode) -> Self {
        let exprs = match q {
            // Match everything.
            QueryNode::MatchAllDocs => make_queries(grammar::DEFAULT_FIELD)
                .into_iter()
                .map(|(_, query)| make_function_call("exists", vec![query]))
                .collect(),
            // Matching nothing.
            QueryNode::MatchNoDocs => make_queries(grammar::DEFAULT_FIELD)
                .into_iter()
                .map(|(_, query)| make_not(make_function_call("exists", vec![query])))
                .collect(),
            // Field existence.
            QueryNode::AttributeExists { attr } => make_queries(&attr)
                .into_iter()
                .map(|(_, query)| make_function_call("exists", vec![query]))
                .collect(),
            QueryNode::AttributeMissing { attr } => make_queries(&attr)
                .into_iter()
                .map(|(_, query)| make_not(make_function_call("exists", vec![query])))
                .collect(),
            // Equality.
            QueryNode::AttributeTerm { attr, value }
            | QueryNode::QuotedAttribute {
                attr,
                phrase: value,
            } => make_queries(&attr)
                .into_iter()
                .map(|(field, query)| match field {
                    Field::Default(_) => {
                        make_function_call("match", vec![query, make_regex(&value)])
                    }
                    _ => make_string_comparison(query, Opcode::Eq, &value),
                })
                .collect(),
            // Comparison.
            QueryNode::AttributeComparison {
                attr,
                comparator,
                value,
            } => make_queries(&attr)
                .into_iter()
                .map(|(_, query)| {
                    make_op(make_node(query), comparator.into(), value.clone().into())
                })
                .collect(),
            // Wildcard suffix.
            QueryNode::AttributePrefix { attr, prefix } => make_queries(&attr)
                .into_iter()
                .map(|(_, query)| {
                    make_function_call("match", vec![query, make_regex(&format!("{}*", &prefix))])
                })
                .collect(),
            // Arbitrary wildcard.
            QueryNode::AttributeWildcard { attr, wildcard } => make_queries(&attr)
                .into_iter()
                .map(|(_, query)| make_function_call("match", vec![query, make_regex(&wildcard)]))
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
                .map(|(_, query)| {
                    match (&lower, &upper) {
                        // If both bounds are wildcards, it'll match everything; just check the field exists.
                        (ComparisonValue::Unbounded, ComparisonValue::Unbounded) => {
                            make_function_call("exists", vec![query])
                        }
                        // Unbounded lower. Wrapped in a container group for negation compatibility.
                        (ComparisonValue::Unbounded, _) => make_container_group(make_op(
                            make_node(query),
                            if upper_inclusive {
                                ast::Opcode::Le
                            } else {
                                ast::Opcode::Lt
                            },
                            make_node(ast::Expr::Literal(make_node(upper.clone().into()))),
                        )),
                        // Unbounded upper. Wrapped in a container group for negation compatibility.
                        (_, ComparisonValue::Unbounded) => make_container_group(make_op(
                            make_node(query),
                            if lower_inclusive {
                                ast::Opcode::Ge
                            } else {
                                ast::Opcode::Gt
                            },
                            make_node(ast::Expr::Literal(make_node(lower.clone().into()))),
                        )),
                        // Definitive range.
                        _ => make_container_group(make_op(
                            make_node(make_op(
                                make_node(query.clone()),
                                if lower_inclusive {
                                    ast::Opcode::Ge
                                } else {
                                    ast::Opcode::Gt
                                },
                                make_node(ast::Expr::Literal(make_node(lower.clone().into()))),
                            )),
                            ast::Opcode::And,
                            make_node(make_op(
                                make_node(query),
                                if upper_inclusive {
                                    ast::Opcode::Le
                                } else {
                                    ast::Opcode::Lt
                                },
                                make_node(ast::Expr::Literal(make_node(upper.clone().into()))),
                            )),
                        )),
                    }
                })
                .collect(),
            // Negation. If the node is an operation type, wrap in a container before negating.
            QueryNode::NegatedNode { node } => {
                let node = match ast::Expr::from(*node) {
                    n @ ast::Expr::Op(_) => make_container_group(n),
                    n => n,
                };

                vec![make_not(node)]
            }
            // Compound.
            QueryNode::Boolean { oper, nodes } => {
                vec![nest_exprs(nodes.into_iter(), oper)]
            }
        };

        nest_exprs(exprs.into_iter(), Opcode::Or)
    }
}

/// Creates a VRL node with a default span.
fn make_node<T>(node: T) -> ast::Node<T> {
    ast::Node::new(Span::default(), node)
}

/// Converts a field/facet name to the VRL equivalent. Datadog payloads have a `message` field
/// (which is used whenever the default field is encountered. Facets are hosted on .custom.*.
fn normalize_fields<T: AsRef<str>>(value: T) -> Vec<Field> {
    let value = value.as_ref();
    if value.eq(grammar::DEFAULT_FIELD) {
        return DEFAULT_FIELDS
            .iter()
            .map(|s| Field::Default((*s).to_owned()))
            .collect();
    }

    let field = match value.replace("@", "custom.") {
        v if DEFAULT_FIELDS.contains(&v.as_ref()) => Field::Default(v),
        v if RESERVED_ATTRIBUTES.contains(&v.as_ref()) => Field::Reserved(v),
        v => Field::Custom(v),
    };

    vec![field]
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

/// Makes a Regex string to be used with the `match`.
fn make_regex<T: AsRef<str>>(value: T) -> ast::Expr {
    ast::Expr::Literal(make_node(ast::Literal::Regex(format!(
        "\\b{}\\b",
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

/// Makes a container group, for wrapping logic for easier negation.
fn make_container_group(expr: ast::Expr) -> ast::Expr {
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

/// Recursive, nested expressions, ultimately returning a single `ast::Expr`.
fn nest_exprs<Expr: ExactSizeIterator<Item = impl Into<ast::Expr>>, O: Into<ast::Opcode>>(
    mut exprs: Expr,
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
        // For 2+ expressions, recurse over the RHS.
        _ => make_container_group(make_op(
            make_node(expr),
            op,
            make_node(nest_exprs(exprs, op)),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::make_node;
    use crate::parse;
    use vrl_parser::ast;

    // Datadog search syntax -> VRL
    static TESTS: &[(&str, &str)] = &[
        // Match everything (empty).
        ("", "(exists(.message) || (exists(.custom.error.message) || (exists(.custom.error.stack) || (exists(.custom.title) || exists(._default_)))))"),
        // Match everything.
        ("*:*", "(exists(.message) || (exists(.custom.error.message) || (exists(.custom.error.stack) || (exists(.custom.title) || exists(._default_)))))"),
        // Match everything (negate).
        ("NOT(*:*)", "(!exists(.message) || (!exists(.custom.error.message) || (!exists(.custom.error.stack) || (!exists(.custom.title) || !exists(._default_)))))"),
        // Match nothing.
        ("-*:*", "(!exists(.message) || (!exists(.custom.error.message) || (!exists(.custom.error.stack) || (!exists(.custom.title) || !exists(._default_)))))"),
        // Tag exists.
        ("_exists_:a", "exists(.a)"),
        // Tag exists (negate).
        ("NOT _exists_:a", "!exists(.a)"),
        // Tag exists (negate w/-).
        ("-_exists_:a", "!exists(.a)"),
        // Facet exists.
        ("_exists_:@b", "exists(.custom.b)"),
        // Facet exists (negate).
        ("NOT _exists_:@b", "!exists(.custom.b)"),
        // Facet exists (negate w/-).
        ("-_exists_:@b", "!exists(.custom.b)"),
        // Tag doesn't exist.
        ("_missing_:a", "!exists(.a)"),
        // Tag doesn't exist (negate).
        ("NOT _missing_:a", "!!exists(.a)"),
        // Tag doesn't exist (negate w/-).
        ("-_missing_:a", "!!exists(.a)"),
        // Facet doesn't exist.
        ("_missing_:@b", "!exists(.custom.b)"),
        // Facet doesn't exist (negate).
        ("NOT _missing_:@b", "!!exists(.custom.b)"),
        // Facet doesn't exist (negate w/-).
        ("-_missing_:@b", "!!exists(.custom.b)"),
        // Keyword.
        ("bla", r#"(match(.message, r'\bbla\b') || (match(.custom.error.message, r'\bbla\b') || (match(.custom.error.stack, r'\bbla\b') || (match(.custom.title, r'\bbla\b') || match(._default_, r'\bbla\b')))))"#),
        // Keyword (negate).
        ("NOT bla", r#"!(match(.message, r'\bbla\b') || (match(.custom.error.message, r'\bbla\b') || (match(.custom.error.stack, r'\bbla\b') || (match(.custom.title, r'\bbla\b') || match(._default_, r'\bbla\b')))))"#),
        // Keyword (negate w/-).
        ("-bla", r#"!(match(.message, r'\bbla\b') || (match(.custom.error.message, r'\bbla\b') || (match(.custom.error.stack, r'\bbla\b') || (match(.custom.title, r'\bbla\b') || match(._default_, r'\bbla\b')))))"#),
        // Quoted keyword.
        (r#""bla""#, r#"(match(.message, r'\bbla\b') || (match(.custom.error.message, r'\bbla\b') || (match(.custom.error.stack, r'\bbla\b') || (match(.custom.title, r'\bbla\b') || match(._default_, r'\bbla\b')))))"#),
        // Quoted keyword (negate).
        (r#"NOT "bla""#, r#"!(match(.message, r'\bbla\b') || (match(.custom.error.message, r'\bbla\b') || (match(.custom.error.stack, r'\bbla\b') || (match(.custom.title, r'\bbla\b') || match(._default_, r'\bbla\b')))))"#),
        // Quoted keyword (negate w/-).
        (r#"-"bla""#, r#"!(match(.message, r'\bbla\b') || (match(.custom.error.message, r'\bbla\b') || (match(.custom.error.stack, r'\bbla\b') || (match(.custom.title, r'\bbla\b') || match(._default_, r'\bbla\b')))))"#),
        // Tag match.
        ("a:bla", r#".a == "bla""#),
        // Tag match (negate).
        ("NOT a:bla", r#"!(.a == "bla")"#),
        // Tag match (negate w/-).
        ("-a:bla", r#"!(.a == "bla")"#),
        // Quoted tag match.
        (r#"a:"bla""#, r#".a == "bla""#),
        // Quoted tag match (negate).
        (r#"NOT a:"bla""#, r#"!(.a == "bla")"#),
        // Quoted tag match (negate).
        (r#"-a:"bla""#, r#"!(.a == "bla")"#),
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
        ("*bla", r#"(match(.message, r'\b.*bla\b') || (match(.custom.error.message, r'\b.*bla\b') || (match(.custom.error.stack, r'\b.*bla\b') || (match(.custom.title, r'\b.*bla\b') || match(._default_, r'\b.*bla\b')))))"#),
        // Wildcard prefix (negate).
        ("NOT *bla", r#"!(match(.message, r'\b.*bla\b') || (match(.custom.error.message, r'\b.*bla\b') || (match(.custom.error.stack, r'\b.*bla\b') || (match(.custom.title, r'\b.*bla\b') || match(._default_, r'\b.*bla\b')))))"#),
        // Wildcard prefix (negate w/-).
        ("-*bla", r#"!(match(.message, r'\b.*bla\b') || (match(.custom.error.message, r'\b.*bla\b') || (match(.custom.error.stack, r'\b.*bla\b') || (match(.custom.title, r'\b.*bla\b') || match(._default_, r'\b.*bla\b')))))"#),
        // Wildcard suffix.
        ("bla*", r#"(match(.message, r'\bbla.*\b') || (match(.custom.error.message, r'\bbla.*\b') || (match(.custom.error.stack, r'\bbla.*\b') || (match(.custom.title, r'\bbla.*\b') || match(._default_, r'\bbla.*\b')))))"#),
        // Wildcard suffix (negate).
        ("NOT bla*", r#"!(match(.message, r'\bbla.*\b') || (match(.custom.error.message, r'\bbla.*\b') || (match(.custom.error.stack, r'\bbla.*\b') || (match(.custom.title, r'\bbla.*\b') || match(._default_, r'\bbla.*\b')))))"#),
        // Wildcard suffix (negate w/-).
        ("-bla*", r#"!(match(.message, r'\bbla.*\b') || (match(.custom.error.message, r'\bbla.*\b') || (match(.custom.error.stack, r'\bbla.*\b') || (match(.custom.title, r'\bbla.*\b') || match(._default_, r'\bbla.*\b')))))"#),
        // Multiple wildcards.
        ("*b*la*", r#"(match(.message, r'\b.*b.*la.*\b') || (match(.custom.error.message, r'\b.*b.*la.*\b') || (match(.custom.error.stack, r'\b.*b.*la.*\b') || (match(.custom.title, r'\b.*b.*la.*\b') || match(._default_, r'\b.*b.*la.*\b')))))"#),
        // Multiple wildcards (negate).
        ("NOT *b*la*", r#"!(match(.message, r'\b.*b.*la.*\b') || (match(.custom.error.message, r'\b.*b.*la.*\b') || (match(.custom.error.stack, r'\b.*b.*la.*\b') || (match(.custom.title, r'\b.*b.*la.*\b') || match(._default_, r'\b.*b.*la.*\b')))))"#),
        // Multiple wildcards (negate w/-).
        ("-*b*la*", r#"!(match(.message, r'\b.*b.*la.*\b') || (match(.custom.error.message, r'\b.*b.*la.*\b') || (match(.custom.error.stack, r'\b.*b.*la.*\b') || (match(.custom.title, r'\b.*b.*la.*\b') || match(._default_, r'\b.*b.*la.*\b')))))"#),
        // Wildcard prefix - tag.
        ("a:*bla", r#"match(.a, r'\b.*bla\b')"#),
        // Wildcard prefix - tag (negate).
        ("NOT a:*bla", r#"!match(.a, r'\b.*bla\b')"#),
        // Wildcard prefix - tag (negate w/-).
        ("-a:*bla", r#"!match(.a, r'\b.*bla\b')"#),
        // Wildcard suffix - tag.
        ("b:bla*", r#"match(.b, r'\bbla.*\b')"#),
        // Wildcard suffix - tag (negate).
        ("NOT b:bla*", r#"!match(.b, r'\bbla.*\b')"#),
        // Wildcard suffix - tag (negate w/-).
        ("-b:bla*", r#"!match(.b, r'\bbla.*\b')"#),
        // Multiple wildcards - tag.
        ("c:*b*la*", r#"match(.c, r'\b.*b.*la.*\b')"#),
        // Multiple wildcards - tag (negate).
        ("NOT c:*b*la*", r#"!match(.c, r'\b.*b.*la.*\b')"#),
        // Multiple wildcards - tag (negate w/-).
        ("-c:*b*la*", r#"!match(.c, r'\b.*b.*la.*\b')"#),
        // Wildcard prefix - facet.
        ("@a:*bla", r#"match(.custom.a, r'\b.*bla\b')"#),
        // Wildcard prefix - facet (negate).
        ("NOT @a:*bla", r#"!match(.custom.a, r'\b.*bla\b')"#),
        // Wildcard prefix - facet (negate w/-).
        ("-@a:*bla", r#"!match(.custom.a, r'\b.*bla\b')"#),
        // Wildcard suffix - facet.
        ("@b:bla*", r#"match(.custom.b, r'\bbla.*\b')"#),
        // Wildcard suffix - facet (negate).
        ("NOT @b:bla*", r#"!match(.custom.b, r'\bbla.*\b')"#),
        // Wildcard suffix - facet (negate w/-).
        ("-@b:bla*", r#"!match(.custom.b, r'\bbla.*\b')"#),
        // Multiple wildcards - facet.
        ("@c:*b*la*", r#"match(.custom.c, r'\b.*b.*la.*\b')"#),
        // Multiple wildcards - facet (negate).
        ("NOT @c:*b*la*", r#"!match(.custom.c, r'\b.*b.*la.*\b')"#),
        // Multiple wildcards - facet (negate w/-).
        ("-@c:*b*la*", r#"!match(.custom.c, r'\b.*b.*la.*\b')"#),
        // Range - numeric, inclusive.
        ("[1 TO 10]", "((.message >= 1 && .message <= 10) || ((.custom.error.message >= 1 && .custom.error.message <= 10) || ((.custom.error.stack >= 1 && .custom.error.stack <= 10) || ((.custom.title >= 1 && .custom.title <= 10) || (._default_ >= 1 && ._default_ <= 10)))))"),
        // Range - numeric, inclusive (negate).
        ("NOT [1 TO 10]", "!((.message >= 1 && .message <= 10) || ((.custom.error.message >= 1 && .custom.error.message <= 10) || ((.custom.error.stack >= 1 && .custom.error.stack <= 10) || ((.custom.title >= 1 && .custom.title <= 10) || (._default_ >= 1 && ._default_ <= 10)))))"),
        // Range - numeric, inclusive (negate w/-).
        ("-[1 TO 10]", "!((.message >= 1 && .message <= 10) || ((.custom.error.message >= 1 && .custom.error.message <= 10) || ((.custom.error.stack >= 1 && .custom.error.stack <= 10) || ((.custom.title >= 1 && .custom.title <= 10) || (._default_ >= 1 && ._default_ <= 10)))))"),
        // Range - numeric, inclusive, unbounded (upper).
        ("[50 TO *]", "((.message >= 50) || ((.custom.error.message >= 50) || ((.custom.error.stack >= 50) || ((.custom.title >= 50) || (._default_ >= 50)))))"),
        // Range - numeric, inclusive, unbounded (upper) (negate).
        ("NOT [50 TO *]", "!((.message >= 50) || ((.custom.error.message >= 50) || ((.custom.error.stack >= 50) || ((.custom.title >= 50) || (._default_ >= 50)))))"),
        // Range - numeric, inclusive, unbounded (upper) (negate w/-).
        ("-[50 TO *]", "!((.message >= 50) || ((.custom.error.message >= 50) || ((.custom.error.stack >= 50) || ((.custom.title >= 50) || (._default_ >= 50)))))"),
        // Range - numeric, inclusive, unbounded (lower).
        ("[* TO 50]", "((.message <= 50) || ((.custom.error.message <= 50) || ((.custom.error.stack <= 50) || ((.custom.title <= 50) || (._default_ <= 50)))))"),
        // Range - numeric, inclusive, unbounded (lower) (negate).
        ("NOT [* TO 50]", "!((.message <= 50) || ((.custom.error.message <= 50) || ((.custom.error.stack <= 50) || ((.custom.title <= 50) || (._default_ <= 50)))))"),
        // Range - numeric, inclusive, unbounded (lower) (negate w/-).
        ("-[* TO 50]", "!((.message <= 50) || ((.custom.error.message <= 50) || ((.custom.error.stack <= 50) || ((.custom.title <= 50) || (._default_ <= 50)))))"),
        // Range - numeric, inclusive, unbounded (both).
        ("[* TO *]", "(exists(.message) || (exists(.custom.error.message) || (exists(.custom.error.stack) || (exists(.custom.title) || exists(._default_)))))"),
        // Range - numeric, inclusive, unbounded (both) (negate).
        ("NOT [* TO *]", "!(exists(.message) || (exists(.custom.error.message) || (exists(.custom.error.stack) || (exists(.custom.title) || exists(._default_)))))"),
        // Range - numeric, inclusive, unbounded (both) (negate w/-).
        ("-[* TO *]", "!(exists(.message) || (exists(.custom.error.message) || (exists(.custom.error.stack) || (exists(.custom.title) || exists(._default_)))))"),
        // Range - numeric, inclusive, tag.
        ("a:[1 TO 10]", "(.a >= 1 && .a <= 10)"),
        // Range - numeric, inclusive, tag (negate).
        ("NOT a:[1 TO 10]", "!(.a >= 1 && .a <= 10)"),
        // Range - numeric, inclusive, tag (negate w/-).
        ("-a:[1 TO 10]", "!(.a >= 1 && .a <= 10)"),
        // Range - numeric, inclusive, unbounded (upper), tag.
        ("a:[50 TO *]", "(.a >= 50)"),
        // Range - numeric, inclusive, unbounded (upper), tag (negate).
        ("NOT a:[50 TO *]", "!(.a >= 50)"),
        // Range - numeric, inclusive, unbounded (upper), tag (negate w/-).
        ("-a:[50 TO *]", "!(.a >= 50)"),
        // Range - numeric, inclusive, unbounded (lower), tag.
        ("a:[* TO 50]", "(.a <= 50)"),
        // Range - numeric, inclusive, unbounded (lower), tag (negate).
        ("NOT a:[* TO 50]", "!(.a <= 50)"),
        // Range - numeric, inclusive, unbounded (lower), tag (negate w/-).
        ("-a:[* TO 50]", "!(.a <= 50)"),
        // Range - numeric, inclusive, unbounded (both), tag.
        ("a:[* TO *]", "exists(.a)"),
        // Range - numeric, inclusive, unbounded (both), tag (negate).
        ("NOT a:[* TO *]", "!exists(.a)"),
        // Range - numeric, inclusive, unbounded (both), tag (negate).
        ("-a:[* TO *]", "!exists(.a)"),
        // Range - numeric, inclusive, facet.
        ("@b:[1 TO 10]", "(.custom.b >= 1 && .custom.b <= 10)"),
        // Range - numeric, inclusive, facet (negate).
        ("NOT @b:[1 TO 10]", "!(.custom.b >= 1 && .custom.b <= 10)"),
        // Range - numeric, inclusive, facet (negate w/-).
        ("-@b:[1 TO 10]", "!(.custom.b >= 1 && .custom.b <= 10)"),
        // Range - numeric, inclusive, unbounded (upper), facet.
        ("@b:[50 TO *]", "(.custom.b >= 50)"),
        // Range - numeric, inclusive, unbounded (upper), facet (negate).
        ("NOT @b:[50 TO *]", "!(.custom.b >= 50)"),
        // Range - numeric, inclusive, unbounded (upper), facet (negate w/-).
        ("-@b:[50 TO *]", "!(.custom.b >= 50)"),
        // Range - numeric, inclusive, unbounded (lower), facet.
        ("@b:[* TO 50]", "(.custom.b <= 50)"),
        // Range - numeric, inclusive, unbounded (lower), facet (negate).
        ("NOT @b:[* TO 50]", "!(.custom.b <= 50)"),
        // Range - numeric, inclusive, unbounded (lower), facet (negate w/-).
        ("-@b:[* TO 50]", "!(.custom.b <= 50)"),
        // Range - numeric, inclusive, unbounded (both), facet.
        ("@b:[* TO *]", "exists(.custom.b)"),
        // Range - numeric, inclusive, unbounded (both), facet (negate).
        ("NOT @b:[* TO *]", "!exists(.custom.b)"),
        // Range - numeric, inclusive, unbounded (both), facet (negate w/-).
        ("-@b:[* TO *]", "!exists(.custom.b)"),
        // AND match, keyword.
        (
            "this AND that",
            r#"((match(.message, r'\bthis\b') || (match(.custom.error.message, r'\bthis\b') || (match(.custom.error.stack, r'\bthis\b') || (match(.custom.title, r'\bthis\b') || match(._default_, r'\bthis\b'))))) && (match(.message, r'\bthat\b') || (match(.custom.error.message, r'\bthat\b') || (match(.custom.error.stack, r'\bthat\b') || (match(.custom.title, r'\bthat\b') || match(._default_, r'\bthat\b'))))))"#,
        ),
        // AND match, keyword (negate last).
        (
            "this AND NOT that",
            r#"((match(.message, r'\bthis\b') || (match(.custom.error.message, r'\bthis\b') || (match(.custom.error.stack, r'\bthis\b') || (match(.custom.title, r'\bthis\b') || match(._default_, r'\bthis\b'))))) && !(match(.message, r'\bthat\b') || (match(.custom.error.message, r'\bthat\b') || (match(.custom.error.stack, r'\bthat\b') || (match(.custom.title, r'\bthat\b') || match(._default_, r'\bthat\b'))))))"#,
        ),
        // AND match, keyword (negate last w/-).
        (
            "this AND -that",
            r#"((match(.message, r'\bthis\b') || (match(.custom.error.message, r'\bthis\b') || (match(.custom.error.stack, r'\bthis\b') || (match(.custom.title, r'\bthis\b') || match(._default_, r'\bthis\b'))))) && !(match(.message, r'\bthat\b') || (match(.custom.error.message, r'\bthat\b') || (match(.custom.error.stack, r'\bthat\b') || (match(.custom.title, r'\bthat\b') || match(._default_, r'\bthat\b'))))))"#,
        ),
        // OR match, keyword, explicit.
        (
            "this OR that",
            r#"((match(.message, r'\bthis\b') || (match(.custom.error.message, r'\bthis\b') || (match(.custom.error.stack, r'\bthis\b') || (match(.custom.title, r'\bthis\b') || match(._default_, r'\bthis\b'))))) || (match(.message, r'\bthat\b') || (match(.custom.error.message, r'\bthat\b') || (match(.custom.error.stack, r'\bthat\b') || (match(.custom.title, r'\bthat\b') || match(._default_, r'\bthat\b'))))))"#,
        ),
        // AND and OR match.
        (
            "this AND (that OR the_other)",
            r#"((match(.message, r'\bthis\b') || (match(.custom.error.message, r'\bthis\b') || (match(.custom.error.stack, r'\bthis\b') || (match(.custom.title, r'\bthis\b') || match(._default_, r'\bthis\b'))))) && ((match(.message, r'\bthat\b') || (match(.custom.error.message, r'\bthat\b') || (match(.custom.error.stack, r'\bthat\b') || (match(.custom.title, r'\bthat\b') || match(._default_, r'\bthat\b'))))) || (match(.message, r'\bthe_other\b') || (match(.custom.error.message, r'\bthe_other\b') || (match(.custom.error.stack, r'\bthe_other\b') || (match(.custom.title, r'\bthe_other\b') || match(._default_, r'\bthe_other\b')))))))"#,
        ),
        // OR and AND match.
        (
            "this OR (that AND the_other)",
            r#"((match(.message, r'\bthis\b') || (match(.custom.error.message, r'\bthis\b') || (match(.custom.error.stack, r'\bthis\b') || (match(.custom.title, r'\bthis\b') || match(._default_, r'\bthis\b'))))) || ((match(.message, r'\bthat\b') || (match(.custom.error.message, r'\bthat\b') || (match(.custom.error.stack, r'\bthat\b') || (match(.custom.title, r'\bthat\b') || match(._default_, r'\bthat\b'))))) && (match(.message, r'\bthe_other\b') || (match(.custom.error.message, r'\bthe_other\b') || (match(.custom.error.stack, r'\bthe_other\b') || (match(.custom.title, r'\bthe_other\b') || match(._default_, r'\bthe_other\b')))))))"#,
        ),
        // A bit of everything.
        (
            "@a:this OR ((@b:test* c:that) AND d:the_other e:[1 TO 5])",
            r#"(.custom.a == "this" || ((match(.custom.b, r'\btest.*\b') && .c == "that") && (.d == "the_other" && (.e >= 1 && .e <= 5))))"#,
        ),
        // Range - numeric, exclusive
        ("f:{1 TO 10}", "(.f > 1 && .f < 10)"),
        // TODO: CURRENTLY FAILING TESTS -- needs work in the main grammar and/or VRL to support!
        // Range - alpha, inclusive
        // TODO: https://github.com/timberio/vector/issues/7539
        //(r#"["a" TO "z"]"#, r#".message >= "a" && .message <= "z""#),
    ];

    #[test]
    /// Compile each Datadog search query -> VRL, and do the same with the equivalent direct
    /// VRL syntax, and then compare the results.
    fn to_vrl() {
        for (dd, vrl) in TESTS.iter() {
            let node =
                parse(dd).unwrap_or_else(|_| panic!("invalid Datadog search syntax: {}", dd));
            let root = ast::RootExpr::Expr(make_node(ast::Expr::from(node)));

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
}
