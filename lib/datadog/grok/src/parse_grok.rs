use std::collections::HashMap;

use lazy_static::lazy_static;
use regex::Regex;

use lookup::LookupBuf;

use crate::ast;
use crate::ast::{Function, GrokPattern};
use crate::parse_grok_pattern::parse_grok_pattern;
use vrl::Value;

#[derive(Debug, Clone)]
pub struct GrokRule {
    pub pattern: String,
    pub filters: HashMap<LookupBuf, Vec<ast::Function>>,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Failed to parse grok expression: '{}'", .0)]
    InvalidGrokExpression(String),
    #[error("Invalid arguments for the function '{}'", .0)]
    InvalidFunctionArguments(String),
    #[error("Unsupported filter '{}'", .0)]
    UnsupportedFilter(String),
}

/**
Parses DD grok rules.

Here is an example:
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
pub fn parse_grok_rules(
    support_rules: &Vec<String>,
    match_rules: &Vec<String>,
) -> Result<Vec<GrokRule>, Error> {
    let mut prev_rule_definitions: HashMap<&str, String> = HashMap::new();
    let mut prev_rule_destinations: HashMap<&str, HashMap<LookupBuf, Vec<ast::Function>>> =
        HashMap::new();

    // parse support rules to reference them later in the match rules
    support_rules
        .iter()
        .filter(|&r| !r.is_empty())
        .map(|r| parse_grok_rule(r, &mut prev_rule_definitions, &mut prev_rule_destinations))
        .collect::<Result<Vec<GrokRule>, Error>>()?;

    // parse match rules and return them
    let match_rules = match_rules
        .iter()
        .filter(|&r| !r.is_empty())
        .map(|r| parse_grok_rule(r, &mut prev_rule_definitions, &mut prev_rule_destinations))
        .collect::<Result<Vec<GrokRule>, Error>>()?;

    Ok(match_rules)
}

fn parse_grok_rule<'a>(
    rule: &'a String,
    mut prev_rule_patterns: &mut HashMap<&'a str, String>,
    mut prev_rule_filters: &mut HashMap<&'a str, HashMap<LookupBuf, Vec<Function>>>,
) -> Result<GrokRule, Error> {
    let mut split_whitespace = rule.splitn(2, " ");
    let split = split_whitespace.by_ref();
    let rule_name = split.next().unwrap();
    let mut rule_def = split.next().unwrap().to_string();

    let rule_def_cloned = rule_def.clone();
    lazy_static! {
        static ref GROK_PATTER_RE: Regex = Regex::new(r"%\{.+?\}").unwrap();
    }
    // find all patterns %{}
    let raw_grok_patterns = GROK_PATTER_RE
        .find_iter(rule_def_cloned.as_str())
        .map(|rule| rule.as_str())
        .collect::<Vec<&str>>();
    // parse them
    let grok_patterns = raw_grok_patterns
        .iter()
        .map(|pattern| {
            parse_grok_pattern(pattern).map_err(|e| Error::InvalidGrokExpression(e.to_string()))
        })
        .collect::<Result<Vec<GrokPattern>, Error>>()?;

    let mut filters: HashMap<LookupBuf, Vec<ast::Function>> = HashMap::new();

    let pure_grok_patterns: Vec<String> = grok_patterns
        .iter()
        .map(|rule| {
            purify_grok_pattern(
                &mut prev_rule_patterns,
                &mut prev_rule_filters,
                &mut filters,
                &rule,
            )
        })
        .collect::<Result<Vec<String>, Error>>()?;

    // replace grok patterns with "purified" ones
    let mut pure_pattern_it = pure_grok_patterns.iter();
    for r in raw_grok_patterns {
        rule_def = rule_def.replace(r, pure_pattern_it.next().unwrap().as_str());
    }

    // collect all filters to apply later
    grok_patterns
        .iter()
        .filter(|&rule| {
            rule.destination.is_some() && rule.destination.as_ref().unwrap().filter_fn.is_some()
        })
        .for_each(|rule| {
            let dest = rule.destination.as_ref().unwrap();
            filters
                .entry(dest.path.clone())
                .and_modify(|v| v.push(dest.filter_fn.as_ref().unwrap().clone()))
                .or_insert(vec![dest.filter_fn.as_ref().unwrap().clone()]);
        });

    let mut pattern = String::new();
    pattern.push('^');
    pattern.push_str(rule_def.as_str());
    pattern.push('$');

    // store rule definitions and filters in case this rule is referenced in the next rules
    prev_rule_patterns.insert(rule_name, rule_def);
    prev_rule_filters.insert(rule_name, filters.clone());

    Ok(GrokRule { pattern, filters })
}

/// Converts each rule to a pure grok rule:
///  - strips filters and "remembers" them to apply later
///  - replaces references to previous rules with actual definitions
///  - replaces match functions with corresponding regex groups
fn purify_grok_pattern(
    prev_rule_definitions: &mut HashMap<&str, String>,
    prev_rule_filters: &mut HashMap<&str, HashMap<LookupBuf, Vec<Function>>>,
    mut filters: &mut HashMap<LookupBuf, Vec<Function>>,
    rule: &GrokPattern,
) -> Result<String, Error> {
    let mut res = String::new();
    if prev_rule_definitions.contains_key(rule.match_fn.name.as_str()) {
        // this is a reference to a previous rule - replace it and copy all destinations from the prev rule
        res.push_str(
            prev_rule_definitions
                .get(rule.match_fn.name.as_str())
                .unwrap(),
        );
        if let Some(d) = prev_rule_filters.get(rule.match_fn.name.as_str()) {
            d.iter().for_each(|(path, function)| {
                filters.insert(path.to_owned(), function.to_owned());
            });
        }
        Ok(res)
    } else if rule.match_fn.name == "regex"
        || rule.match_fn.name == "date"
        || rule.match_fn.name == "boolean"
    {
        // these patterns will be converted to named capture groups e.g. (?<http.status_code>[0-9]{3})
        res.push_str("(?");
        res.push_str("<");
        if let Some(destination) = &rule.destination {
            res.push_str(destination.path.to_string().as_str());
        }
        res.push_str(">");
        res.push_str(process_match_function(&mut filters, &rule)?.as_str());
        res.push_str(")");

        Ok(res)
    } else {
        // these will be converted to "pure" grok patterns %{PATTERN:DESTINATION} but without filters
        res.push_str("%{");

        res.push_str(process_match_function(&mut filters, &rule)?.as_str());

        if let Some(destination) = &rule.destination {
            res.push_str(":");
            res.push_str(destination.path.to_string().as_str());
        }
        res.push_str("}");

        Ok(res)
    }
}

fn process_match_function(
    filters: &mut HashMap<LookupBuf, Vec<Function>>,
    pattern: &ast::GrokPattern,
) -> Result<String, Error> {
    let match_fn = &pattern.match_fn;
    let result = match match_fn.name.as_ref() {
        "regex" => {
            if match_fn.args.is_some() {
                if let ast::FunctionArgument::ARG(Value::Bytes(b)) =
                    &match_fn.args.as_ref().unwrap()[0]
                {
                    return Ok(String::from_utf8_lossy(&b).to_string());
                }
            }
            Err(Error::InvalidFunctionArguments(match_fn.name.clone()))
        }
        "date" => Ok("13\\/Jul\\/2016:10:55:36 \\+0000".to_string()), // TODO in a follow-up PR
        "integer" => replace_with_pattern_and_add_as_filter(
            "integerStr",
            Function::new("integer"),
            filters,
            pattern,
        ),
        "integerExt" => replace_with_pattern_and_add_as_filter(
            "integerExtStr",
            Function::new("integerExt"),
            filters,
            pattern,
        ),
        "number" => replace_with_pattern_and_add_as_filter(
            "numberStr",
            Function::new("number"),
            filters,
            pattern,
        ),
        "numberExt" => replace_with_pattern_and_add_as_filter(
            "numberExtStr",
            Function::new("numberExt"),
            filters,
            pattern,
        ),
        "boolean" => {
            if match_fn.args.is_some() {
                let args = match_fn.args.as_ref().unwrap();
                if args.len() == 2 {
                    if let ast::FunctionArgument::ARG(true_pattern) = &args[0] {
                        if let ast::FunctionArgument::ARG(false_pattern) = &args[1] {
                            return replace_with_pattern_and_add_as_filter(
                                format!(
                                    "{}|{}",
                                    true_pattern.try_bytes_utf8_lossy().map_err(|_| {
                                        Error::InvalidFunctionArguments(match_fn.name.clone())
                                    })?,
                                    false_pattern.try_bytes_utf8_lossy().map_err(|_| {
                                        Error::InvalidFunctionArguments(match_fn.name.clone())
                                    })?
                                )
                                .as_str(),
                                match_fn.clone(),
                                filters,
                                pattern,
                            );
                        }
                    }
                }
                Err(Error::InvalidFunctionArguments(match_fn.name.clone()))
            } else {
                replace_with_pattern_and_add_as_filter(
                    "[Tt][Rr][Uu][Ee]|[Ff][Aa][Ll][Ss][Ee]",
                    match_fn.clone(),
                    filters,
                    pattern,
                )
            }
        }
        // otherwise just add it as is, it should be a known grok pattern
        grok_pattern_name => Ok(grok_pattern_name.to_string()),
    };
    result
}

fn replace_with_pattern_and_add_as_filter(
    new_pattern: &str,
    filter: Function,
    filters: &mut HashMap<LookupBuf, Vec<Function>>,
    pattern: &ast::GrokPattern,
) -> Result<String, Error> {
    if let Some(destination) = &pattern.destination {
        filters.insert(destination.path.clone(), vec![filter]);
    }
    Ok(new_pattern.to_string())
}
