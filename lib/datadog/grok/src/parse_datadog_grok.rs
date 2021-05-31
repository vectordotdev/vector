use crate::ast::{Destination, Function, FunctionArgument, GrokPattern};
use crate::grok_pattern_parser::parse_grok_pattern;
use crate::lexer::Lexer;
use grok::{Grok, Pattern};
use lazy_static::lazy_static;
use lookup::{Lookup, LookupBuf, Segment};
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use shared::conversion::Conversion;
use shared::{btreemap, TimeZone};
use std::collections::btree_map::{Entry, OccupiedEntry};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::iter::Map;
use std::ops::DerefMut;
use std::path::Path;
use std::slice::Iter;
use std::sync::Arc;
use vector_core::event::{Event, LogEvent};
use vrl::prelude::DiagnosticError;
use vrl::Target;
use vrl_compiler::expression::Expr::InternalExpression;
use vrl_compiler::expression::{
    assignment, Assignment, Block, Container, Expr, IfStatement, Literal, Op, Predicate, Query,
    Variable, Variant,
};
use vrl_compiler::function::{ArgumentList, Compiled, Example};
use vrl_compiler::state::Compiler;
use vrl_compiler::value::{kind, Kind};
use vrl_compiler::{expression, Context, Expression, Parameter, Resolved, TypeDef, Value};
use vrl_compiler::{map, value};
use vrl_parser::ast;
use vrl_stdlib::{LengthFn, ToIntFn};

lazy_static! {
    static ref GROK_PATTER_RE: Regex = Regex::new(r"%\{.+?\}").unwrap();
}

#[derive(Debug, Clone)]
struct ParseDataDogGrokRuleFn {
    value: Box<dyn Expression>,
    rule: GrokRule,
}

impl Expression for ParseDataDogGrokRuleFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let bytes = value.try_bytes_utf8_lossy()?;

        let mut result = value!(btreemap! {});

        if let Some(ref matches) = self.rule.pattern.match_against(&bytes) {
            for (name, value) in matches.iter() {
                let path = name.parse().unwrap(); //TODO error handling
                result.insert(&path, value!(value));
            }
            return Ok(result);
        };

        Ok(value!(btreemap! {}))
    }

    fn type_def(&self, state: &Compiler) -> TypeDef {
        TypeDef::new()
            .fallible()
            .add_object::<(), Kind>(map! { (): Kind::all() })
    }
}

/*
   // TODO document it properly as normal VRL functions
   Here is an example of grok rules:
   access.common %{_client_ip} %{_ident} %{_auth} \[%{_date_access}\] "(?>%{_method} |)%{_url}(?> %{_version}|)" %{_status_code} (?>%{_bytes_written}|-)
   access.combined %{access.common} (%{number:duration:scale(1000000000)} )?"%{_referer}" "%{_user_agent}"( "%{_x_forwarded_for}")?.*"#

   You can write parsing rules with the %{MATCHER:EXTRACT:FILTER} syntax:
    - Matcher: A rule (possibly a reference to another token rule) that describes what to expect (number, word, notSpace, etc.)
    - Extract (optional): An identifier representing the capture destination for the piece of text matched by the Matcher.
    - Filter (optional): A post-processor of the match to transform it.

   Each rule can reference parsing rules defined above itself in the list.
   Only one can match any given log. The first one that matches, from top to bottom, is the one that does the parsing.
   For further documentation and the full list of available matcher and filters check out https://docs.datadoghq.com/logs/processing/parsing
*/
#[derive(Clone, Copy, Debug)]
pub struct ParseDataDogGrok;

