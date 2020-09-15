extern crate pest;

use crate::{
    event::Value,
    mapping::{
        query::{
            self,
            arithmetic::Arithmetic,
            arithmetic::Operator,
            functions::{NotFn, ToBooleanFn, ToFloatFn, ToIntegerFn, ToStringFn, ToTimestampFn},
            path::Path as QueryPath,
            Literal,
        },
        Assignment, Deletion, Function, IfStatement, Mapping, Noop, OnlyFields, Result,
    },
};
use pest::{
    error::ErrorVariant,
    iterators::{Pair, Pairs},
    Parser,
};

static TOKEN_ERR: &str = "unexpected token sequence";

#[derive(Parser)]
#[grammar = "./mapping/parser/grammar.pest"]
struct MappingParser;

fn target_path_from_pair(pair: Pair<Rule>) -> Result<String> {
    let mut segments = Vec::new();
    for segment in pair.into_inner() {
        match segment.as_rule() {
            Rule::path_segment => segments.push(segment.as_str().to_string()),
            Rule::quoted_path_segment => {
                segments.push(quoted_path_from_pair(segment)?.replace(".", "\\."))
            }
            _ => unreachable!("parser should not allow other target_path child rules here"),
        }
    }
    Ok(segments.join("."))
}

fn quoted_path_from_pair(pair: Pair<Rule>) -> Result<String> {
    let (first, mut other) = split_inner_rules_from_pair(pair)?;
    let base = inner_quoted_string_escaped_from_pair(first)?;
    Ok(match other.next() {
        Some(pair) => base + pair.as_str(),
        None => base,
    })
}

fn path_segments_from_pair(pair: Pair<Rule>) -> Result<Vec<Vec<String>>> {
    let mut segments = Vec::new();
    for segment in pair.into_inner() {
        match segment.as_rule() {
            Rule::path_segment => segments.push(vec![segment.as_str().to_string()]),
            Rule::quoted_path_segment => segments.push(vec![quoted_path_from_pair(segment)?]),
            Rule::path_coalesce => {
                let mut options = Vec::new();
                for option in segment.into_inner() {
                    match option.as_rule() {
                        Rule::path_segment => options.push(option.as_str().to_string()),
                        Rule::quoted_path_segment => options.push(quoted_path_from_pair(option)?),
                        _ => unreachable!(
                            "parser should not allow other path_coalesce child rules here"
                        ),
                    }
                }
                segments.push(options);
            }
            _ => unreachable!("parser should not allow other path_segments child rules here"),
        }
    }
    Ok(segments)
}

fn query_arithmetic_product_from_pairs(mut pairs: Pairs<Rule>) -> Result<Box<dyn query::Function>> {
    let pair = pairs.next().ok_or(TOKEN_ERR)?;
    let mut left = query_from_pair(pair)?;
    let mut op = Operator::Multiply;

    for pair in pairs {
        match pair.as_rule() {
            Rule::arithmetic_operator_product => {
                op = match pair.as_str() {
                    "*" => Operator::Multiply,
                    "/" => Operator::Divide,
                    "%" => Operator::Modulo,
                    s => return Err(format!("operator not recognized: {}", s)),
                };
            }
            _ => {
                left = Box::new(Arithmetic::new(left, query_from_pair(pair)?, op.clone()));
            }
        }
    }

    Ok(left)
}

fn query_arithmetic_sum_from_pairs(mut pairs: Pairs<Rule>) -> Result<Box<dyn query::Function>> {
    let inner_pairs = pairs.next().ok_or(TOKEN_ERR)?.into_inner();
    let mut left = query_arithmetic_product_from_pairs(inner_pairs)?;
    let mut op = Operator::Add;

    for pair in pairs {
        match pair.as_rule() {
            Rule::arithmetic_operator_sum => {
                op = match pair.as_str() {
                    "+" => Operator::Add,
                    "-" => Operator::Subtract,
                    s => return Err(format!("operator not recognized: {}", s)),
                };
            }
            _ => {
                left = Box::new(Arithmetic::new(
                    left,
                    query_arithmetic_product_from_pairs(pair.into_inner())?,
                    op.clone(),
                ));
            }
        }
    }

    Ok(left)
}

