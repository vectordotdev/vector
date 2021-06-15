use super::{
    field::Field,
    node::{ComparisonValue, QueryNode},
    vrl::{
        coalesce, make_bool, make_container_group, make_function_call, make_node, make_not,
        make_op, make_queries, make_string, make_string_comparison, make_wildcard_regex,
        make_word_regex, recurse, recurse_op,
    },
};

use vrl_parser::ast;

/// Builds a VRL expression. Building an expression means converting each leaf of a
/// `QueryNode` into an equilvalent VRL `Expr`, and tracking whether any of its sub-expressions
/// require final coalescence to a `false` value to avoid error states.
pub struct Builder {
    coalesce: bool,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    /// Default to not requiring coalescence, since it is an error to use it when not needed.
    pub fn new() -> Self {
        Self { coalesce: false }
    }

    /// Build a VRL expression from a `&QueryNode`. Will recurse through each leaf element
    /// as required, and coalesce the final expression to false if a fallible expression is found.
    pub fn build(mut self, node: &QueryNode) -> ast::Expr {
        let expr = recurse(self.parse_node(&node).into_iter());

        if self.coalesce {
            coalesce(expr)
        } else {
            expr
        }
    }

    /// Indicate that coalescence is required in the final expression.
    fn coalesce(&mut self) {
        self.coalesce = true;
    }