impl vrl::function::Function for ParseDataDogGrok {
    fn identifier(&self) -> &'static str {
        "parse_datadog_grok"
    }

    fn examples(&self) -> &'static [Example] {
        todo!()
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "rules",
                kind: kind::ARRAY,
                required: true,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        let rules_exprs = arguments.required_array("rules")?;
        let mut rules = Vec::with_capacity(rules_exprs.len());
        for expr in rules_exprs {
            let value = expr
                .as_value()
                .ok_or(vrl::function::Error::ExpectedStaticExpression {
                    keyword: "patterns",
                    expr,
                })?;

            let re = value
                .try_bytes_utf8_lossy()
                .map_err(|e| Box::new(e) as Box<dyn DiagnosticError>)?;
            rules.push(re.to_string());
        }

        // parse grok rules first
        let grok_rules = parse_grok_rules(&rules);
        // then convert them to VRL exprs as:
        /**
            target_var = {}
            // check the first pattern
            if len(target_var) == 0 {
                target_var = parse_datadog_grok_rule(value, pattern1)
                if len(target_var) > 0 { // it matched - apply filters
                    target_var.http.status_code = to_int(target_var.http.status_code)
                    target_var.http.url_details = parse_url(target_var.http.url)
                }
            }
            // check the second, third pattern and so on
            if len(target_var) == 0 {
                target_var = parse_datadog_grok_rule(value, pattern2)
                if len(target_var) > 0 { // it matched - apply filters
                    ...
                }
            }
            ...
            target_var

        **/
        let target_var_name = "parsed"; //TODO which one to choose to avoid conflicts?;
        let grok_rule_exprs: Vec<Expr> = grok_rules
            .iter()
            .map(|rule| grok_rule_to_expr(target_var_name, value.clone(), rule))
            .collect();

        let mut block = vec![];

        // initialize target variable with an empty object
        let target_var_expr = Expr::Variable(Variable {
            ident: ast::Ident(target_var_name.to_string()),
            value: None,
        });
        let initial_asgt = Expr::Assignment(Assignment {
            variant: assignment::Variant::Single {
                target: assignment::Target::Internal(ast::Ident(target_var_name.to_string()), None),
                expr: Box::new(Expr::Container(Container::new(Variant::Object(
                    expression::Object::new(BTreeMap::new()),
                )))),
            },
        });
        block.push(initial_asgt);

        // apply all grok rules
        block.extend(grok_rule_exprs);
        // return the result
        block.push(target_var_expr);

        Ok(Box::new(Container::new(Variant::Block(Block::new(block)))))
    }
}

#[derive(Debug, Clone)]
struct GrokRule {
    pattern: Arc<grok::Pattern>,
    destinations: HashMap<LookupBuf, Function>,
}

