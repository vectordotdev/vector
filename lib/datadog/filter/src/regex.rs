use regex::Regex;
use wildmatch::WildMatch;

/// Returns compiled word boundary regex.
pub fn word_regex(to_match: &str) -> Regex {
    Regex::new(&format!(
        r#"\b{}\b"#,
        regex::escape(to_match).replace("\\*", ".*")
    ))
    .expect("invalid wildcard regex")
}

/// Returns compiled wildcard regex.
pub fn wildcard(to_match: &str) -> WildMatch {
    WildMatch::new(to_match)
}
