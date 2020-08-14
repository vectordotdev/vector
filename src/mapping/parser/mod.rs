extern crate pest;

use crate::{
    event::Value,
    mapping::{
        query,
        query::{arithmetic::Arithmetic, arithmetic::Operator, path::Path as QueryPath, Literal},
        Assignment, Deletion, Function, IfStatement, Mapping, Noop, OnlyFields, Result,
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

fn query_arithmetic_from_pair(pair: Pair<Rule>) -> Result<Box<dyn query::Function>> {
    query_arithmetic_boolean_from_pairs(pair.into_inner())
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
            let v = pair.as_str() == "true";
            Box::new(Literal::from(Value::from(v)))
        }
        Rule::dot_path => Box::new(QueryPath::from(path_segments_from_pair(pair)?)),
        Rule::group => query_arithmetic_from_pair(pair.into_inner().next().unwrap())?,
        _ => unreachable!(),
    })
}

fn if_statement_from_pairs(mut pairs: Pairs<Rule>) -> Result<Box<dyn Function>> {
    let query = query_arithmetic_from_pair(pairs.next().unwrap())?;

    let first = statement_from_pair(pairs.next().unwrap())?;

    let second = if let Some(pair) = pairs.next() {
        statement_from_pair(pair)?
    } else {
        Box::new(Noop {})
    };

    Ok(Box::new(IfStatement::new(query, first, second)))
}

fn function_from_pair(pair: Pair<Rule>) -> Result<Box<dyn Function>> {
    match pair.as_rule() {
        Rule::deletion => {
            let path = path_from_pair(pair.into_inner().next().unwrap())?;
            Ok(Box::new(Deletion::new(path)))
        }
        Rule::only_fields => {
            let mut paths = Vec::new();
            for pair in pair.into_inner() {
                paths.push(path_from_pair(pair)?);
            }
            Ok(Box::new(OnlyFields::new(paths)))
        }
        _ => unreachable!(),
    }
}

fn statement_from_pair(pair: Pair<Rule>) -> Result<Box<dyn Function>> {
    match pair.as_rule() {
        Rule::assignment => {
            let mut inner_rules = pair.into_inner();
            let path = path_from_pair(inner_rules.next().unwrap())?;
            let query = query_arithmetic_from_pair(inner_rules.next().unwrap())?;
            Ok(Box::new(Assignment::new(path, query)))
        }
        Rule::function => function_from_pair(pair.into_inner().next().unwrap()),
        Rule::if_statement => if_statement_from_pairs(pair.into_inner()),
        _ => unreachable!(),
    }
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
            _ => unreachable!(),
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
mod test {
    use super::*;

    #[test]
    fn check_parser_errors() {
        let cases = vec![
            (".foo = {\"bar\"}", vec![" 1:8\n", "= expected query"]),
            (
                ". = \"bar\"",
                vec![
                    " 1:1\n",
                    "= expected if_statement, target_path, or function",
                ],
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
                vec![" 1:48\n", "= expected operator"],
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
            (
                "del(.foo)",
                Mapping::new(vec![Box::new(Deletion::new("foo".to_string()))]),
            ),
            (
                "del(.foo)\ndel(.bar.baz)",
                Mapping::new(vec![
                    Box::new(Deletion::new("foo".to_string())),
                    Box::new(Deletion::new("bar.baz".to_string())),
                ]),
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
                    Box::new(Deletion::new("buz".to_string())),
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
        ];

        for (mapping, exp) in cases {
            match parse(mapping) {
                Ok(p) => assert_eq!(format!("{:?}", p), format!("{:?}", exp), "{}", mapping),
                Err(e) => panic!("{}, mapping: {}", e, mapping),
            }
        }
    }
}