fn query_arithmetic_compare_from_pairs(mut pairs: Pairs<Rule>) -> Result<Box<dyn query::Function>> {
    let inner_pairs = pairs.next().ok_or(TOKEN_ERR)?.into_inner();
    let mut left = query_arithmetic_sum_from_pairs(inner_pairs)?;
    let mut op = Operator::Equal;

    for pair in pairs {
        match pair.as_rule() {
            Rule::arithmetic_operator_compare => {
                op = match pair.as_str() {
                    "==" => Operator::Equal,
                    "!=" => Operator::NotEqual,
                    ">" => Operator::Greater,
                    ">=" => Operator::GreaterOrEqual,
                    "<" => Operator::Less,
                    "<=" => Operator::LessOrEqual,
                    s => return Err(format!("operator not recognized: {}", s)),
                };
            }
            _ => {
                left = Box::new(Arithmetic::new(
                    left,
                    query_arithmetic_sum_from_pairs(pair.into_inner())?,
                    op.clone(),
                ));
            }
        }
    }

    Ok(left)
}

fn query_arithmetic_boolean_from_pairs(mut pairs: Pairs<Rule>) -> Result<Box<dyn query::Function>> {
    let inner_pairs = pairs.next().ok_or(TOKEN_ERR)?.into_inner();
    let mut left = query_arithmetic_compare_from_pairs(inner_pairs)?;
    let mut op = Operator::And;

    for pair in pairs {
        match pair.as_rule() {
            Rule::arithmetic_operator_boolean => {
                op = match pair.as_str() {
                    "||" => Operator::Or,
                    "&&" => Operator::And,
                    s => return Err(format!("operator not recognized: {}", s)),
                };
            }
            _ => {
                left = Box::new(Arithmetic::new(
                    left,
                    query_arithmetic_compare_from_pairs(pair.into_inner())?,
                    op.clone(),
                ));
            }
        }
    }

    Ok(left)
}

fn query_arithmetic_from_pair(pair: Pair<Rule>) -> Result<Box<dyn query::Function>> {
    query_arithmetic_boolean_from_pairs(pair.into_inner())
}

fn query_function_from_pair(pair: Pair<Rule>) -> Result<Box<dyn query::Function>> {
    match pair.as_rule() {
        Rule::to_string => {
            let (first, mut other) = split_inner_rules_from_pair(pair)?;
            let query = query_arithmetic_from_pair(first)?;
            let default = other
                .next()
                .map(|r| match r.as_rule() {
                    Rule::string => r
                        .into_inner()
                        .next()
                        .ok_or(TOKEN_ERR)
                        .map_err(str::to_owned)
                        .and_then(inner_quoted_string_escaped_from_pair)
                        .map(Value::from),
                    Rule::null => Ok(Value::Null),
                    _ => unreachable!(
                        "parser should not allow other to_string default arg child rules here"
                    ),
                })
                .transpose()?;
            Ok(Box::new(ToStringFn::new(query, default)))
        }
        Rule::to_int => {
            let (first, mut other) = split_inner_rules_from_pair(pair)?;
            let query = query_arithmetic_from_pair(first)?;
            let default = other.next().map(|r| match r.as_rule() {
                // TODO: Try parsing directly into int first. Maybe return error
                // if the string is not a valid int.
                Rule::number => Value::Integer(r.as_str().parse::<f64>().unwrap() as i64),
                Rule::null => Value::Null,
                _ => unreachable!(
                    "parser should not allow other to_int default arg child rules here"
                ),
            });
            Ok(Box::new(ToIntegerFn::new(query, default)))
        }
        Rule::to_float => {
            let (first, mut other) = split_inner_rules_from_pair(pair)?;
            let query = query_arithmetic_from_pair(first)?;
            let default = other.next().map(|r| match r.as_rule() {
                Rule::number => Value::Float(r.as_str().parse::<f64>().unwrap()),
                Rule::null => Value::Null,
                _ => unreachable!(
                    "parser should not allow other to_float default arg child rules here"
                ),
            });
            Ok(Box::new(ToFloatFn::new(query, default)))
        }
        Rule::to_bool => {
            let (first, mut other) = split_inner_rules_from_pair(pair)?;
            let query = query_arithmetic_from_pair(first)?;
            let default = other.next().map(|r| match r.as_rule() {
                Rule::boolean => Value::Boolean(r.as_str() == "true"),
                Rule::null => Value::Null,
                _ => unreachable!(
                    "parser should not allow other to_bool default arg child rules here"
                ),
            });
            Ok(Box::new(ToBooleanFn::new(query, default)))
        }
        Rule::to_timestamp => {
            let (first, mut other) = split_inner_rules_from_pair(pair)?;
            let query = query_arithmetic_from_pair(first)?;
            let format = inner_quoted_string_escaped_from_pair(
                other
                    .next()
                    .ok_or(TOKEN_ERR)?
                    .into_inner()
                    .next()
                    .ok_or(TOKEN_ERR)?,
            )?;
            Ok(Box::new(ToTimestampFn::new(&format, query, None)?))
        }
        _ => unreachable!("parser should not allow other query_function child rules here"),
    }
}

