use std::collections::HashMap;

use lazy_static::lazy_static;
use regex::Regex;

use lookup::LookupBuf;

use crate::ast;
use crate::ast::{Destination, Function, FunctionArgument, GrokPattern};
use crate::grok_filter::GrokFilter;
use crate::parse_grok_pattern::parse_grok_pattern;
use grok::Grok;
use std::convert::TryFrom;
use std::sync::Arc;
use vector_core::event::Value;

#[derive(Debug, Clone)]
pub struct GrokRule {
    pub pattern: Arc<grok::Pattern>,
    pub filters: HashMap<LookupBuf, Vec<GrokFilter>>,
}

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("failed to parse grok expression '{}': {}", .0, .1)]
    InvalidGrokExpression(String, String),
    #[error("invalid arguments for the function '{}'", .0)]
    InvalidFunctionArguments(String),
    #[error("unknown filter '{}'", .0)]
    UnknownFilter(String),
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
    helper_rules: &Vec<String>,
    parsing_rules: &Vec<String>,
) -> Result<Vec<GrokRule>, Error> {
    let mut prev_rule_definitions: HashMap<&str, String> = HashMap::new();
    let mut prev_rule_filters: HashMap<&str, HashMap<LookupBuf, Vec<GrokFilter>>> = HashMap::new();
    let mut grok = initialize_grok();

    // parse support rules to reference them later in the match rules
    helper_rules
        .iter()
        .filter(|&r| !r.is_empty())
        .map(|r| {
            parse_grok_rule(
                r,
                &mut prev_rule_definitions,
                &mut prev_rule_filters,
                &mut grok,
            )
        })
        .collect::<Result<Vec<GrokRule>, Error>>()?;

    // parse match rules and return them
    let match_rules = parsing_rules
        .iter()
        .filter(|&r| !r.is_empty())
        .map(|r| {
            parse_grok_rule(
                r,
                &mut prev_rule_definitions,
                &mut prev_rule_filters,
                &mut grok,
            )
        })
        .collect::<Result<Vec<GrokRule>, Error>>()?;

    Ok(match_rules)
}

fn parse_grok_rule<'a>(
    rule: &'a String,
    mut prev_rule_patterns: &mut HashMap<&'a str, String>,
    mut prev_rule_filters: &mut HashMap<&'a str, HashMap<LookupBuf, Vec<GrokFilter>>>,
    grok: &mut Grok,
) -> Result<GrokRule, Error> {
    let mut split_whitespace = rule.splitn(2, " ");
    let split = split_whitespace.by_ref();
    let rule_name = split.next().ok_or(Error::InvalidGrokExpression(
        rule.clone(),
        "format must be: 'ruleName definition'".into(),
    ))?;
    let mut rule_def = split
        .next()
        .ok_or(Error::InvalidGrokExpression(
            rule.clone(),
            "format must be: 'ruleName definition'".into(),
        ))?
        .to_string();

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
            parse_grok_pattern(pattern)
                .map_err(|e| Error::InvalidGrokExpression(pattern.to_string(), e.to_string()))
        })
        .collect::<Result<Vec<GrokPattern>, Error>>()?;

    let mut filters: HashMap<LookupBuf, Vec<GrokFilter>> = HashMap::new();

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
        .map(|rule| {
            let dest = rule.destination.as_ref().unwrap();
            let filter = GrokFilter::try_from(dest.filter_fn.as_ref().unwrap())?;
            Ok((dest, filter))
        })
        .collect::<Result<Vec<_>, Error>>()?
        .iter()
        .for_each(|(dest, filter)| {
            filters
                .entry(dest.path.clone())
                .and_modify(|v| v.push(filter.clone()))
                .or_insert(vec![filter.clone()]);
        });

    let mut pattern = String::new();
    pattern.push('^');
    pattern.push_str(rule_def.as_str());
    pattern.push('$');

    // store rule definitions and filters in case this rule is referenced in the next rules
    prev_rule_patterns.insert(rule_name, rule_def);
    prev_rule_filters.insert(rule_name, filters.clone());

    let pattern = Arc::new(
        grok.compile(&pattern, true)
            .map_err(|e| Error::InvalidGrokExpression(pattern, e.to_string()))?,
    );

    Ok(GrokRule { pattern, filters })
}

/// Converts each rule to a pure grok rule:
///  - strips filters and "remembers" them to apply later
///  - replaces references to previous rules with actual definitions
///  - replaces match functions with corresponding regex groups
fn purify_grok_pattern(
    prev_rule_definitions: &mut HashMap<&str, String>,
    prev_rule_filters: &mut HashMap<&str, HashMap<LookupBuf, Vec<GrokFilter>>>,
    mut filters: &mut HashMap<LookupBuf, Vec<GrokFilter>>,
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
    filters: &mut HashMap<LookupBuf, Vec<GrokFilter>>,
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
            GrokFilter::Integer,
            filters,
            pattern,
        ),
        "integerExt" => replace_with_pattern_and_add_as_filter(
            "integerExtStr",
            GrokFilter::IntegerExt,
            filters,
            pattern,
        ),
        "number" => replace_with_pattern_and_add_as_filter(
            "numberStr",
            GrokFilter::Number,
            filters,
            pattern,
        ),
        "numberExt" => replace_with_pattern_and_add_as_filter(
            "numberExtStr",
            GrokFilter::NumberExt,
            filters,
            pattern,
        ),
        "boolean" => {
            if match_fn.args.is_some() {
                let args = match_fn.args.as_ref().unwrap();
                if args.len() == 2 {
                    if let ast::FunctionArgument::ARG(true_pattern) = &args[0] {
                        if let ast::FunctionArgument::ARG(false_pattern) = &args[1] {
                            let regex = Regex::new(
                                format!("^{}$", true_pattern.to_string_lossy()).as_str(),
                            )
                            .unwrap();
                            return replace_with_pattern_and_add_as_filter(
                                format!(
                                    "{}|{}",
                                    true_pattern.to_string_lossy(),
                                    false_pattern.to_string_lossy()
                                )
                                .as_str(),
                                GrokFilter::Boolean(Some(regex)),
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
                    GrokFilter::Boolean(None),
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
    filter: GrokFilter,
    filters: &mut HashMap<LookupBuf, Vec<GrokFilter>>,
    pattern: &ast::GrokPattern,
) -> Result<String, Error> {
    if let Some(destination) = &pattern.destination {
        filters.insert(destination.path.clone(), vec![filter]);
    }
    Ok(new_pattern.to_string())
}

include!(concat!(env!("OUT_DIR"), "/patterns.rs"));
fn initialize_grok() -> Grok {
    let mut grok = grok::Grok::with_patterns();

    // insert DataDog grok patterns
    for &(key, value) in PATTERNS {
        grok.insert_definition(String::from(key), String::from(value));
    }
    grok
}
