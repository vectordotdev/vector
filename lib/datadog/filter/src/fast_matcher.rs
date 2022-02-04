use crate::{regex::word_regex, Resolver};
use datadog_search_syntax::{BooleanType, Comparison, ComparisonValue, Field, QueryNode};
use regex::Regex;

#[derive(Debug, Clone)]
pub struct FastMatcher {
    pub mode: Mode,
}

// impl FastMatcher {
//     /// Returns true if `value` matches, else false
//     pub fn run(&self, log: &LogEvent) -> bool {
//         match op {
//             Op::True => true,
//             Op::False => true,
//             Op::Exists(field) => exists(field, log),
//             Op::NotExists(field) => !exists(&field, log),
//             Op::Equals { field, value } => equals(field, value, log),
//             Op::TagExists(value) => tag_exists(value, log),
//             Op::RegexMatch { field, re } => regex_match(field, re, log),
//             Op::Prefix(field, value) => {
//                 todo!()
//             }
//             Op::Wildcard(field, value) => {
//                 todo!()
//             }
//             Op::Compare(field, comparison, comparison_value) => {
//                 todo!()
//             }
//             Op::Range {
//                 field,
//                 lower,
//                 lower_inclusive,
//                 upper,
//                 upper_inclusive,
//             } => {
//                 todo!()
//             }
//             Op::Not(matcher) => {
//                 todo!()
//             }
//             Op::Nested(matcher) => EventFilter::run(matcher, log),
//         }
//     }
// }

#[derive(Debug, Clone)]
pub enum Mode {
    One(Op),
    Any(Vec<Op>),
    All(Vec<Op>),
}

#[derive(Debug, Clone)]
pub enum Op {
    True,
    False,
    Exists(Field),
    NotExists(Field),
    Equals {
        field: String,
        value: String,
    },
    TagExists(String),
    RegexMatch {
        field: String,
        re: Regex,
    },
    Prefix(Field, String),
    Wildcard(Field, String),
    Compare(Field, Comparison, ComparisonValue),
    Range {
        field: Field,
        lower: ComparisonValue,
        lower_inclusive: bool,
        upper: ComparisonValue,
        upper_inclusive: bool,
    },
    Not(Box<FastMatcher>),
    Nested(Box<FastMatcher>),
}

impl FastMatcher {
    fn op(op: Op) -> Self {
        Self {
            mode: Mode::One(op),
        }
    }

    fn any(ops: Vec<Op>) -> Self {
        Self {
            mode: Mode::Any(ops),
        }
    }

    fn all(ops: Vec<Op>) -> Self {
        Self {
            mode: Mode::All(ops),
        }
    }
}

pub fn build_matcher<F>(node: &QueryNode, filter: &F) -> FastMatcher
where
    F: Resolver,
{
    match node {
        QueryNode::MatchNoDocs => FastMatcher::op(Op::False),
        QueryNode::MatchAllDocs => FastMatcher::op(Op::True),
        QueryNode::AttributeExists { attr } => {
            let matchers = filter
                .build_fields(attr)
                .into_iter()
                .map(|field| Op::Exists(field))
                .collect();

            FastMatcher::any(matchers)
        }
        QueryNode::AttributeMissing { attr } => {
            let matchers = filter
                .build_fields(attr)
                .into_iter()
                .map(|field| Op::NotExists(field))
                .collect();

            FastMatcher::all(matchers)
        }
        QueryNode::AttributeTerm { attr, value }
        | QueryNode::QuotedAttribute {
            attr,
            phrase: value,
        } => {
            let matchers = filter
                .build_fields(attr)
                .into_iter()
                .map(|field| match field {
                    Field::Default(field) => {
                        let re = word_regex(value);
                        Op::RegexMatch { field, re }
                    }
                    Field::Reserved(field) if field == "tags" => Op::TagExists(value.clone()),
                    Field::Tag(field) => {
                        let full = format!("{}:{}", field, value);
                        Op::TagExists(full)
                    }
                    Field::Reserved(field) | Field::Facet(field) => Op::Equals {
                        field,
                        value: value.clone(),
                    },
                })
                .collect();

            FastMatcher::any(matchers)
        }
        QueryNode::AttributePrefix { attr, prefix } => {
            let matchers = filter
                .build_fields(attr)
                .into_iter()
                .map(|field| Op::Prefix(field, prefix.clone()))
                .collect();

            FastMatcher::any(matchers)
        }
        QueryNode::AttributeWildcard { attr, wildcard } => {
            let matchers = filter
                .build_fields(attr)
                .into_iter()
                .map(|field| Op::Wildcard(field, wildcard.clone()))
                .collect();

            FastMatcher::any(matchers)
        }
        QueryNode::AttributeComparison {
            attr,
            comparator,
            value,
        } => {
            let matchers = filter
                .build_fields(attr)
                .into_iter()
                .map(|field| Op::Compare(field, *comparator, value.clone()))
                .collect();

            FastMatcher::any(matchers)
        }
        QueryNode::AttributeRange {
            attr,
            lower,
            lower_inclusive,
            upper,
            upper_inclusive,
        } => {
            let matchers = filter
                .build_fields(attr)
                .into_iter()
                .map(|field| Op::Range {
                    field,
                    lower: lower.clone(),
                    lower_inclusive: *lower_inclusive,
                    upper: upper.clone(),
                    upper_inclusive: *upper_inclusive,
                })
                .collect();

            FastMatcher::any(matchers)
        }
        QueryNode::NegatedNode { node } => {
            FastMatcher::op(Op::Not(Box::new(build_matcher(node, filter))))
        }
        QueryNode::Boolean { oper, nodes } => {
            let funcs = nodes
                .iter()
                .map(|node| build_matcher(node, filter))
                .map(Box::new)
                .map(Op::Nested)
                .collect();

            match oper {
                BooleanType::And => FastMatcher::all(funcs),
                BooleanType::Or => FastMatcher::any(funcs),
            }
        }
    }
}
