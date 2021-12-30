use std::{
    collections::{BTreeMap, HashMap},
    convert::TryFrom,
    fmt::Write,
    sync::Arc,
};

use grok::Grok;
use itertools::{Itertools, Position};
use lazy_static::lazy_static;
use lookup::LookupBuf;
use regex::Regex;
use vrl_compiler::Value;

use crate::{
    ast::{self, Destination, GrokPattern},
    grok_filter::GrokFilter,
    matchers::{date, date::DateFilter},
    parse_grok_pattern::parse_grok_pattern,
};
use onig::EncodedChars;

#[derive(Debug, Clone)]
pub struct GrokRule {
    pub pattern: Arc<grok::Pattern>,
    pub fields: HashMap<String, GrokField>,
}

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("failed to parse grok expression '{}': {}", .0, .1)]
    InvalidGrokExpression(String, String),
    #[error("invalid arguments for the function '{}'", .0)]
    InvalidFunctionArguments(String),
    #[error("unknown filter '{}'", .0)]
    UnknownFilter(String),
    #[error("Circular dependency found in the alias '{}'", .0)]
    CircularDependencyInAliasDefinition(String),
}

///
/// Parses DD grok rules.
///
/// Here is an example:
/// patterns:
///  %{access.common} \[%{_date_access}\] "(?>%{_method} |)%{_url}(?> %{_version}|)" %{_status_code} (?>%{_bytes_written}|-)
///  %{access.common} (%{number:duration:scale(1000000000)} )?"%{_referer}" "%{_user_agent}"( "%{_x_forwarded_for}")?.*"#
/// aliases:
///  "access.common" : %{_client_ip} %{_ident} %{_auth}
///
/// You can write grok patterns with the %{MATCHER:EXTRACT:FILTER} syntax:
/// - Matcher: A rule (possibly a reference to another token rule) that describes what to expect (number, word, notSpace, etc.)
/// - Extract (optional): An identifier representing the capture destination for the piece of text matched by the Matcher.
/// - Filter (optional): A post-processor of the match to transform it.
///
/// Rules can reference aliases as %{alias_name}, aliases can reference each other themselves, cross-references or circular dependencies are not allowed and result in an error.
/// Only one can match any given log. The first one that matches, from top to bottom, is the one that does the parsing.
/// For further documentation and the full list of available matcher and filters check out https://docs.datadoghq.com/logs/processing/parsing
pub fn parse_grok_rules(
    patterns: &[String],
    aliases: BTreeMap<String, String>,
) -> Result<Vec<GrokRule>, Error> {
    let mut resolved_aliases: HashMap<String, String> = HashMap::new();
    let mut parsed_aliases_stack = vec![];

    for alias_name in aliases.keys() {
        resolve_aliases(
            alias_name,
            &aliases,
            &mut resolved_aliases,
            &mut parsed_aliases_stack,
        )?;
    }

    let mut grok = initialize_grok();

    patterns
        .iter()
        .filter(|&r| !r.is_empty())
        .map(|r| parse_pattern(r, &resolved_aliases, &mut grok))
        .collect::<Result<Vec<GrokRule>, Error>>()
}

/// The result of parsing grok rules - pure grok definitions, which can be feed directly to the grok,
/// and grok fields, that will be extracted, with their filters
#[derive(Debug, Clone)]
struct ParsedGrokRule {
    pub definition: String,
    pub fields: HashMap<String, GrokField>,
}

/// A grok field, that should be extracted, with its lookup path and post-processing filters to apply.
#[derive(Debug, Clone)]
pub struct GrokField {
    pub lookup: LookupBuf,
    pub filters: Vec<GrokFilter>,
}

/// Parses pattern definitions.
///
/// # Arguments
///
/// - `pattern` - the definition of the pattern
/// - `parsed_aliases` - aliases that have already been parsed
/// - `grok` - an instance of Grok parser
fn parse_pattern(
    pattern: &str,
    aliases: &HashMap<String, String>,
    grok: &mut Grok,
) -> Result<GrokRule, Error> {
    let parsed_pattern = parse_grok_rule(pattern, aliases)?;
    let mut pattern = String::new();
    pattern.push('^');
    pattern.push_str(parsed_pattern.definition.as_str());
    pattern.push('$');

    // compile pattern
    let pattern = Arc::new(
        grok.compile(&pattern, true)
            .map_err(|e| Error::InvalidGrokExpression(pattern, e.to_string()))?,
    );

    Ok(GrokRule {
        pattern,
        fields: parsed_pattern.fields,
    })
}