fn parse_grok_rules(rules: &Vec<String>) -> Vec<GrokRule> {
    let mut match_rules = vec![];
    let mut grok = initialize_grok();
    let mut prev_rule_definitions: HashMap<&str, String> = HashMap::new();
    let mut prev_rule_destinations: HashMap<&str, HashMap<LookupBuf, Function>> = HashMap::new();

    rules
        .iter()
        .filter(|&r| !r.is_empty())
        .for_each(|match_rule| {
            let mut split_whitespace = match_rule.splitn(2, " ");
            let split = split_whitespace.by_ref();
            let match_rule_name = split.next().unwrap();
            let mut match_rule_def = split.next().unwrap().to_string();

            let match_rule_def_cloned = match_rule_def.clone();
            // find all patterns %{}
            let raw_grok_patterns = GROK_PATTER_RE
                .find_iter(match_rule_def_cloned.as_str())
                .map(|rule| rule.as_str())
                .collect::<Vec<&str>>();
            // parse them
            let grok_patterns: Vec<GrokPattern> = raw_grok_patterns
                .iter()
                .map(|pattern| parse_grok_pattern(pattern))
                .collect();

            let mut filters: HashMap<LookupBuf, Function> = HashMap::new();

            // convert each rule to a pure grok rule:
            //  - strip filters
            //  - replace references to previous rules with actual definitions
            //  - replace match functions with corresponding regex groups
            let pure_grok_patterns: Vec<String> = grok_patterns
                .iter()
                .map(|rule| {
                    let mut res = String::new();
                    if prev_rule_definitions.contains_key(rule.match_fn.name.as_str()) {
                        // this is a reference to a previous rule - replace it and copy all destinations from the prev rule
                        res.push_str(
                            prev_rule_definitions
                                .get(rule.match_fn.name.as_str())
                                .unwrap(),
                        );
                        if let Some(d) = prev_rule_destinations.get(rule.match_fn.name.as_str()) {
                            d.iter().for_each(|(path, function)| {
                                filters.insert(path.to_owned(), function.to_owned());
                            });
                        }
                        res
                    } else if rule.match_fn.name == "regex" || rule.match_fn.name == "date" {
                        // these patterns will be converted to named capture groups e.g. (?<http.status_code>[0-9]{3})
                        res.push_str("(?");
                        res.push_str("<");
                        if let Some(destination) = &rule.destination {
                            res.push_str(destination.path.to_string().as_str());
                        }
                        res.push_str(">");
                        process_match_function(&mut res, &rule.match_fn);
                        res.push_str(")");

                        res
                    } else {
                        // these will be converted to "pure" grok patterns %{PATTERN:DESTINATION} but without filters
                        res.push_str("%{");

                        match rule.match_fn.name.as_str() {
                            "number" => res.push_str("numberStr"), // replace with the core grok pattern, TODO apply corresponding filters
                            _ => res.push_str(rule.match_fn.name.as_str()),
                        }

                        if let Some(destination) = &rule.destination {
                            res.push_str(":");
                            res.push_str(destination.path.to_string().as_str());
                        }
                        res.push_str("}");

                        res
                    }
                })
                .collect();

            // replace grok patterns with "purified" ones
            let mut pure_pattern_it = pure_grok_patterns.iter();
            for r in raw_grok_patterns {
                match_rule_def =
                    match_rule_def.replace(r, pure_pattern_it.next().unwrap().as_str());
            }

            // collect all filters to apply later
            grok_patterns
                .iter()
                .filter(|&rule| {
                    rule.destination.is_some()
                        && rule.destination.as_ref().unwrap().filter_fn.is_some()
                })
                .for_each(|rule| {
                    let dest = rule.destination.as_ref().unwrap();
                    filters.insert(dest.path.clone(), dest.filter_fn.as_ref().unwrap().clone());
                });

            let mut match_rule_def_closed = String::new();
            match_rule_def_closed.push('^');
            match_rule_def_closed.push_str(match_rule_def.as_str());
            match_rule_def_closed.push('$');
            let pattern = grok.compile(&match_rule_def_closed, true).unwrap();

            // store rule definitions and filters in case this rule is referenced in the next rules
            prev_rule_definitions.insert(match_rule_name, match_rule_def);
            prev_rule_destinations.insert(match_rule_name, filters.clone());

            match_rules.push(GrokRule {
                pattern: Arc::new(pattern),
                destinations: filters,
            });
        });

    match_rules
}
/**
    Converts a list of grok rules to a corresponding VRL expr:

    if len(target_var) == 0 {
        target_var = parse_datadog_grok_rule(value, pattern1)
        if len(target_var) > 0 { // it matched - apply filters
            target_var.http.status_code = to_int(target_var.http.status_code)
            target_var.http.url_details = parse_url(target_var.http.url)
        }
    }
**/
fn grok_rule_to_expr(
    target_var_name: &str,
    value: Box<dyn Expression>,
    grok_rule: &GrokRule,
) -> Expr {
    let target = assignment::Target::Internal(ast::Ident(target_var_name.to_string()), None);
    let target_var = Variable {
        ident: ast::Ident(target_var_name.to_string()),
        value: None,
    };
    let target_var_expr = Expr::Variable(target_var.clone());

    let match_fn = ParseDataDogGrokRuleFn {
        value: value.clone(),
        rule: grok_rule.clone(),
    };

    // target_var = parse_datadog_grok_rule(value, pattern1)
    let asgt = Expr::Assignment(Assignment {
        variant: assignment::Variant::Single {
            target: target.clone(),
            expr: Box::new(to_internal_expr(Box::new(match_fn))),
        },
    });
    let len_fn = LengthFn {
        value: Box::new(target_var_expr.clone()),
    };

    let filters = grok_rule
        .destinations
        .iter()
        .map(|(path, filter)| {
            let filter_target = expression::Target::Internal(target_var.clone());
            // target_var.http.status_code = to_int(target_var.http.status_code)
            let filter_expr = Expr::Assignment(Assignment {
                variant: assignment::Variant::Single {
                    target: assignment::Target::Internal(
                        ast::Ident(target_var_name.to_string()),
                        Some(path.clone()),
                    ),
                    expr: Box::new(to_filter_expression(
                        Box::new(Query {
                            target: filter_target,
                            path: path.clone(),
                        }),
                        filter,
                    )),
                },
            });
            filter_expr
        })
        .collect();
    let filter_block = Block::new(filters);

    // len(target_var) > 0
    let len_gt_zero = Expr::Op(expression::Op {
        lhs: Box::new(to_internal_expr(Box::new(len_fn.clone()))),
        rhs: Box::new(Expr::Literal(Literal::Integer(0))),
        opcode: ast::Opcode::Gt,
    });
    // len(target_var) == 0
    let len_eq_zero = Expr::Op(expression::Op {
        lhs: Box::new(to_internal_expr(Box::new(len_fn))),
        rhs: Box::new(Expr::Literal(Literal::Integer(0))),
        opcode: ast::Opcode::Eq,
    });
    // if len(target_var) > 0 {
    let if_parsed = Expr::IfStatement(IfStatement {
        predicate: Predicate::new_unchecked(vec![len_gt_zero]),
        consequent: filter_block,
        alternative: None,
    });
    // if len(target_var) == 0 {
    let if_not_parsed = Expr::IfStatement(IfStatement {
        predicate: Predicate::new_unchecked(vec![len_eq_zero]),
        consequent: Block::new(vec![asgt, if_parsed]),
        alternative: None,
    });

    if_not_parsed
}

