// This file modified from
// https://github.com/daschl/grok/blob/1c958207c7e60a776752f1343f82c25c3c704a34/src/lib.rs
// under the terms of the Apache 2.0 license.
//
// There isn't a whole lot going on here. The underlying regex engine must have
// backtracking and look-ahead and what _this_ bit of code does is include the
// ability to 'alias' named capture groups, import some default patterns.

include!(concat!(env!("OUT_DIR"), "/patterns.rs"));

use onig::{Captures, Regex};
use std::collections::{btree_map, BTreeMap};
use std::sync::Arc;
use thiserror::Error;

const MAX_RECURSION: usize = 1024;

const GROK_PATTERN: &str = r"%\{(?<name>(?<pattern>[A-z0-9]+)(?::(?<alias>[A-z0-9_:;\/\s\.]+))?)(?:=(?<definition>(?:(?:[^{}]+|\.+)+)+))?\}";
const NAME_INDEX: usize = 1;
const PATTERN_INDEX: usize = 2;
const ALIAS_INDEX: usize = 3;
const DEFINITION_INDEX: usize = 4;

/// The `Matches` represent matched results from a `Pattern` against text.
#[derive(Debug)]
pub struct Matches<'a> {
    captures: Captures<'a>,
    names: &'a BTreeMap<String, usize>,
}

impl<'a> Matches<'a> {
    /// Instantiates the matches for a pattern after the match.
    pub fn new(captures: Captures<'a>, names: &'a BTreeMap<String, usize>) -> Self {
        Matches { captures, names }
    }

    /// Returns a tuple of key/value with all the matches found.
    ///
    /// Note that if no match is found, the value is empty.
    pub fn iter(&'a self) -> MatchesIter<'a> {
        MatchesIter {
            captures: &self.captures,
            names: self.names.iter(),
        }
    }
}

pub struct MatchesIter<'a> {
    captures: &'a Captures<'a>,
    names: btree_map::Iter<'a, String, usize>,
}

impl<'a> Iterator for MatchesIter<'a> {
    type Item = (&'a str, &'a str);

    // Returns the name of the match group, the value matched.
    fn next(&mut self) -> Option<Self::Item> {
        // Okay, here's the trick. We allow the user to pass in an 'alias'. An
        // alias is a different name for the capture group name and one capture
        // group can have multiple aliases. The only way to recognize these
        // aliased capture groups is to know the offset of the capture name in
        // the regex, loop over the captures and return whatever is present for
        // that index, if anything.
        self.names.next().map(|(k, v)| {
            let key = k.as_str();
            let value = self.captures.at(*v as usize).unwrap_or("");
            (key, value)
        })
    }
}

/// The `Pattern` represents a compiled regex, ready to be matched against arbitrary text.
#[derive(Clone, Debug)]
pub struct Pattern {
    // NOTE this Arc exists solely to satisfy Clone and provide a Sync + Send
    // constraint for calling code in VRL. Theoretically we could remove this
    // entirely and have it be the responsibilty of the caller to provide for
    // Clone + Sync + Send.
    regex: Arc<Regex>,
    names: BTreeMap<String, usize>,
}

impl Pattern {
    /// Creates a new pattern from a raw regex string and an alias map to identify the
    /// fields properly.
    fn new(regex: &str, alias: &BTreeMap<String, String>) -> Result<Self, Error> {
        match Regex::new(regex) {
            Ok(r) => Ok({
                let mut names: BTreeMap<String, usize> = BTreeMap::new();
                r.foreach_name(|cap_name, cap_idx| {
                    let name = match alias.iter().find(|&(_k, v)| *v == cap_name) {
                        Some(item) => item.0.clone(),
                        None => String::from(cap_name),
                    };
                    names.insert(name, cap_idx[0] as usize);
                    true
                });
                Pattern {
                    regex: Arc::new(r),
                    names,
                }
            }),
            Err(_) => Err(Error::RegexCompilationFailed(regex.into())),
        }
    }

    /// Matches this compiled `Pattern` against the text and returns the matches.
    #[inline]
    pub fn match_against<'a>(&'a self, text: &'a str) -> Option<Matches<'a>> {
        self.regex
            .captures(text)
            .map(|cap| Matches::new(cap, &self.names))
    }
}

/// The basic structure to manage patterns, entry point for common usage.
#[derive(Debug)]
pub struct Grok {
    definitions: BTreeMap<String, String>,
}