/// Replaces all references to other aliases with their definitions.
fn resolve_aliases(
    alias_name: &str,
    aliases: &BTreeMap<String, String>,
    resolved_aliases: &mut HashMap<String, String>,
    parsed_aliases_stack: &mut Vec<String>,
) -> Result<String, Error> {
    let definition = aliases
        .get(alias_name)
        .unwrap_or_else(|| panic!("{} is not an alias", alias_name));
    let raw_grok_patterns = find_grok_patterns(definition);
    let mut definition = definition.to_string();

    // resolve aliases
    for pattern in &raw_grok_patterns {
        let alias_name = &pattern[2..pattern.len() - 1];
        if aliases.get(alias_name).is_some() {
            let alias_definition =
                resolve_alias(alias_name, aliases, resolved_aliases, parsed_aliases_stack)?;
            definition = definition.replacen(pattern, &alias_definition, 1);
        }
    }
    resolved_aliases.insert(alias_name.to_string(), definition.to_string());
    Ok(definition)
}

fn find_grok_patterns(rule: &str) -> Vec<&str> {
    lazy_static! {
        static ref GROK_PATTERN_RE: onig::Regex =
            onig::Regex::new(r#"%\{(?:[^"\}]|(?<!\\)"(?:\\"|[^"])*(?<!\\)")+\}"#).unwrap();
    }
    // find all patterns %{}
    GROK_PATTERN_RE
        .find_iter(rule)
        .map(|(start, end)| &rule[start..end])
        .collect::<Vec<&str>>()
}

/// Parses a given rule to a pure grok pattern with a set of post-processing filters.
///
/// # Arguments
///
/// - `rule` - the definition of a grok rule(can be a pattern or an alias)
/// - `aliases` - all aliases and their definitions
/// - `parsed_aliases` - aliases that have already been parsed
/// - `parsed_aliases_stack` - names of the aliases that are being currently parsed(aliases can refer to other aliases) to catch circular dependencies
fn parse_grok_rule(rule: &str, aliases: &HashMap<String, String>) -> Result<ParsedGrokRule, Error> {
    let mut rule = rule.to_string();
    for (alias, def) in aliases {
        rule = rule.replacen(&format!("%{{{}}}", alias), def, 1);
    }
    // find all patterns %{}
    let raw_grok_patterns = find_grok_patterns(&rule);

    // parse them
    let mut grok_patterns = raw_grok_patterns
        .iter()
        .map(|pattern| {
            parse_grok_pattern(pattern)
                .map_err(|e| Error::InvalidGrokExpression(pattern.to_string(), e))
        })
        .collect::<Result<Vec<GrokPattern>, Error>>()?;

    let mut fields: HashMap<String, GrokField> = HashMap::new();
    grok_patterns = index_repeated_fields(grok_patterns);

    let pure_grok_patterns: Vec<String> = grok_patterns
        .iter()
        .map(|pattern| purify_grok_pattern(pattern, &mut fields))
        .collect::<Result<Vec<String>, Error>>()?;

    // replace grok patterns with "purified" ones
    let mut rule = rule.to_string();
    for (r, pure) in raw_grok_patterns.iter().zip(pure_grok_patterns.iter()) {
        rule = rule.replacen(r, pure.as_str(), 1);
    }

    // collect all filters to apply later
    for pattern in &grok_patterns {
        if let GrokPattern {
            destination: Some(destination),
            ..
        } = pattern
        {
            match &destination {
                Destination {
                    filter_fn: Some(filter),
                    path,
                } => {
                    let filter = GrokFilter::try_from(filter)?;
                    fields
                        .entry(destination.to_grok_field_name())
                        .and_modify(|v| v.filters.push(filter.clone()))
                        .or_insert_with(|| GrokField {
                            lookup: path.clone(),
                            filters: vec![filter.clone()],
                        });
                }
                Destination {
                    filter_fn: None,
                    path,
                } => {
                    fields.entry(path.to_string()).or_insert_with(|| GrokField {
                        lookup: path.clone(),
                        filters: vec![],
                    });
                }
            }
        }
    }

    Ok(ParsedGrokRule {
        definition: rule,
        fields,
    })
}

/// Replaces repeated field names with indexed versions, e.g. : field.name, field.name -> field.name.0, field.name.1 to avoid collisions in grok.
fn index_repeated_fields(grok_patterns: Vec<GrokPattern>) -> Vec<GrokPattern> {
    grok_patterns
        .iter()
        // group-by is a bit suboptimal with extra cloning, but acceptable since parsing usually happens only once
        .group_by(|pattern| pattern.destination.as_ref().map(|d| d.path.clone()))
        .into_iter()
        .flat_map(|(path, patterns)| match path {
            Some(path) => patterns
                .with_position()
                .enumerate()
                .map(|(i, pattern)| match pattern {
                    Position::First(pattern)
                    | Position::Middle(pattern)
                    | Position::Last(pattern) => {
                        let mut indexed_path = path.clone();
                        indexed_path.push_back(i as isize);
                        GrokPattern {
                            match_fn: pattern.match_fn.clone(),
                            destination: Some(Destination {
                                path: indexed_path,
                                filter_fn: pattern.destination.as_ref().unwrap().filter_fn.clone(),
                            }),
                        }
                    }
                    Position::Only(pattern) => pattern.to_owned(),
                })
                .collect::<Vec<_>>(),
            None => patterns.map(|p| p.to_owned()).collect::<Vec<_>>(),
        })
        .collect::<Vec<_>>()
}

/// Returns a resolved definition of the alias(with all alias references replaced).
fn resolve_alias(
    alias_name: &str,
    aliases: &BTreeMap<String, String>,
    resolved_aliases: &mut HashMap<String, String>,
    parsed_aliases_stack: &mut Vec<String>,
) -> Result<String, Error> {
    // track circular dependencies
    if parsed_aliases_stack.iter().any(|a| a == alias_name) {
        return Err(Error::CircularDependencyInAliasDefinition(
            parsed_aliases_stack.first().unwrap().to_string(),
        ));
    } else {
        parsed_aliases_stack.push(alias_name.to_string());
    }

    let alias_def = match resolved_aliases.get(alias_name) {
        None => {
            // this alias is not parsed yet - let's parse it
            resolve_aliases(alias_name, aliases, resolved_aliases, parsed_aliases_stack)?
        }
        Some(definition) => definition.to_string(),
    };

    parsed_aliases_stack.pop();

    Ok(alias_def)
}

/// Converts each rule to a pure grok rule:
///  - strips filters and collects them to apply later
///  - replaces match functions with corresponding regex groups.
///
/// # Arguments
///
/// - `pattern` - a parsed grok pattern
/// - `fields` - grok fields with their filters
fn purify_grok_pattern(
    pattern: &GrokPattern,
    fields: &mut HashMap<String, GrokField>,
) -> Result<String, Error> {
    let mut res = String::new();
    match pattern.match_fn.name.as_str() {
        "regex" | "date" | "boolean" => {
            // these patterns will be converted to named capture groups e.g. (?<http.status_code>[0-9]{3})
            if let Some(destination) = &pattern.destination {
                res.push_str("(?<");
                res.push_str(destination.to_grok_field_name().as_str());
                res.push('>');
            } else {
                res.push_str("(?:"); // non-capturing group
            }
            res.push_str(resolves_match_function(fields, pattern)?.as_str());
            res.push(')');
        }
        _ => {
            // these will be converted to "pure" grok patterns %{PATTERN:DESTINATION} but without filters
            res.push_str("%{");

            res.push_str(resolves_match_function(fields, pattern)?.as_str());

            if let Some(destination) = &pattern.destination {
                if destination.path.is_empty() {
                    write!(res, r#":."#).unwrap(); // root
                } else {
                    write!(res, ":{}", destination.path).unwrap();
                }
            }
            res.push('}');
        }
    }
    Ok(res)
}

impl Destination {
    fn to_grok_field_name(&self) -> String {
        match &self.path {
            p if p.is_empty() => ".".to_string(), // grok does not support empty field names,
            p => p.to_string(),
        }
    }
}

/// Process a match function from a given pattern:
/// - returns a grok expression(a grok pattern or a regular expression) corresponding to a given match function
/// - some match functions(e.g. number) implicitly introduce a filter to be applied to an extracted value - stores it to `fields`.
fn resolves_match_function(
    fields: &mut HashMap<String, GrokField>,
    pattern: &ast::GrokPattern,
) -> Result<String, Error> {
    let match_fn = &pattern.match_fn;
    let result = match match_fn.name.as_ref() {
        "regex" => match match_fn.args.as_ref() {
            Some(args) if !args.is_empty() => {
                if let ast::FunctionArgument::Arg(Value::Bytes(ref b)) = args[0] {
                    return Ok(String::from_utf8_lossy(b).to_string());
                }
                Err(Error::InvalidFunctionArguments(match_fn.name.clone()))
            }
            _ => Err(Error::InvalidFunctionArguments(match_fn.name.clone())),
        },
        "integer" => {
            if let Some(destination) = &pattern.destination {
                fields.insert(
                    destination.to_grok_field_name(),
                    GrokField {
                        lookup: destination.path.clone(),
                        filters: vec![GrokFilter::Integer],
                    },
                );
            }
            Ok("integerStr".to_string())
        }
        "integerExt" => {
            if let Some(destination) = &pattern.destination {
                fields.insert(
                    destination.to_grok_field_name(),
                    GrokField {
                        lookup: destination.path.clone(),
                        filters: vec![GrokFilter::IntegerExt],
                    },
                );
            }
            Ok("integerExtStr".to_string())
        }
        "number" => {
            if let Some(destination) = &pattern.destination {
                fields.insert(
                    destination.to_grok_field_name(),
                    GrokField {
                        lookup: destination.path.clone(),
                        filters: vec![GrokFilter::Number],
                    },
                );
            }
            Ok("numberStr".to_string())
        }
        "numberExt" => {
            if let Some(destination) = &pattern.destination {
                fields.insert(
                    destination.to_grok_field_name(),
                    GrokField {
                        lookup: destination.path.clone(),
                        filters: vec![GrokFilter::NumberExt],
                    },
                );
            }
            Ok("numberExtStr".to_string())
        }
        "date" => {
            return match match_fn.args.as_ref() {
                Some(args) if !args.is_empty() && args.len() <= 2 => {
                    if let ast::FunctionArgument::Arg(Value::Bytes(b)) = &args[0] {
                        let format = String::from_utf8_lossy(b);
                        let result = date::time_format_to_regex(&format, true)
                            .map_err(|_e| Error::InvalidFunctionArguments(match_fn.name.clone()))?;
                        let mut regext_opt = None;
                        if result.tz_captured {
                            regext_opt = Some(Regex::new(&result.regex).map_err(|error| {
                                error!(message = "Error compiling regex", regex = %result.regex, %error);
                                Error::InvalidFunctionArguments(match_fn.name.clone())
                            })?);
                        }
                        let strp_format = date::convert_time_format(&format).map_err(|error| {
                            error!(message = "Error compiling regex", regex = %result.regex, %error);
                            Error::InvalidFunctionArguments(match_fn.name.clone())
                        })?;
                        let mut target_tz = None;
                        if args.len() == 2 {
                            if let ast::FunctionArgument::Arg(Value::Bytes(b)) = &args[1] {
                                let tz = String::from_utf8_lossy(b);
                                date::parse_timezone(&tz).map_err(|error| {
                                    error!(message = "Invalid(unrecognized) timezone", %error);
                                    Error::InvalidFunctionArguments(match_fn.name.clone())
                                })?;
                                target_tz = Some(tz.to_string());
                            }
                        }
                        let filter = GrokFilter::Date(DateFilter {
                            original_format: format.to_string(),
                            strp_format,
                            regex_with_tz: regext_opt,
                            target_tz,
                            tz_aware: result.with_tz,
                        });
                        let result =
                            date::time_format_to_regex(&format, false).map_err(|error| {
                                error!(message = "Invalid time format", format = %format, %error);
                                Error::InvalidFunctionArguments(match_fn.name.clone())
                            })?;
                        if let Some(destination) = &pattern.destination {
                            fields.insert(
                                destination.to_grok_field_name(),
                                GrokField {
                                    lookup: destination.path.clone(),
                                    filters: vec![filter],
                                },
                            );
                        }
                        return Ok(result.regex);
                    }
                    Err(Error::InvalidFunctionArguments(match_fn.name.clone()))
                }
                _ => Err(Error::InvalidFunctionArguments(match_fn.name.clone())),
            };
        }
        // otherwise just add it as is, it should be a known grok pattern
        grok_pattern_name => Ok(grok_pattern_name.to_string()),
    };
    result
}

// test some tricky cases here, more high-level tests are in parse_grok
#[cfg(test)]
mod tests {
    use shared::btreemap;

    use super::*;

    #[test]
    fn supports_escaped_quotes() {
        let rules = parse_grok_rules(
            &[r#"%{notSpace:field:nullIf("with \"escaped\" quotes")}"#.to_string()],
            btreemap! {},
        )
        .expect("couldn't parse rules");
        assert!(matches!(
            &rules[0]
                .fields
                .get("field")
                .expect("invalid grok pattern")
            .filters[0],
            GrokFilter::NullIf(v) if *v == r#"with "escaped" quotes"#
        ));
    }
}

include!(concat!(env!("OUT_DIR"), "/patterns.rs"));
fn initialize_grok() -> Grok {
    let mut grok = grok::Grok::with_patterns();

    // Insert Datadog grok patterns.
    for &(key, value) in PATTERNS {
        grok.insert_definition(String::from(key), String::from(value));
    }
    grok
}
