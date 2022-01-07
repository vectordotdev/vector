use onig::Regex;
use regex_syntax::escape;

/// Returns compiled word boundary regex.
pub fn word_regex(to_match: &str) -> Regex {
    Regex::new(&format!(r#"\b{}\b"#, escape(to_match).replace("\\*", ".*")))
        .expect("invalid wildcard regex")
}

/// Returns compiled wildcard regex.
pub fn wildcard_regex(to_match: &str) -> Regex {
    Regex::new(&format!("^{}$", escape(to_match).replace("\\*", ".*")))
        .expect("invalid wildcard regex")
}