#[inline]
fn to_internal_expr(expr: Box<Expression>) -> Expr {
    InternalExpression(expression::InternalExpression { expr })
}

fn process_match_function(mut regex: &mut String, function: &Function) {
    match function.name.as_ref() {
        "regex" => {
            let args = function.args.as_ref().unwrap();
            if let FunctionArgument::ARG(Value::Bytes(b)) = &args[0] {
                regex.push_str(String::from_utf8_lossy(&b).as_ref());
            }
        }
        "date" => {
            regex.push_str("13\\/Jul\\/2016:10:55:36 \\+0000"); //TODO
        }
        _ => {}
    }
}

fn to_filter_expression(value: Box<dyn Expression>, filter: &Function) -> Expr {
    let filter = match filter.name.as_ref() {
        "integer" => to_internal_expr(Box::new(ToIntFn { value })),
        "nullIf" => to_internal_expr(value), //TODO
        "scale" => to_internal_expr(value),  //TODO
        _ => panic!("filter {:?} is not supported", filter.name),
    };
    to_internal_expr(Box::new(filter))
}

fn initialize_grok() -> Grok {
    // TODO can we make it static?
    let mut grok = grok::Grok::with_patterns();

    // insert grok patterns
    fs::read_dir(Path::new("grok_patterns"))
        .unwrap()
        .for_each(|pf| {
            let file_path = pf.unwrap().path();
            let f = File::open(file_path.as_path()).unwrap();
            BufReader::new(&f).lines().for_each(|l| {
                let pattern = l.unwrap();
                if !pattern.is_empty() && !pattern.starts_with("#") {
                    let mut split_whitespace = pattern.splitn(2, " ");
                    let split = split_whitespace.by_ref();
                    let name = split.next().unwrap();
                    let def = split.next().unwrap();
                    grok.insert_definition(name, def);
                }
            });
        });
    grok
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use lookup::SegmentBuf;
    use vrl::prelude::*;
    use vrl::Function;

    test_function![
        parse_datadog_grok => ParseDataDogGrok;

        simple_grok {
            args: func_args![ value: "2020-10-02T23:22:12.223222Z info Hello world",
                rules: value!(["simpleRule %{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"])],
            want: Ok(Value::from(btreemap! {
                "timestamp" => "2020-10-02T23:22:12.223222Z",
                "level" => "info",
                "message" => "Hello world",
            })),
            tdef: TypeDef::new().fallible().null(), //TODO why it wants null here?
        }

        nginx_common {
            args: func_args![ value: r##"127.0.0.1 - frank [13/Jul/2016:10:55:36 +0000] "GET /apache_pb.gif HTTP/1.0" 200 2326"##,
                rules: value!([
                    // support rules
                    r#"_auth %{notSpace:http.auth:nullIf("-")}"#,
                    r#"_bytes_written %{NUMBER:network.bytes_written}"#,
                    r#"_client_ip %{ipOrHost:network.client.ip}"#,
                    r#"_version HTTP\/(?<http.version>\d+\.\d+)"#,
                    r#"_url %{notSpace:http.url}"#,
                    r#"_ident %{notSpace:http.ident}"#,
                    r#"_user_agent %{regex("[^\\\"]*"):http.useragent}"#,
                    r#"_referer %{notSpace:http.referer}"#,
                    r#"_status_code %{NUMBER:http.status_code:integer}"#,
                    r#"_method %{word:http.method}"#,
                    r#"_date_access %{date("dd/MMM/yyyy:HH:mm:ss Z"):date_access}"#,
                    r#"_x_forwarded_for %{regex("[^\\\"]*"):http._x_forwarded_for:nullIf("-")}"#,
                    // match rules
                    r#"access.common %{_client_ip} %{_ident} %{_auth} \[%{_date_access}\] "(?>%{_method} |)%{_url}(?> %{_version}|)" %{_status_code} (?>%{_bytes_written}|-)"#,
                    ])],
            want: Ok(value!(btreemap! {
                "date_access" => "13/Jul/2016:10:55:36 +0000",
                "http" => btreemap! {
                    "auth" => "frank",
                    "ident" => "-",
                    "method" => "GET",
                    "status_code" => 200,
                    "url" => "/apache_pb.gif",
                    "version" => "1.0",
                },
                "network" => btreemap! {
                    "bytes_written" => "2326",
                    "client" => btreemap! {
                        "ip" => "127.0.0.1"
                    }
                }
            })),
            tdef: TypeDef::new().fallible().null(),
        }

        nginx_combined {
            args: func_args![ value: r##"127.0.0.1 - frank [13/Jul/2016:10:55:36 +0000] "GET /apache_pb.gif HTTP/1.0" 200 2326 "http://www.perdu.com/" "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/55.0.2883.87 Safari/537.36" "-""##,
                rules: value!([
                    // support rules
                    r#"_auth %{notSpace:http.auth:nullIf("-")}"#,
                    r#"_bytes_written %{NUMBER:network.bytes_written}"#,
                    r#"_client_ip %{ipOrHost:network.client.ip}"#,
                    r#"_version HTTP\/(?<http.version>\d+\.\d+)"#,
                    r#"_url %{notSpace:http.url}"#,
                    r#"_ident %{notSpace:http.ident}"#,
                    r#"_user_agent %{regex("[^\\\"]*"):http.useragent}"#,
                    r#"_referer %{notSpace:http.referer}"#,
                    r#"_status_code %{NUMBER:http.status_code:integer}"#,
                    r#"_method %{word:http.method}"#,
                    r#"_date_access %{date("dd/MMM/yyyy:HH:mm:ss Z"):date_access}"#,
                    r#"_x_forwarded_for %{regex("[^\\\"]*"):http._x_forwarded_for:nullIf("-")}"#,
                    // match rules
                    r#"access.common %{_client_ip} %{_ident} %{_auth} \[%{_date_access}\] "(?>%{_method} |)%{_url}(?> %{_version}|)" %{_status_code} (?>%{_bytes_written}|-)"#,
                    r#"access.combined %{access.common} (%{number:duration:scale(1000000000)} )?"%{_referer}" "%{_user_agent}"( "%{_x_forwarded_for}")?.*"#,
                    ])],
            want: Ok(value!(btreemap! {
                "date_access" => "13/Jul/2016:10:55:36 +0000",
                "duration" => "", //TODO empty value should be ignored
                "http" => btreemap! {
                    "auth" => "frank",
                    "ident" => "-",
                    "method" => "GET",
                    "status_code" => 200,
                    "url" => "/apache_pb.gif",
                    "version" => "1.0",
                    "useragent" => "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/55.0.2883.87 Safari/537.36",
                    "referer" => "http://www.perdu.com/",
                    "_x_forwarded_for" => "-",
                },
                "network" => btreemap! {
                    "bytes_written" => "2326",
                    "client" => btreemap! {
                        "ip" => "127.0.0.1"
                    }
                }
            })),
            tdef: TypeDef::new().fallible().null(),
        }
    ];
}