fn inner_quoted_string_escaped_from_pair(pair: Pair<Rule>) -> Result<String> {
    // This is only executed once per string at parse time, and so I'm not
    // losing sleep over the reallocation. However, if we want to mutate the
    // underlying string then we can take some inspiration from:
    // https://github.com/rust-lang/rust/blob/master/src/librustc_lexer/src/unescape.rs

    let literal_str = pair.as_str();
    let mut escaped_chars: Vec<char> = Vec::with_capacity(literal_str.len());

    let mut is_escaped = false;
    for c in literal_str.chars() {
        if is_escaped {
            match c {
                '\\' => escaped_chars.push(c),
                'n' => escaped_chars.push('\n'),
                't' => escaped_chars.push('\t'),
                '"' => escaped_chars.push('"'),
                // This isn't reachable currently due to the explicit list of
                // allowed escape chars in our parser grammar. However, if that
                // changes then we might need to rely on this error.
                _ => return Err(format!("invalid escape char '{}'", c)),
            }
            is_escaped = false;
        } else if c == '\\' {
            is_escaped = true;
        } else {
            escaped_chars.push(c);
        }
    }

    Ok(escaped_chars.into_iter().collect())
}

fn query_from_pair(pair: Pair<Rule>) -> Result<Box<dyn query::Function>> {
    Ok(match pair.as_rule() {
        Rule::not_operator => {
            let inner_query = query_from_pair(pair.into_inner().next().ok_or(TOKEN_ERR)?)?;
            Box::new(NotFn::new(inner_query))
        }
        Rule::string => Box::new(Literal::from(Value::from(
            inner_quoted_string_escaped_from_pair(pair.into_inner().next().ok_or(TOKEN_ERR)?)?,
        ))),
        Rule::null => Box::new(Literal::from(Value::Null)),
        Rule::number => Box::new(Literal::from(Value::from(
            pair.as_str().parse::<f64>().unwrap(),
        ))),
        Rule::boolean => {
            let v = pair.as_str() == "true";
            Box::new(Literal::from(Value::from(v)))
        }
        Rule::dot_path => Box::new(QueryPath::from(path_segments_from_pair(pair)?)),
        Rule::group => query_arithmetic_from_pair(pair.into_inner().next().ok_or(TOKEN_ERR)?)?,
        Rule::query_function => {
            query_function_from_pair(pair.into_inner().next().ok_or(TOKEN_ERR)?)?
        }
        _ => unreachable!("parser should not allow other query child rules here"),
    })
}

fn if_statement_from_pairs(mut pairs: Pairs<Rule>) -> Result<Box<dyn Function>> {
    let query = query_arithmetic_from_pair(pairs.next().ok_or(TOKEN_ERR)?)?;

    let first = statement_from_pair(pairs.next().ok_or(TOKEN_ERR)?)?;

    let second = match pairs.next() {
        Some(pair) => statement_from_pair(pair)?,
        None => Box::new(Noop {}),
    };

    Ok(Box::new(IfStatement::new(query, first, second)))
}