impl Grok {
    /// Creates a new `Grok` instance and loads all the default patterns.
    pub fn with_patterns() -> Self {
        let mut grok = Grok {
            definitions: BTreeMap::new(),
        };
        for &(key, value) in PATTERNS {
            grok.insert_definition(String::from(key), String::from(value));
        }
        grok
    }

    /// Inserts a custom pattern.
    pub fn insert_definition<S: Into<String>>(&mut self, name: S, pattern: S) {
        self.definitions.insert(name.into(), pattern.into());
    }

    /// Compiles the given pattern, making it ready for matching.
    pub fn compile(&mut self, pattern: &str, with_alias_only: bool) -> Result<Pattern, Error> {
        let mut named_regex = String::from(pattern);
        let mut alias: BTreeMap<String, String> = BTreeMap::new();

        let mut index = 0;
        let mut iteration_left = MAX_RECURSION;
        let mut continue_iteration = true;

        let grok_regex = match Regex::new(GROK_PATTERN) {
            Ok(r) => r,
            Err(_) => return Err(Error::RegexCompilationFailed(GROK_PATTERN.into())),
        };

        while continue_iteration {
            continue_iteration = false;
            if iteration_left == 0 {
                return Err(Error::RecursionTooDeep(MAX_RECURSION));
            }
            iteration_left -= 1;

            if let Some(m) = grok_regex.captures(&named_regex.clone()) {
                continue_iteration = true;
                let raw_pattern = match m.at(PATTERN_INDEX) {
                    Some(p) => p,
                    None => {
                        return Err(Error::GenericCompilationFailure(
                            "Could not find pattern in matches".into(),
                        ))
                    }
                };

                let mut name = match m.at(NAME_INDEX) {
                    Some(n) => String::from(n),
                    None => {
                        return Err(Error::GenericCompilationFailure(
                            "Could not find name in matches".into(),
                        ))
                    }
                };

                if let Some(definition) = m.at(DEFINITION_INDEX) {
                    self.insert_definition(raw_pattern, definition);
                    name = format!("{}={}", name, definition);
                }

                // Since a pattern with a given name can show up more than once, we need to
                // loop through the number of matches found and apply the transformations
                // on each of them.
                for _ in 0..named_regex.matches(&format!("%{{{}}}", name)).count() {
                    // Check if we have a definition for the raw pattern key and fail quickly
                    // if not.
                    let pattern_definition = match self.definitions.get(raw_pattern) {
                        Some(d) => d,
                        None => return Err(Error::DefinitionNotFound(String::from(raw_pattern))),
                    };

                    // If no alias is specified and all but with alias are ignored,
                    // the replacement tells the regex engine to ignore the matches.
                    // Otherwise, the definition is turned into a regex that the
                    // engine understands and uses a named group.

                    let replacement = if with_alias_only && m.at(ALIAS_INDEX).is_none() {
                        format!("(?:{})", pattern_definition)
                    } else {
                        // If an alias is specified by the user use that one to
                        // match the name<index> conversion, otherwise just use
                        // the name of the pattern definition directly.
                        alias.insert(
                            match m.at(ALIAS_INDEX) {
                                Some(a) => String::from(a),
                                None => name.clone(),
                            },
                            format!("name{}", index),
                        );

                        format!("(?<name{}>{})", index, pattern_definition)
                    };

                    // Finally, look for the original %{...} style pattern and
                    // replace it with our replacement (only the first occurrence
                    // since we are iterating one by one).
                    named_regex = named_regex.replacen(&format!("%{{{}}}", name), &replacement, 1);

                    index += 1;
                }
            }
        }

        if named_regex.is_empty() {
            Err(Error::CompiledPatternIsEmpty(pattern.into()))
        } else {
            Pattern::new(&named_regex, &alias)
        }
    }
}

/// An error that occurred when using this library.
#[derive(Clone, Error, Debug, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// The recursion while compiling has exhausted the limit.
    #[error("Recursion while compiling reached the limit of {0}")]
    RecursionTooDeep(usize),
    /// After compiling, the resulting compiled regex pattern is empty.
    #[error("The given pattern \"{0}\" ended up compiling into an empty regex")]
    CompiledPatternIsEmpty(String),
    /// A corresponding pattern definition could not be found for the given name.
    #[error("The given pattern definition name \"{0}\" could not be found in the definition map")]
    DefinitionNotFound(String),
    /// If the compilation for a specific regex in the underlying engine failed.
    #[error("The given regex \"{0}\" failed compilation in the underlying engine")]
    RegexCompilationFailed(String),
    /// Something is messed up during the compilation phase.
    #[error("Something unexpected happened during the compilation phase: \"{0}\"")]
    GenericCompilationFailure(String),
}
