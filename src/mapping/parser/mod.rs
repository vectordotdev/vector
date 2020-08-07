extern crate pest;

use crate::{
    event::Value,
    mapping::{
        query,
        query::{arithmetic::Arithmetic, arithmetic::Operator, path::Path as QueryPath, Literal},
        Assignment, Deletion, Function, Mapping, Result,
    },
};

use pest::{
    error::ErrorVariant,
    iterators::{Pair, Pairs},
    Parser,
};

#[derive(Parser)]
#[grammar = "./mapping/parser/grammar.pest"]
struct MappingParser;

fn path_from_pair(pair: Pair<Rule>) -> Result<String> {
    Ok(pair.as_str().get(1..).unwrap().to_string())
}

fn path_segments_from_pair(pair: Pair<Rule>) -> Result<Vec<Vec<String>>> {
    let mut segments = Vec::new();
    for segment in pair.into_inner() {
        match segment.as_rule() {
            Rule::path_segment => segments.push(vec![segment.as_str().to_string()]),
            Rule::path_coalesce => {
                let mut options = Vec::new();
                for option in segment.into_inner() {
                    match option.as_rule() {
                        Rule::path_segment => options.push(option.as_str().to_string()),
                        _ => unreachable!(),
                    }
                }
                segments.push(options);
            }
            _ => unreachable!(),
        }
    }
    Ok(segments)
}

fn query_arithmetic_product_from_pairs(mut pairs: Pairs<Rule>) -> Result<Box<dyn query::Function>> {
    let mut left = query_from_pair(pairs.next().unwrap())?;
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
    let mut left = query_arithmetic_product_from_pairs(pairs.next().unwrap().into_inner())?;
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
    let mut left = query_arithmetic_sum_from_pairs(pairs.next().unwrap().into_inner())?;
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
    let mut left = query_arithmetic_compare_from_pairs(pairs.next().unwrap().into_inner())?;
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

fn query_arithmetic_from_pairs(mut pairs: Pairs<Rule>) -> Result<Box<dyn query::Function>> {
    query_arithmetic_boolean_from_pairs(pairs.next().unwrap().into_inner())
}

fn query_from_pair(pair: Pair<Rule>) -> Result<Box<dyn query::Function>> {
    Ok(match pair.as_rule() {
        Rule::string => Box::new(Literal::from(Value::from(
            pair.into_inner()
                .next()
                .unwrap()
                .as_str()
                // TODO: Include unicode escape sequences, surely there must be
                // a standard lib opposite of https://doc.rust-lang.org/std/primitive.str.html#method.escape_default
                // but I can't find it anywhere.
                .replace("\\\"", "\"")
                .replace("\\n", "\n")
                .replace("\\t", "\t")
                .replace("\\\\", "\\"),
        ))),
        Rule::null => Box::new(Literal::from(Value::Null)),
        Rule::number => Box::new(Literal::from(Value::from(
            pair.as_str().parse::<f64>().unwrap(),
        ))),
        Rule::boolean => {
            let v = if pair.as_str() == "true" { true } else { false };
            Box::new(Literal::from(Value::from(v)))
        }
        Rule::dot_path => Box::new(QueryPath::from(path_segments_from_pair(pair)?)),
        Rule::group => query_arithmetic_from_pairs(pair.into_inner())?,
        _ => unreachable!(),
    })
}

fn mapping_from_pairs(pairs: Pairs<Rule>) -> Result<Mapping> {
    let mut assignments = Vec::<Box<dyn Function>>::new();
    for pair in pairs {
        match pair.as_rule() {
            Rule::assignment => {
                let mut inner_rules = pair.into_inner();
                let path = path_from_pair(inner_rules.next().unwrap())?;
                let query = query_arithmetic_from_pairs(inner_rules)?;
                assignments.push(Box::new(Assignment::new(path, query)));
            }
            Rule::deletion => {
                let mut inner_rules = pair.into_inner();
                let path = path_from_pair(inner_rules.next().unwrap())?;
                assignments.push(Box::new(Deletion::new(path)));
            }
            _ => (),
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
            match err.variant {
                ErrorVariant::ParsingError {
                    ref mut positives,
                    negatives: _,
                } => {
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
                _ => (),
            }
            err = err.renamed_rules(|rule| match *rule {
                Rule::arithmetic_operator_product => "operator".to_owned(),
                _ => format!("{:?}", rule),
            });
            return Err(format!("mapping parse error\n{}", err));
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn check_parser_errors() {
        let cases = vec![
            (
                ".foo = {\"bar\"}",
                r###"mapping parse error
 --> 1:8
  |
1 | .foo = {"bar"}
  |        ^---
  |
  = expected query"###,
            ),
            (
                ". = \"bar\"",
                r###"mapping parse error
 --> 1:1
  |
1 | . = "bar"
  | ^---
  |
  = expected target_path or deletion"###,
            ),
            (
                "foo = \"bar\"",
                r###"mapping parse error
 --> 1:1
  |
1 | foo = "bar"
  | ^---
  |
  = expected target_path or deletion"###,
            ),
            (
                ".foo.bar = \"baz\" and this",
                r###"mapping parse error
 --> 1:18
  |
1 | .foo.bar = "baz" and this
  |                  ^---
  |
  = expected EOI or operator"###,
            ),
            (
                ".foo.bar = \"baz\" + ",
                r###"mapping parse error
 --> 1:20
  |
1 | .foo.bar = "baz" + 
  |                    ^---
  |
  = expected query"###,
            ),
            (
                ".foo.bar = .foo.(bar |)",
                r###"mapping parse error
 --> 1:23
  |
1 | .foo.bar = .foo.(bar |)
  |                       ^---
  |
  = expected path_segment"###,
            ),
        ];

        for (mapping, exp) in cases {
            assert_eq!(
                format!("{}", parse(mapping).err().unwrap()),
                exp,
                "mapping: {}",
                mapping
            );
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
        ];

        for (mapping, exp) in cases {
            match parse(mapping) {
                Ok(p) => assert_eq!(format!("{:?}", p), format!("{:?}", exp), "{}", mapping),
                Err(e) => panic!("{}, mapping: {}", e, mapping),
            }
        }
    }
}
