extern crate pest;

use crate::{
    event::Value,
    mapping::{
        query,
        query::{path::Path as QueryPath, Literal},
        Assignment, Deletion, Function, Mapping, Result,
    },
};

use pest::{
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
                let query = query_from_pair(inner_rules.next().unwrap())?;
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
        Err(err) => Err(format!("mapping parse error\n{}", err)),
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
  = expected dot_path, boolean, null, string, or number"###,
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
  = expected EOI"###,
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
                ".foo = .(bar | baz)",
                Mapping::new(vec![Box::new(Assignment::new(
                    "foo".to_string(),
                    Box::new(QueryPath::from(vec![vec!["bar", "baz"]])),
                ))]),
            ),
            (
                ".foo = .foo.(bar | baz)",
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
        ];

        for (mapping, exp) in cases {
            match parse(mapping) {
                Ok(p) => assert_eq!(format!("{:?}", p), format!("{:?}", exp), "{}", mapping),
                Err(e) => panic!("{}, mapping: {}", e, mapping),
            }
        }
    }
}