fn function_from_pair(pair: Pair<Rule>) -> Result<Box<dyn Function>> {
    let rule = pair.as_rule();
    let paths = pair
        .into_inner()
        .map(target_path_from_pair)
        .collect::<Result<Vec<_>>>();

    match rule {
        Rule::deletion => Ok(Box::new(Deletion::new(paths?))),
        Rule::only_fields => Ok(Box::new(OnlyFields::new(paths?))),
        _ => unreachable!("parser should not allow other function child rules here"),
    }
}

fn statement_from_pair(pair: Pair<Rule>) -> Result<Box<dyn Function>> {
    match pair.as_rule() {
        Rule::assignment => {
            let mut inner_rules = pair.into_inner();
            let path = target_path_from_pair(inner_rules.next().ok_or(TOKEN_ERR)?)?;
            let query = query_arithmetic_from_pair(inner_rules.next().ok_or(TOKEN_ERR)?)?;
            Ok(Box::new(Assignment::new(path, query)))
        }
        Rule::function => function_from_pair(pair.into_inner().next().ok_or(TOKEN_ERR)?),
        Rule::if_statement => if_statement_from_pairs(pair.into_inner()),
        _ => unreachable!("parser should not allow other statement child rules here"),
    }
}

fn split_inner_rules_from_pair(pair: Pair<Rule>) -> Result<(Pair<Rule>, Pairs<Rule>)> {
    let mut inner = pair.into_inner();
    let first = inner.next().ok_or(TOKEN_ERR)?;

    Ok((first, inner))
}

fn mapping_from_pairs(pairs: Pairs<Rule>) -> Result<Mapping> {
    let mut assignments = Vec::<Box<dyn Function>>::new();
    for pair in pairs {
        match pair.as_rule() {
            // Rules expected at the root of a mapping statement.
            Rule::assignment | Rule::function | Rule::if_statement => {
                assignments.push(statement_from_pair(pair)?);
            }
            Rule::EOI => (),
            _ => unreachable!("parser should not allow other mapping child rules here"),
        }
    }
    Ok(Mapping::new(assignments))
}

