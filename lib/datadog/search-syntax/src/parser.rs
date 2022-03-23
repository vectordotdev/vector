use pest::Parser;

use crate::{
    grammar::{EventPlatformQuery, QueryVisitor, DEFAULT_FIELD},
    node::QueryNode,
};

pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Quick wrapper parse function to convert query strings into our AST
pub fn parse(query: &str) -> Result<QueryNode, Error> {
    // Clean up our query string
    let clean_query = query.trim();
    // If we have an empty query, we presume we're matching everything
    if clean_query.is_empty() {
        return Ok(QueryNode::MatchAllDocs);
    }
    // Otherwise parse and interpret the query
    let mut ast = EventPlatformQuery::parse(crate::grammar::Rule::queryroot, query)?;
    let rootquery = ast.next().ok_or("Unable to find root query")?;
    let q = QueryVisitor::visit_queryroot(rootquery, DEFAULT_FIELD);
    Ok(q)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{BooleanType, Comparison, ComparisonValue, QueryNode};

    #[test]
    fn parses_basic_string() {
        parse("foo:bar").expect("Unable to parse 'foo:bar'");
    }

    #[test]
    fn parses_whitespace() {
        let cases = [" ", "    ", "\t"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res, QueryNode::MatchAllDocs),
                "Failed to parse MatchAllDocs query out of empty input"
            );
        }
    }

    #[test]
    fn parses_unquoted_default_field_query() {
        let cases = ["foo"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res,
                QueryNode::AttributeTerm { ref attr, ref value }
                if attr == DEFAULT_FIELD && value == "foo"),
                "Unable to properly parse '{:?}' - got {:?}",
                query,
                res
            );
        }
    }

    #[test]
    fn parses_quoted_default_field_query() {
        let cases = ["\"foo bar\""];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res,
                QueryNode::QuotedAttribute { ref attr, ref phrase }
                if attr == DEFAULT_FIELD && phrase == "foo bar"),
                "Unable to properly parse '{:?}' - got {:?}",
                query,
                res
            );
        }
    }

    #[test]
    fn parses_attribute_term_query() {
        let cases = ["foo:bar", "foo:(bar)", "foo:b\\ar", "foo:(b\\ar)"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res,
                QueryNode::AttributeTerm { ref attr, ref value }
                if attr == "foo" && value == "bar"),
                "Unable to properly parse '{:?}' - got {:?}",
                query,
                res
            );
        }
    }

    #[test]
    fn parses_numeric_attribute_term_query() {
        let cases = ["foo:10"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res,
                QueryNode::AttributeTerm { ref attr, ref value }
                if attr == "foo" && value == "10"),
                "Unable to properly parse '{:?}' - got {:?}",
                query,
                res
            );
        }
    }

    #[test]
    fn parses_attribute_term_query_with_escapes() {
        let cases = ["foo:bar\\:baz", "fo\\o:bar\\:baz"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res,
                QueryNode::AttributeTerm { ref attr, ref value }
                if attr == "foo" && value == "bar:baz"),
                "Unable to properly parse '{:?}' - got {:?}",
                query,
                res
            );
        }
    }

    #[test]
    fn parses_attribute_comparison_query_with_escapes() {
        let cases = ["foo:<4.12345E-4", "foo:<4.12345E\\-4"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res,
                QueryNode::AttributeComparison { ref attr, value: ComparisonValue::Float(ref compvalue), comparator: Comparison::Lt }
                if attr == "foo" && (*compvalue - 4.12345E-4).abs() < 0.001),
                "Unable to properly parse '{:?}' - got {:?}",
                query,
                res
            );
        }
    }

    #[test]
    fn parses_and_normalizes_multiterm_query() {
        let cases = ["foo bar", "foo        bar"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res,
                QueryNode::AttributeTerm { ref attr, ref value }
                if attr == DEFAULT_FIELD && value == "foo bar"),
                "Unable to properly parse '{:?}' - got {:?}",
                query,
                res
            );
        }
    }

    #[test]
    fn parses_multiple_multiterm_query() {
        let cases = ["foo bar baz AND qux quux quuz"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            if let QueryNode::Boolean {
                oper: BooleanType::And,
                ref nodes,
            } = res
            {
                assert!(
                    matches!(nodes[0], QueryNode::AttributeTerm { ref attr, ref value } if attr == "_default_" && value == "foo bar")
                        && matches!(nodes[1], QueryNode::AttributeTerm { ref attr, ref value } if attr == "_default_" && value == "baz")
                        && matches!(nodes[2], QueryNode::AttributeTerm { ref attr, ref value } if attr == "_default_" && value == "qux")
                        && matches!(nodes[3], QueryNode::AttributeTerm { ref attr, ref value } if attr == "_default_" && value == "quux quuz"),
                    "Unable to properly parse '{:?}' - got {:?}",
                    query,
                    res
                );
            } else {
                panic!("Unable to properly parse '{:?}' - got {:?}", query, res)
            }
        }
    }

    #[test]
    fn parses_negated_attribute_term_query() {
        let cases = ["-foo:bar", "- foo:bar", "NOT foo:bar"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            if let QueryNode::NegatedNode { ref node } = res {
                if let QueryNode::AttributeTerm {
                    ref attr,
                    ref value,
                } = **node
                {
                    if attr == "foo" && value == "bar" {
                        continue;
                    }
                }
            }
            panic!("Unable to properly parse '{:?}' - got {:?}", query, res)
        }
    }

    #[test]
    fn parses_quoted_attribute_term_query() {
        let cases = ["foo:\"bar baz\""];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res,
                QueryNode::QuotedAttribute { ref attr, ref phrase }
                if attr == "foo" && phrase == "bar baz"),
                "Unable to properly parse '{:?}' - got {:?}",
                query,
                res
            );
        }
    }

    #[test]
    fn parses_attribute_prefix_query() {
        let cases = ["foo:ba*"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res,
                QueryNode::AttributePrefix { ref attr, ref prefix }
                if attr == "foo" && prefix == "ba"), // We strip the trailing * from the prefix for escaping
                "Unable to properly parse '{:?}' - got {:?}",
                query,
                res
            );
        }
    }

    #[test]
    fn parses_attribute_wildcard_query() {
        let cases = ["foo:b*r"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res,
                QueryNode::AttributeWildcard { ref attr, ref wildcard }
                if attr == "foo" && wildcard == "b*r"),
                "Unable to properly parse '{:?}' - got {:?}",
                query,
                res
            );
        }
    }

    #[test]
    fn parses_attribute_wildcard_query_with_trailing_question_mark() {
        let cases = ["foo:ba?"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res,
                QueryNode::AttributeWildcard { ref attr, ref wildcard }
                if attr == "foo" && wildcard == "ba?"),
                "Unable to properly parse '{:?}' - got {:?}",
                query,
                res
            );
        }
    }

    #[test]
    fn parses_attribute_wildcard_query_with_leading_wildcard() {
        let cases = ["foo:*ar"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res,
                QueryNode::AttributeWildcard { ref attr, ref wildcard }
                if attr == "foo" && wildcard == "*ar"),
                "Unable to properly parse '{:?}' - got {:?}",
                query,
                res
            );
        }
    }

    #[test]
    fn parses_non_numeric_attribute_comparison_query() {
        let cases = ["foo:>=bar"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res,
                QueryNode::AttributeComparison {
                    ref attr,
                    value: ComparisonValue::String(ref cval),
                    comparator: Comparison::Gte
                } if attr == "foo" && cval == "bar"),
                "Unable to properly parse '{:?}' - got {:?}'",
                query,
                res
            );
        }
    }

    #[test]
    fn parses_numeric_attribute_range_query() {
        let cases = ["foo:[10 TO 20]"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res,
                QueryNode::AttributeRange {
                    ref attr,
                    lower: ComparisonValue::Integer(ref lstr),
                    lower_inclusive: true,
                    upper: ComparisonValue::Integer(ref ustr),
                    upper_inclusive: true
                } if attr == "foo" && *lstr == 10 && *ustr == 20),
                "Unable to properly parse '{:?}' - got {:?}'",
                query,
                res
            );
        }
    }

    #[test]
    fn parses_non_numeric_attribute_range_query() {
        let cases = ["foo:{bar TO baz}"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res,
                QueryNode::AttributeRange {
                    ref attr,
                    lower: ComparisonValue::String(ref lstr),
                    lower_inclusive: false,
                    upper: ComparisonValue::String(ref ustr),
                    upper_inclusive: false
                } if attr == "foo" && lstr == "bar" && ustr == "baz"),
                "Unable to properly parse '{:?}' - got {:?}'",
                query,
                res
            );
        }
    }

    #[test]
    fn parses_attribute_range_query_with_open_endpoints() {
        let cases = ["foo:[* TO *]"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res,
                QueryNode::AttributeRange {
                    ref attr,
                    lower: ComparisonValue::Unbounded,
                    lower_inclusive: true,
                    upper: ComparisonValue::Unbounded,
                    upper_inclusive: true
                } if attr == "foo"),
                "Unable to properly parse '{:?}' - got {:?}'",
                query,
                res
            );
        }
    }

    #[test]
    fn parses_attribute_range_query_with_fake_wildcards() {
        let cases = ["foo:[ba* TO b*z]"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res,
                QueryNode::AttributeRange {
                    ref attr,
                    lower: ComparisonValue::String(ref lstr),
                    lower_inclusive: true,
                    upper: ComparisonValue::String(ref ustr),
                    upper_inclusive: true
                } if attr == "foo" && lstr == "ba*" && ustr == "b*z"),
                "Unable to properly parse '{:?}' - got {:?}'",
                query,
                res
            );
        }
    }

    #[test]
    fn parses_attribute_exists_query() {
        let cases = ["_exists_:foo", "_exists_:\"foo\""];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res,
                QueryNode::AttributeExists { ref attr }
                if attr == "foo"),
                "Unable to properly parse '{:?}' - got {:?}",
                query,
                res
            );
        }
    }

    #[test]
    fn parses_attribute_exists_query_with_escapes() {
        let cases = ["_exists_:foo\\ bar"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res,
                QueryNode::AttributeExists { ref attr }
                if attr == "foo bar"),
                "Unable to properly parse '{:?}' - got {:?}",
                query,
                res
            );
        }
    }

    #[test]
    fn parses_star_as_wildcard_not_exists() {
        let cases = ["foo:*"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res,
                QueryNode::AttributeWildcard { ref attr, ref wildcard }
                if attr == "foo" && wildcard == "*"),
                "Unable to properly parse '{:?}' - got {:?}",
                query,
                res
            );
        }
    }

    #[test]
    fn parses_attribute_missing_query() {
        let cases = ["_missing_:foo", "_missing_:\"foo\""];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res,
                QueryNode::AttributeMissing { ref attr }
                if attr == "foo"),
                "Unable to properly parse '{:?}' - got {:?}",
                query,
                res
            );
        }
    }

    #[test]
    fn parses_attribute_missing_query_with_escapes() {
        let cases = ["_missing_:foo\\ bar"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res,
                QueryNode::AttributeMissing { ref attr }
                if attr == "foo bar"),
                "Unable to properly parse '{:?}' - got {:?}",
                query,
                res
            );
        }
    }

    #[test]
    fn parses_match_all_docs_query() {
        let cases = ["*:*", "*", "_default_:*", "foo:(*:*)"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res, QueryNode::MatchAllDocs),
                "Failed to parse '{:?}' as MatchAllDocs, got {:?}",
                query,
                res
            );
        }
    }

    #[test]
    fn parses_all_as_wildcard() {
        let cases = ["_all:*"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res,
                QueryNode::AttributeWildcard { ref attr, ref wildcard }
                if attr == "_all" && wildcard == "*"),
                "Unable to properly parse '{:?}' - got {:?}",
                query,
                res
            );
        }
    }

    #[test]
    fn parses_match_no_docs_query() {
        let cases = [
            "NOT *:*",
            "NOT *",
            "NOT _default_:*",
            "NOT foo:(*:*)",
            "foo:(NOT *:*)",
        ];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            assert!(
                matches!(res, QueryNode::MatchNoDocs),
                "Failed to parse '{:?}' as MatchNoDocs, got {:?}",
                query,
                res
            );
        }
    }

    #[test]
    fn parses_boolean_nodes_with_implicit_operators() {
        let cases = ["foo:bar baz:qux quux:quuz"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            if let QueryNode::Boolean {
                oper: BooleanType::And,
                ref nodes,
            } = res
            {
                assert!(
                    matches!(nodes[0], QueryNode::AttributeTerm { ref attr, ref value } if attr == "foo" && value == "bar")
                        && matches!(nodes[1], QueryNode::AttributeTerm { ref attr, ref value } if attr == "baz" && value == "qux")
                        && matches!(nodes[2], QueryNode::AttributeTerm { ref attr, ref value } if attr == "quux" && value == "quuz"),
                    "Unable to properly parse '{:?}' - got {:?}",
                    query,
                    res
                );
            } else {
                panic!("Unable to properly parse '{:?}' - got {:?}", query, res)
            }
        }
    }

    #[test]
    fn parses_boolean_nodes_with_implicit_operators_and_negated_clauses() {
        let cases = [
            "NOT foo:bar baz:qux NOT quux:quuz",
            "NOT foo:bar baz:qux -quux:quuz",
            "-foo:bar baz:qux NOT quux:quuz",
            "-foo:bar baz:qux -quux:quuz",
        ];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            if let QueryNode::Boolean {
                oper: BooleanType::And,
                ref nodes,
            } = res
            {
                assert!(
                    matches!(nodes[0], QueryNode::NegatedNode { ref node } if matches!(**node, QueryNode::AttributeTerm {ref attr, ref value } if attr == "foo" && value == "bar"))
                        && matches!(nodes[1], QueryNode::AttributeTerm { ref attr, ref value } if attr == "baz" && value == "qux")
                        && matches!(nodes[2], QueryNode::NegatedNode { ref node } if matches!(**node, QueryNode::AttributeTerm {ref attr, ref value } if attr == "quux" && value == "quuz")),
                    "Unable to properly parse '{:?}' - got {:?}",
                    query,
                    res
                );
            } else {
                panic!("Unable to properly parse '{:?}' - got {:?}", query, res)
            }
        }
    }

    #[test]
    fn parses_boolean_nodes_with_explicit_operators() {
        let cases = ["foo:bar OR baz:qux AND quux:quuz"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            if let QueryNode::Boolean {
                oper: BooleanType::And,
                ref nodes,
            } = res
            {
                assert!(
                    matches!(nodes[0], QueryNode::AttributeTerm { ref attr, ref value } if attr == "baz" && value == "qux")
                        && matches!(nodes[1], QueryNode::AttributeTerm { ref attr, ref value } if attr == "quux" && value == "quuz"),
                    "Unable to properly parse '{:?}' - got {:?}",
                    query,
                    res
                );
            } else {
                panic!("Unable to properly parse '{:?}' - got {:?}", query, res)
            }
        }
    }

    #[test]
    fn parses_boolean_nodes_with_implicit_and_explicit_operators() {
        let cases = ["foo:bar OR baz:qux quux:quuz"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            if let QueryNode::Boolean {
                oper: BooleanType::And,
                ref nodes,
            } = res
            {
                assert!(
                    matches!(nodes[0], QueryNode::AttributeTerm { ref attr, ref value } if attr == "quux" && value == "quuz"),
                    "Unable to properly parse '{:?}' - got {:?}",
                    query,
                    res
                );
            } else {
                panic!("Unable to properly parse '{:?}' - got {:?}", query, res)
            }
        }
    }

    #[test]
    fn parses_nested_boolean_query_node() {
        let cases = ["foo:bar (baz:qux quux:quuz)"];
        for query in cases.iter() {
            let res = parse(query).unwrap_or_else(|_| panic!("Unable to parse query {:?}", query));
            if let QueryNode::Boolean {
                oper: BooleanType::And,
                ref nodes,
            } = res
            {
                assert!(
                    matches!(nodes[0], QueryNode::AttributeTerm {ref attr, ref value } if attr == "foo" && value == "bar")
                        && matches!(nodes[1], QueryNode::Boolean { oper: BooleanType::And, ref nodes } if
                            matches!(nodes[0], QueryNode::AttributeTerm { ref attr, ref value } if attr == "baz" && value == "qux") &&
                            matches!(nodes[1], QueryNode::AttributeTerm { ref attr, ref value } if attr == "quux" && value == "quuz")
                        ),
                    "Unable to properly parse '{:?}' - got {:?}",
                    query,
                    res
                );
            } else {
                panic!("Unable to properly parse '{:?}' - got {:?}", query, res)
            }
        }
    }
}