    /// Parse the provided Datadog `QueryNode`. This will return a vector of VRL expressions,
    /// in order to accommodate expansion to multiple fields where relevant.
    fn parse_node(&mut self, node: &QueryNode) -> Vec<ast::Expr> {
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
                    Field::Default(_) => {
                        self.coalesce();
                        make_function_call("match", vec![query, make_word_regex(&value)])
                    }
                    // Special case for tags, which should be an array.
                    Field::Reserved(f) if f == "tags" => {
                        self.coalesce();
                        make_function_call("includes", vec![query, make_string(value)])
                    }
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
                .map(|(_, query)| {
                    make_op(make_node(query), comparator.into(), value.clone().into())
                })
                .collect(),
            // Wildcard suffix.
            QueryNode::AttributePrefix { attr, prefix } => make_queries(attr)
                .into_iter()
                .map(|(field, query)| {
                    self.coalesce();
                    match field {
                        Field::Default(_) => make_function_call(
                            "match",
                            vec![query, make_word_regex(&format!("{}*", &prefix))],
                        ),
                        _ => make_function_call("starts_with", vec![query, make_string(prefix)]),
                    }
                })
                .collect(),
            // Arbitrary wildcard.
            QueryNode::AttributeWildcard { attr, wildcard } => make_queries(attr)
                .into_iter()
                .map(|(field, query)| {
                    self.coalesce();

                    match field {
                        // Default fields use word boundary matching.
                        Field::Default(_) => {
                            make_function_call("match", vec![query, make_word_regex(&wildcard)])
                        }
                        // If there's only one `*` and it's at the beginning, `ends_with` is faster.
                        _ if wildcard.starts_with('*') && wildcard.matches('*').count() == 1 => {
                            make_function_call(
                                "ends_with",
                                vec![query, make_string(wildcard.replace('*', ""))],
                            )
                        }
                        // Otherwise, default to non word boundary matching.
                        _ => {
                            make_function_call("match", vec![query, make_wildcard_regex(&wildcard)])
                        }
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
                            self.coalesce();

                            let upper = match field {
                                Field::Facet(_) => upper.clone().into(),
                                _ => ComparisonValue::String(upper.to_string()).into(),
                            };

                            make_container_group(make_op(
                                make_node(query),
                                if *upper_inclusive {
                                    ast::Opcode::Le
                                } else {
                                    ast::Opcode::Lt
                                },
                                make_node(ast::Expr::Literal(make_node(upper))),
                            ))
                        }
                        // Unbounded upper. Wrapped in a container group for negation compatibility.
                        (_, ComparisonValue::Unbounded) => {
                            self.coalesce();

                            let lower = match field {
                                Field::Facet(_) => lower.clone().into(),
                                _ => ComparisonValue::String(lower.to_string()).into(),
                            };

                            make_container_group(make_op(
                                make_node(query),
                                if *lower_inclusive {
                                    ast::Opcode::Ge
                                } else {
                                    ast::Opcode::Gt
                                },
                                make_node(ast::Expr::Literal(make_node(lower))),
                            ))
                        }
                        // Definitive range.
                        _ => {
                            self.coalesce();

                            let (lower, upper) = match field {
                                Field::Facet(_) => (lower.clone().into(), upper.clone().into()),
                                _ => (
                                    ComparisonValue::String(lower.to_string()).into(),
                                    ComparisonValue::String(upper.to_string()).into(),
                                ),
                            };

                            make_container_group(make_op(
                                make_node(make_op(
                                    make_node(query.clone()),
                                    if *lower_inclusive {
                                        ast::Opcode::Ge
                                    } else {
                                        ast::Opcode::Gt
                                    },
                                    make_node(ast::Expr::Literal(make_node(lower))),
                                )),
                                ast::Opcode::And,
                                make_node(make_op(
                                    make_node(query),
                                    if *upper_inclusive {
                                        ast::Opcode::Le
                                    } else {
                                        ast::Opcode::Lt
                                    },
                                    make_node(ast::Expr::Literal(make_node(upper))),
                                )),
                            ))
                        }
                    }
                })
                .collect(),
            // Negation. If the node is an operation type, wrap in a container before negating.
            QueryNode::NegatedNode { node } => {
                let reset = self.coalesce;
                let mut expr = recurse(self.parse_node(node).into_iter());

                if self.coalesce {
                    expr = coalesce(expr);

                    // Set coalescence to its previous state.
                    self.coalesce = reset;
                }

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
                    .map(|node| recurse(self.parse_node(node).into_iter()));

                vec![recurse_op(exprs, oper)]
            }
        }
    }
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
        ("bla", r#"(match(.message, r'\bbla\b') || (match(.custom.error.message, r'\bbla\b') || (match(.custom.error.stack, r'\bbla\b') || (match(.custom.title, r'\bbla\b') || match(._default_, r'\bbla\b'))))) ?? false"#),
        // Keyword (negate).
        ("NOT bla", r#"!((match(.message, r'\bbla\b') || (match(.custom.error.message, r'\bbla\b') || (match(.custom.error.stack, r'\bbla\b') || (match(.custom.title, r'\bbla\b') || match(._default_, r'\bbla\b'))))) ?? false)"#),
        // Keyword (negate w/-).
        ("-bla", r#"!((match(.message, r'\bbla\b') || (match(.custom.error.message, r'\bbla\b') || (match(.custom.error.stack, r'\bbla\b') || (match(.custom.title, r'\bbla\b') || match(._default_, r'\bbla\b'))))) ?? false)"#),
        // Quoted keyword.
        (r#""bla""#, r#"(match(.message, r'\bbla\b') || (match(.custom.error.message, r'\bbla\b') || (match(.custom.error.stack, r'\bbla\b') || (match(.custom.title, r'\bbla\b') || match(._default_, r'\bbla\b'))))) ?? false"#),
        // Quoted keyword (negate).
        (r#"NOT "bla""#, r#"!((match(.message, r'\bbla\b') || (match(.custom.error.message, r'\bbla\b') || (match(.custom.error.stack, r'\bbla\b') || (match(.custom.title, r'\bbla\b') || match(._default_, r'\bbla\b'))))) ?? false)"#),
        // Quoted keyword (negate w/-).
        (r#"-"bla""#, r#"!((match(.message, r'\bbla\b') || (match(.custom.error.message, r'\bbla\b') || (match(.custom.error.stack, r'\bbla\b') || (match(.custom.title, r'\bbla\b') || match(._default_, r'\bbla\b'))))) ?? false)"#),
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
        // Quoted tag match (negate).
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
        ("*bla", r#"(match(.message, r'\b.*bla\b') || (match(.custom.error.message, r'\b.*bla\b') || (match(.custom.error.stack, r'\b.*bla\b') || (match(.custom.title, r'\b.*bla\b') || match(._default_, r'\b.*bla\b'))))) ?? false"#),
        // Wildcard prefix (negate).
        ("NOT *bla", r#"!((match(.message, r'\b.*bla\b') || (match(.custom.error.message, r'\b.*bla\b') || (match(.custom.error.stack, r'\b.*bla\b') || (match(.custom.title, r'\b.*bla\b') || match(._default_, r'\b.*bla\b'))))) ?? false)"#),
        // Wildcard prefix (negate w/-).
        ("-*bla", r#"!((match(.message, r'\b.*bla\b') || (match(.custom.error.message, r'\b.*bla\b') || (match(.custom.error.stack, r'\b.*bla\b') || (match(.custom.title, r'\b.*bla\b') || match(._default_, r'\b.*bla\b'))))) ?? false)"#),
        // Wildcard suffix.
        ("bla*", r#"(match(.message, r'\bbla.*\b') || (match(.custom.error.message, r'\bbla.*\b') || (match(.custom.error.stack, r'\bbla.*\b') || (match(.custom.title, r'\bbla.*\b') || match(._default_, r'\bbla.*\b'))))) ?? false"#),
        // Wildcard suffix (negate).
        ("NOT bla*", r#"!((match(.message, r'\bbla.*\b') || (match(.custom.error.message, r'\bbla.*\b') || (match(.custom.error.stack, r'\bbla.*\b') || (match(.custom.title, r'\bbla.*\b') || match(._default_, r'\bbla.*\b'))))) ?? false)"#),
        // Wildcard suffix (negate w/-).
        ("-bla*", r#"!((match(.message, r'\bbla.*\b') || (match(.custom.error.message, r'\bbla.*\b') || (match(.custom.error.stack, r'\bbla.*\b') || (match(.custom.title, r'\bbla.*\b') || match(._default_, r'\bbla.*\b'))))) ?? false)"#),
        // Multiple wildcards.
        ("*b*la*", r#"(match(.message, r'\b.*b.*la.*\b') || (match(.custom.error.message, r'\b.*b.*la.*\b') || (match(.custom.error.stack, r'\b.*b.*la.*\b') || (match(.custom.title, r'\b.*b.*la.*\b') || match(._default_, r'\b.*b.*la.*\b'))))) ?? false"#),
        // Multiple wildcards (negate).
        ("NOT *b*la*", r#"!((match(.message, r'\b.*b.*la.*\b') || (match(.custom.error.message, r'\b.*b.*la.*\b') || (match(.custom.error.stack, r'\b.*b.*la.*\b') || (match(.custom.title, r'\b.*b.*la.*\b') || match(._default_, r'\b.*b.*la.*\b'))))) ?? false)"#),
        // Multiple wildcards (negate w/-).
        ("-*b*la*", r#"!((match(.message, r'\b.*b.*la.*\b') || (match(.custom.error.message, r'\b.*b.*la.*\b') || (match(.custom.error.stack, r'\b.*b.*la.*\b') || (match(.custom.title, r'\b.*b.*la.*\b') || match(._default_, r'\b.*b.*la.*\b'))))) ?? false)"#),
        // Wildcard prefix - tag.
        ("a:*bla", r#"ends_with(.__datadog_tags.a, "bla") ?? false"#),
        // Wildcard prefix - tag (negate).
        ("NOT a:*bla", r#"!(ends_with(.__datadog_tags.a, "bla") ?? false)"#),
        // Wildcard prefix - tag (negate w/-).
        ("-a:*bla", r#"!(ends_with(.__datadog_tags.a, "bla") ?? false)"#),
        // Wildcard suffix - tag.
        ("b:bla*", r#"starts_with(.__datadog_tags.b, "bla") ?? false"#),
        // Wildcard suffix - tag (negate).
        ("NOT b:bla*", r#"!(starts_with(.__datadog_tags.b, "bla") ?? false)"#),
        // Wildcard suffix - tag (negate w/-).
        ("-b:bla*", r#"!(starts_with(.__datadog_tags.b, "bla") ?? false)"#),
        // Multiple wildcards - tag.
        ("c:*b*la*", r#"match(.__datadog_tags.c, r'^.*b.*la.*$') ?? false"#),
        // Multiple wildcards - tag (negate).
        ("NOT c:*b*la*", r#"!(match(.__datadog_tags.c, r'^.*b.*la.*$') ?? false)"#),
        // Multiple wildcards - tag (negate w/-).
        ("-c:*b*la*", r#"!(match(.__datadog_tags.c, r'^.*b.*la.*$') ?? false)"#),
        // Wildcard prefix - facet.
        ("@a:*bla", r#"ends_with(.custom.a, "bla") ?? false"#),
        // Wildcard prefix - facet (negate).
        ("NOT @a:*bla", r#"!(ends_with(.custom.a, "bla") ?? false)"#),
        // Wildcard prefix - facet (negate w/-).
        ("-@a:*bla", r#"!(ends_with(.custom.a, "bla") ?? false)"#),
        // Wildcard suffix - facet.
        ("@b:bla*", r#"starts_with(.custom.b, "bla") ?? false"#),
        // Wildcard suffix - facet (negate).
        ("NOT @b:bla*", r#"!(starts_with(.custom.b, "bla") ?? false)"#),
        // Wildcard suffix - facet (negate w/-).
        ("-@b:bla*", r#"!(starts_with(.custom.b, "bla") ?? false)"#),
        // Multiple wildcards - facet.
        ("@c:*b*la*", r#"match(.custom.c, r'^.*b.*la.*$') ?? false"#),
        // Multiple wildcards - facet (negate).
        ("NOT @c:*b*la*", r#"!(match(.custom.c, r'^.*b.*la.*$') ?? false)"#),
        // Multiple wildcards - facet (negate w/-).
        ("-@c:*b*la*", r#"!(match(.custom.c, r'^.*b.*la.*$') ?? false)"#),
        // Special case for tags.
        ("tags:a", r#"includes(.tags, "a") ?? false"#),
        // Special case for tags (negate).
        ("NOT tags:a", r#"!(includes(.tags, "a") ?? false)"#),
        // Special case for tags (negate w/-).
        ("-tags:a", r#"!(includes(.tags, "a") ?? false)"#),
        // Range - numeric, inclusive.
        ("[1 TO 10]", r#"((.message >= "1" && .message <= "10") || ((.custom.error.message >= "1" && .custom.error.message <= "10") || ((.custom.error.stack >= "1" && .custom.error.stack <= "10") || ((.custom.title >= "1" && .custom.title <= "10") || (._default_ >= "1" && ._default_ <= "10"))))) ?? false"#),
        // Range - numeric, inclusive (negate).
        ("NOT [1 TO 10]", r#"!(((.message >= "1" && .message <= "10") || ((.custom.error.message >= "1" && .custom.error.message <= "10") || ((.custom.error.stack >= "1" && .custom.error.stack <= "10") || ((.custom.title >= "1" && .custom.title <= "10") || (._default_ >= "1" && ._default_ <= "10"))))) ?? false)"#),
        // Range - numeric, inclusive (negate w/-).
        ("-[1 TO 10]", r#"!(((.message >= "1" && .message <= "10") || ((.custom.error.message >= "1" && .custom.error.message <= "10") || ((.custom.error.stack >= "1" && .custom.error.stack <= "10") || ((.custom.title >= "1" && .custom.title <= "10") || (._default_ >= "1" && ._default_ <= "10"))))) ?? false)"#),
        // Range - numeric, inclusive, unbounded (upper).
        ("[50 TO *]", r#"((.message >= "50") || ((.custom.error.message >= "50") || ((.custom.error.stack >= "50") || ((.custom.title >= "50") || (._default_ >= "50"))))) ?? false"#),
        // Range - numeric, inclusive, unbounded (upper) (negate).
        ("NOT [50 TO *]", r#"!(((.message >= "50") || ((.custom.error.message >= "50") || ((.custom.error.stack >= "50") || ((.custom.title >= "50") || (._default_ >= "50"))))) ?? false)"#),
        // Range - numeric, inclusive, unbounded (upper) (negate w/-).
        ("-[50 TO *]", r#"!(((.message >= "50") || ((.custom.error.message >= "50") || ((.custom.error.stack >= "50") || ((.custom.title >= "50") || (._default_ >= "50"))))) ?? false)"#),
        // Range - numeric, inclusive, unbounded (lower).
        ("[* TO 50]", r#"((.message <= "50") || ((.custom.error.message <= "50") || ((.custom.error.stack <= "50") || ((.custom.title <= "50") || (._default_ <= "50"))))) ?? false"#),
        // Range - numeric, inclusive, unbounded (lower) (negate).
        ("NOT [* TO 50]", r#"!(((.message <= "50") || ((.custom.error.message <= "50") || ((.custom.error.stack <= "50") || ((.custom.title <= "50") || (._default_ <= "50"))))) ?? false)"#),
        // Range - numeric, inclusive, unbounded (lower) (negate w/-).
        ("-[* TO 50]", r#"!(((.message <= "50") || ((.custom.error.message <= "50") || ((.custom.error.stack <= "50") || ((.custom.title <= "50") || (._default_ <= "50"))))) ?? false)"#),
        // Range - numeric, inclusive, unbounded (both).
        ("[* TO *]", "(exists(.message) || (exists(.custom.error.message) || (exists(.custom.error.stack) || (exists(.custom.title) || exists(._default_)))))"),
        // Range - numeric, inclusive, unbounded (both) (negate).
        ("NOT [* TO *]", "!(exists(.message) || (exists(.custom.error.message) || (exists(.custom.error.stack) || (exists(.custom.title) || exists(._default_)))))"),
        // Range - numeric, inclusive, unbounded (both) (negate w/-).
        ("-[* TO *]", "!(exists(.message) || (exists(.custom.error.message) || (exists(.custom.error.stack) || (exists(.custom.title) || exists(._default_)))))"),
        // Range - numeric, inclusive, tag.
        ("a:[1 TO 10]", r#"(.__datadog_tags.a >= "1" && .__datadog_tags.a <= "10") ?? false"#),
        // Range - numeric, inclusive, tag (negate).
        ("NOT a:[1 TO 10]", r#"!((.__datadog_tags.a >= "1" && .__datadog_tags.a <= "10") ?? false)"#),
        // Range - numeric, inclusive, tag (negate w/-).
        ("-a:[1 TO 10]", r#"!((.__datadog_tags.a >= "1" && .__datadog_tags.a <= "10") ?? false)"#),
        // Range - numeric, inclusive, unbounded (upper), tag.
        ("a:[50 TO *]", r#"(.__datadog_tags.a >= "50") ?? false"#),
        // Range - numeric, inclusive, unbounded (upper), tag (negate).
        ("NOT a:[50 TO *]", r#"!((.__datadog_tags.a >= "50") ?? false)"#),
        // Range - numeric, inclusive, unbounded (upper), tag (negate w/-).
        ("-a:[50 TO *]", r#"!((.__datadog_tags.a >= "50") ?? false)"#),
        // Range - numeric, inclusive, unbounded (lower), tag.
        ("a:[* TO 50]", r#"(.__datadog_tags.a <= "50") ?? false"#),
        // Range - numeric, inclusive, unbounded (lower), tag (negate).
        ("NOT a:[* TO 50]", r#"!((.__datadog_tags.a <= "50") ?? false)"#),
        // Range - numeric, inclusive, unbounded (lower), tag (negate w/-).
        ("-a:[* TO 50]", r#"!((.__datadog_tags.a <= "50") ?? false)"#),
        // Range - numeric, inclusive, unbounded (both), tag.
        ("a:[* TO *]", "exists(.__datadog_tags.a)"),
        // Range - numeric, inclusive, unbounded (both), tag (negate).
        ("NOT a:[* TO *]", "!exists(.__datadog_tags.a)"),
        // Range - numeric, inclusive, unbounded (both), tag (negate).
        ("-a:[* TO *]", "!exists(.__datadog_tags.a)"),
        // Range - numeric, inclusive, facet.
        ("@b:[1 TO 10]", "(.custom.b >= 1 && .custom.b <= 10) ?? false"),
        // Range - numeric, inclusive, facet (negate).
        ("NOT @b:[1 TO 10]", "!((.custom.b >= 1 && .custom.b <= 10) ?? false)"),
        // Range - numeric, inclusive, facet (negate w/-).
        ("-@b:[1 TO 10]", "!((.custom.b >= 1 && .custom.b <= 10) ?? false)"),
        // Range - numeric, inclusive, unbounded (upper), facet.
        ("@b:[50 TO *]", "(.custom.b >= 50) ?? false"),
        // Range - numeric, inclusive, unbounded (upper), facet (negate).
        ("NOT @b:[50 TO *]", "!((.custom.b >= 50) ?? false)"),
        // Range - numeric, inclusive, unbounded (upper), facet (negate w/-).
        ("-@b:[50 TO *]", "!((.custom.b >= 50) ?? false)"),
        // Range - numeric, inclusive, unbounded (lower), facet.
        ("@b:[* TO 50]", "(.custom.b <= 50) ?? false"),
        // Range - numeric, inclusive, unbounded (lower), facet (negate).
        ("NOT @b:[* TO 50]", "!((.custom.b <= 50) ?? false)"),
        // Range - numeric, inclusive, unbounded (lower), facet (negate w/-).
        ("-@b:[* TO 50]", "!((.custom.b <= 50) ?? false)"),
        // Range - numeric, inclusive, unbounded (both), facet.
        ("@b:[* TO *]", "exists(.custom.b)"),
        // Range - numeric, inclusive, unbounded (both), facet (negate).
        ("NOT @b:[* TO *]", "!exists(.custom.b)"),
        // Range - numeric, inclusive, unbounded (both), facet (negate w/-).
        ("-@b:[* TO *]", "!exists(.custom.b)"),
        // Range - tag, exclusive
        ("f:{1 TO 10}", r#"(.__datadog_tags.f > "1" && .__datadog_tags.f < "10") ?? false"#),
        // Range - facet, exclusive
        ("@f:{1 TO 10}", "(.custom.f > 1 && .custom.f < 10) ?? false"),
        // Range - alpha, inclusive
        (r#"g:[a TO z]"#, r#"(.__datadog_tags.g >= "a" && .__datadog_tags.g <= "z") ?? false"#),
        // Range - alpha, exclusive
        (r#"g:{a TO z}"#, r#"(.__datadog_tags.g > "a" && .__datadog_tags.g < "z") ?? false"#),
        // Range - alpha, inclusive (quoted)
        (r#"g:["a" TO "z"]"#, r#"(.__datadog_tags.g >= "a" && .__datadog_tags.g <= "z") ?? false"#),
        // Range - alpha, exclusive (quoted)
        (r#"g:{"a" TO "z"}"#, r#"(.__datadog_tags.g > "a" && .__datadog_tags.g < "z") ?? false"#),
        // AND match, known tags.
        (
            "message:this AND @title:that",
            r#"(match(.message, r'\bthis\b') && match(.custom.title, r'\bthat\b')) ?? false"#
        ),
        // OR match, known tags.
        (
            "message:this OR @title:that",
            r#"(match(.message, r'\bthis\b') || match(.custom.title, r'\bthat\b')) ?? false"#
        ),
        // AND + OR match, nested, known tags.
        (
            "message:this AND (@title:that OR @title:the_other)",
            r#"(match(.message, r'\bthis\b') && (match(.custom.title, r'\bthat\b') || match(.custom.title, r'\bthe_other\b'))) ?? false"#
        ),
        // AND match, keyword.
        (
            "this AND that",
            r#"((match(.message, r'\bthis\b') || (match(.custom.error.message, r'\bthis\b') || (match(.custom.error.stack, r'\bthis\b') || (match(.custom.title, r'\bthis\b') || match(._default_, r'\bthis\b'))))) && (match(.message, r'\bthat\b') || (match(.custom.error.message, r'\bthat\b') || (match(.custom.error.stack, r'\bthat\b') || (match(.custom.title, r'\bthat\b') || match(._default_, r'\bthat\b')))))) ?? false"#,
        ),
        // AND match, keyword (negate last).
        (
            "this AND NOT that",
            r#"((match(.message, r'\bthis\b') || (match(.custom.error.message, r'\bthis\b') || (match(.custom.error.stack, r'\bthis\b') || (match(.custom.title, r'\bthis\b') || match(._default_, r'\bthis\b'))))) && !((match(.message, r'\bthat\b') || (match(.custom.error.message, r'\bthat\b') || (match(.custom.error.stack, r'\bthat\b') || (match(.custom.title, r'\bthat\b') || match(._default_, r'\bthat\b'))))) ?? false)) ?? false"#,
        ),
        // AND match, keyword (negate last w/-).
        (
            "this AND -that",
            r#"((match(.message, r'\bthis\b') || (match(.custom.error.message, r'\bthis\b') || (match(.custom.error.stack, r'\bthis\b') || (match(.custom.title, r'\bthis\b') || match(._default_, r'\bthis\b'))))) && !((match(.message, r'\bthat\b') || (match(.custom.error.message, r'\bthat\b') || (match(.custom.error.stack, r'\bthat\b') || (match(.custom.title, r'\bthat\b') || match(._default_, r'\bthat\b'))))) ?? false)) ?? false"#,
        ),
        // OR match, keyword, explicit.
        (
            "this OR that",
            r#"((match(.message, r'\bthis\b') || (match(.custom.error.message, r'\bthis\b') || (match(.custom.error.stack, r'\bthis\b') || (match(.custom.title, r'\bthis\b') || match(._default_, r'\bthis\b'))))) || (match(.message, r'\bthat\b') || (match(.custom.error.message, r'\bthat\b') || (match(.custom.error.stack, r'\bthat\b') || (match(.custom.title, r'\bthat\b') || match(._default_, r'\bthat\b')))))) ?? false"#,
        ),
        // AND and OR match.
        (
            "this AND (that OR the_other)",
            r#"((match(.message, r'\bthis\b') || (match(.custom.error.message, r'\bthis\b') || (match(.custom.error.stack, r'\bthis\b') || (match(.custom.title, r'\bthis\b') || match(._default_, r'\bthis\b'))))) && ((match(.message, r'\bthat\b') || (match(.custom.error.message, r'\bthat\b') || (match(.custom.error.stack, r'\bthat\b') || (match(.custom.title, r'\bthat\b') || match(._default_, r'\bthat\b'))))) || (match(.message, r'\bthe_other\b') || (match(.custom.error.message, r'\bthe_other\b') || (match(.custom.error.stack, r'\bthe_other\b') || (match(.custom.title, r'\bthe_other\b') || match(._default_, r'\bthe_other\b'))))))) ?? false"#,
        ),
        // OR and AND match.
        (
            "this OR (that AND the_other)",
            r#"((match(.message, r'\bthis\b') || (match(.custom.error.message, r'\bthis\b') || (match(.custom.error.stack, r'\bthis\b') || (match(.custom.title, r'\bthis\b') || match(._default_, r'\bthis\b'))))) || ((match(.message, r'\bthat\b') || (match(.custom.error.message, r'\bthat\b') || (match(.custom.error.stack, r'\bthat\b') || (match(.custom.title, r'\bthat\b') || match(._default_, r'\bthat\b'))))) && (match(.message, r'\bthe_other\b') || (match(.custom.error.message, r'\bthe_other\b') || (match(.custom.error.stack, r'\bthe_other\b') || (match(.custom.title, r'\bthe_other\b') || match(._default_, r'\bthe_other\b'))))))) ?? false"#,
        ),
        // A bit of everything.
        (
            "host:this OR ((@b:test* AND c:that) AND d:the_other @e:[1 TO 5])",
            r#"(.host == "this" || ((starts_with(.custom.b, "test") && .__datadog_tags.c == "that") && (.__datadog_tags.d == "the_other" && (.custom.e >= 1 && .custom.e <= 5)))) ?? false"#,
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

            let builder = Builder::new();
            let root = ast::RootExpr::Expr(make_node(builder.build(&node)));

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
            let builder = Builder::new();

            let program = compile(builder.build(&node))
                .unwrap_or_else(|e| panic!("failed to compile: '{}'. Errors: {:?}", dd, e));

            assert!(program.into_iter().len() == 2);
        }
    }
}