pub fn parse(input: &str) -> Result<Mapping> {
    match MappingParser::parse(Rule::mapping, input) {
        Ok(a) => mapping_from_pairs(a),
        // We need to do a bit of manual pruning of the error here as any
        // non-silent rule will be included in the list of candidates for a
        // parse error. Since we have several different sets of arithmetic
        // operator rules we first remove all but one type and then we rename it
        // to a more general 'operator' rule.
        Err(mut err) => {
            if let ErrorVariant::ParsingError {
                ref mut positives,
                negatives: _,
            } = err.variant
            {
                let mut i = 0;
                while i != positives.len() {
                    match positives[i] {
                        Rule::arithmetic_operator_boolean
                        | Rule::arithmetic_operator_compare
                        | Rule::arithmetic_operator_sum => {
                            positives.remove(i);
                        }
                        _ => {
                            i += 1;
                        }
                    };
                }
            }
            err = err.renamed_rules(|rule| match *rule {
                Rule::arithmetic_operator_product => "operator".to_owned(),
                _ => format!("{:?}", rule),
            });
            Err(format!("mapping parse error\n{}", err))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_parser_errors() {
        let cases = vec![
            (".foo = {\"bar\"}", vec![" 1:8\n", "= expected query"]),
            (
                ". = \"bar\"",
                vec![" 1:2\n", "= expected path_segment or quoted_path_segment"],
            ),
            (
                ".foo = !",
                vec![" 1:9\n", "= expected dot_path, query_function, group, boolean, null, string, number, or not_operator"],
            ),
            (
                "foo = \"bar\"",
                vec![
                    " 1:1\n",
                    "= expected if_statement, target_path, or function",
                ],
            ),
            (
                ".foo.bar = \"baz\" and this",
                vec![" 1:18\n", "= expected EOI or operator"],
            ),
            (".foo.bar = \"baz\" +", vec![" 1:19", "= expected query"]),
            (
                ".foo.bar = .foo.(bar |)",
                vec![" 1:23\n", "= expected path_segment"],
            ),
            (
                "if .foo > 0 { .foo = \"bar\" } else",
                vec![" 1:30\n", "= expected EOI"],
            ),
            (
                r#"if .foo { }"#,
                vec![
                    " 1:11\n",
                    "= expected if_statement, target_path, or function",
                ],
            ),
            (
                r#"if { del(.foo) } else { del(.bar) }"#,
                vec![" 1:4\n", "= expected query"],
            ),
            (
                r#"if .foo > .bar { del(.foo) } else { .bar = .baz"#,
                // This message isn't great, ideally I'd like "expected closing bracket"
                vec![" 1:48\n", "= expected path_index or operator"],
            ),
            (
                r#"only_fields(.foo,)"#,
                vec![" 1:18\n", "= expected target_path"],
            ),
            (
                r#"only_fields()"#,
                vec![" 1:13\n", "= expected target_path"],
            ),
            (
                r#"only_fields(,)"#,
                vec![" 1:13\n", "= expected target_path"],
            ),
            (
                ".foo = string(\"bar\",)",
                vec![" 1:21\n", "= expected null or string"],
            ),
            (
                // Due to the explicit list of allowed escape chars our grammar
                // doesn't actually recognize this as a string literal.
                r#".foo = "invalid escape \k sequence""#,
                vec![" 1:8\n", "= expected query"],
            ),
            (
                // Same here as above.
                r#".foo."invalid \k escape".sequence = "foo""#,
                vec![" 1:6\n", "= expected path_segment or quoted_path_segment"],
            ),
        ];

        for (mapping, exp_expressions) in cases {
            let err = parse(mapping).err().unwrap().to_string();
            for exp in exp_expressions {
                assert!(
                    err.contains(exp),
                    "expected: {}\nwith mapping: {}\nfull error message: {}",
                    exp,
                    mapping,
                    err
                );
            }
        }
    }

    #[test]
    fn check_parser() {
        let cases = vec![
            (
                ".foo = \"bar\"",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(Literal::from(Value::from("bar"))),
                ))]),
            ),
            (
                r#".foo = "bar\t\n\\\n and also \\n""#,
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(Literal::from(Value::from("bar\t\n\\\n and also \\n"))),
                ))]),
            ),
            (
                ".foo.\"bar baz\".\"buz.bev\"[5].quz = \"bar\"",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo.bar baz.buz\\.bev[5].quz".to_string(),
                    Box::new(Literal::from(Value::from("bar"))),
                ))]),
            ),
            (
                ".foo = (\"bar\")",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(Literal::from(Value::from("bar"))),
                ))]),
            ),
            (
                ".foo = \"bar\\\"escaped\\\" stuff\"",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(Literal::from(Value::from("bar\"escaped\" stuff"))),
                ))]),
            ),
            (
                ".foo = true",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(Literal::from(Value::from(true))),
                ))]),
            ),
            (
                ".foo = false",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(Literal::from(Value::from(false))),
                ))]),
            ),
            (
                ".foo = null",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(Literal::from(Value::Null)),
                ))]),
            ),
            (
                ".foo = 50.5",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(Literal::from(Value::from(50.5))),
                ))]),
            ),
            (
                ".foo = .bar",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(QueryPath::from(vec![vec!["bar"]])),
                ))]),
            ),
            (
                ".foo = .bar[0].baz",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(QueryPath::from(vec![vec!["bar[0]"], vec!["baz"]])),
                ))]),
            ),
            (
                ".foo = .foo.\"bar baz\".\"buz.bev\"[5].quz",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(QueryPath::from(vec![
                        vec!["foo"],
                        vec!["bar baz"],
                        vec!["buz.bev[5]"],
                        vec!["quz"],
                    ])),
                ))]),
            ),
            (
                ".foo = .bar\n.bar.buz = .qux.quz",
                Mapping::new(vec![
                    Box::new(Assignment::new(
                        "foo".to_string(),
                        Box::new(QueryPath::from(vec![vec!["bar"]])),
                    )),
                    Box::new(Assignment::new(
                        "bar.buz".to_string(),
                        Box::new(QueryPath::from(vec![vec!["qux"], vec!["quz"]])),
                    )),
                ]),
            ),
            (
                ".foo = .bar\n\t\n.bar.buz = .qux.quz\n.qux = .bev",
                Mapping::new(vec![
                    Box::new(Assignment::new(
                        "foo".to_string(),
                        Box::new(QueryPath::from(vec![vec!["bar"]])),
                    )),
                    Box::new(Assignment::new(
                        "bar.buz".to_string(),
                        Box::new(QueryPath::from(vec![vec!["qux"], vec!["quz"]])),
                    )),
                    Box::new(Assignment::new(
                        "qux".to_string(),
                        Box::new(QueryPath::from(vec![vec!["bev"]])),
                    )),
                ]),
            ),
            (
                ".foo = .(bar | baz)\n",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(QueryPath::from(vec![vec!["bar", "baz"]])),
                ))]),
            ),
            (
                ".foo = .(bar | \"baz buz\")\n",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(QueryPath::from(vec![vec!["bar", "baz buz"]])),
                ))]),
            ),
            (
                ".foo = .foo.(bar | baz)\n \n",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(QueryPath::from(vec![vec!["foo"], vec!["bar", "baz"]])),
                ))]),
            ),
            (
                ".foo = .(foo | zap).(bar | baz | buz)",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(QueryPath::from(vec![
                        vec!["foo", "zap"],
                        vec!["bar", "baz", "buz"],
                    ])),
                ))]),
            ),
            (
                ".foo = !.bar",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(NotFn::new(Box::new(QueryPath::from(vec![vec!["bar"]])))),
                ))]),
            ),
            (
                ".foo = !!.bar",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(NotFn::new(Box::new(NotFn::new(Box::new(QueryPath::from(
                        vec![vec!["bar"]],
                    )))))),
                ))]),
            ),
            (
                ".foo = 5 + 15 / 10",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(Arithmetic::new(
                        Box::new(Literal::from(Value::from(5.0))),
                        Box::new(Arithmetic::new(
                            Box::new(Literal::from(Value::from(15.0))),
                            Box::new(Literal::from(Value::from(10.0))),
                            Operator::Divide,
                        )),
                        Operator::Add,
                    )),
                ))]),
            ),
            (
                ".foo = (5 + 15) / 10",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(Arithmetic::new(
                        Box::new(Arithmetic::new(
                            Box::new(Literal::from(Value::from(5.0))),
                            Box::new(Literal::from(Value::from(15.0))),
                            Operator::Add,
                        )),
                        Box::new(Literal::from(Value::from(10.0))),
                        Operator::Divide,
                    )),
                ))]),
            ),
            (
                ".foo = 1 || 2 > 3 * 4 + 5",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(Arithmetic::new(
                        Box::new(Literal::from(Value::from(1.0))),
                        Box::new(Arithmetic::new(
                            Box::new(Literal::from(Value::from(2.0))),
                            Box::new(Arithmetic::new(
                                Box::new(Arithmetic::new(
                                    Box::new(Literal::from(Value::from(3.0))),
                                    Box::new(Literal::from(Value::from(4.0))),
                                    Operator::Multiply,
                                )),
                                Box::new(Literal::from(Value::from(5.0))),
                                Operator::Add,
                            )),
                            Operator::Greater,
                        )),
                        Operator::Or,
                    )),
                ))]),
            ),
            (
                "del(.foo)",
                Mapping::new(vec![Box::new(Deletion::new(vec!["foo".to_string()]))]),
            ),
            (
                "del(.\"foo bar\")",
                Mapping::new(vec![Box::new(Deletion::new(vec!["foo bar".to_string()]))]),
            ),
            (
                "del(.foo)\ndel(.bar.baz)",
                Mapping::new(vec![
                    Box::new(Deletion::new(vec!["foo".to_string()])),
                    Box::new(Deletion::new(vec!["bar.baz".to_string()])),
                ]),
            ),
            (
                "del(.foo, .bar.baz)",
                Mapping::new(vec![Box::new(Deletion::new(vec![
                    "foo".to_string(),
                    "bar.baz".to_string(),
                ]))]),
            ),
            (
                r#"if .foo == 5 {
                    .foo = .bar
                  } else {
                    del(.buz)
                  }"#,
                Mapping::new(vec![Box::new(IfStatement::new(
                    Box::new(Arithmetic::new(
                        Box::new(QueryPath::from("foo")),
                        Box::new(Literal::from(Value::from(5.0))),
                        Operator::Equal,
                    )),
                    Box::new(Assignment::new(
                        "foo".to_string(),
                        Box::new(QueryPath::from("bar")),
                    )),
                    Box::new(Deletion::new(vec!["buz".to_string()])),
                ))]),
            ),
            (
                "if .foo > .buz { .thing = .foo }",
                Mapping::new(vec![Box::new(IfStatement::new(
                    Box::new(Arithmetic::new(
                        Box::new(QueryPath::from("foo")),
                        Box::new(QueryPath::from("buz")),
                        Operator::Greater,
                    )),
                    Box::new(Assignment::new(
                        "thing".to_string(),
                        Box::new(QueryPath::from("foo")),
                    )),
                    Box::new(Noop {}),
                ))]),
            ),
            (
                "only_fields(.foo)",
                Mapping::new(vec![Box::new(OnlyFields::new(vec!["foo".to_string()]))]),
            ),
            (
                "only_fields(.foo.bar, .baz)",
                Mapping::new(vec![Box::new(OnlyFields::new(vec![
                    "foo.bar".to_string(),
                    "baz".to_string(),
                ]))]),
            ),
            (
                ".foo = string(.foo, \"bar\")",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(ToStringFn::new(
                        Box::new(QueryPath::from("foo")),
                        Some(Value::from("bar")),
                    )),
                ))]),
            ),
            (
                ".foo = string(.bar)",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(ToStringFn::new(Box::new(QueryPath::from("bar")), None)),
                ))]),
            ),
            (
                ".foo = int(.foo, 5)",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(ToIntegerFn::new(
                        Box::new(QueryPath::from("foo")),
                        Some(Value::Integer(5)),
                    )),
                ))]),
            ),
            (
                ".foo = int(.foo, 5.0)",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(ToIntegerFn::new(
                        Box::new(QueryPath::from("foo")),
                        Some(Value::Integer(5)),
                    )),
                ))]),
            ),
            (
                ".foo = int(.bar)",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(ToIntegerFn::new(Box::new(QueryPath::from("bar")), None)),
                ))]),
            ),
            (
                ".foo = float(.foo, 5.5)",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(ToFloatFn::new(
                        Box::new(QueryPath::from("foo")),
                        Some(Value::Float(5.5)),
                    )),
                ))]),
            ),
            (
                ".foo = float(.bar)",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(ToFloatFn::new(Box::new(QueryPath::from("bar")), None)),
                ))]),
            ),
            (
                ".foo = bool(.foo, true)",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(ToBooleanFn::new(
                        Box::new(QueryPath::from("foo")),
                        Some(Value::Boolean(true)),
                    )),
                ))]),
            ),
            (
                ".foo = bool(.bar)",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(ToBooleanFn::new(Box::new(QueryPath::from("bar")), None)),
                ))]),
            ),
            (
                ".foo = timestamp(.foo, \"%d %m %Y\")",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(
                        ToTimestampFn::new("%d %m %Y", Box::new(QueryPath::from("foo")), None)
                            .unwrap(),
                    ),
                ))]),
            ),
        ];

        for (mapping, exp) in cases {
            match parse(mapping) {
                Ok(p) => assert_eq!(format!("{:?}", p), format!("{:?}", exp), "{}", mapping),
                Err(e) => panic!("{}, mapping: {}", e, mapping),
            }
        }
    }
}
