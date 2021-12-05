use crate::mapping::Result;

/// Regex is created by the remap parser when a regex is parsed from the script.
/// Regexes are parsed using the syntax /<pattern>/<flags>
/// Currently the flags supported are
/// i - case insensitive
/// m - multiline
/// g - global
///
/// Note the global flag is not actually a part of the compiled regex and is not used in all places,
/// the flag is often used to determine which function to call when using the regex.. ie `replace` vs `replace_all`.
/// Technically the other data - pattern, multiline, insensitive do not need to be stored as they are a part of the
/// compiled regex, but it may be useful for debugging and testing.
#[derive(Debug, Clone)]
pub(in crate::mapping) struct Regex {
    pattern: String,
    multiline: bool,
    insensitive: bool,
    global: bool,
    compiled: regex::Regex,
}

impl Regex {
    pub(in crate::mapping) fn new(
        pattern: String,
        multiline: bool,
        insensitive: bool,
        global: bool,
    ) -> Result<Self> {
        let compiled = Self::compile(&pattern, multiline, insensitive)?;
        Ok(Self {
            pattern,
            multiline,
            insensitive,
            global,
            compiled,
        })
    }

    /// Retrieve the compiled regex.
    pub fn regex(&self) -> &regex::Regex {
        &self.compiled
    }

    fn compile(pattern: &str, multiline: bool, insensitive: bool) -> Result<regex::Regex> {
        regex::RegexBuilder::new(pattern)
            .case_insensitive(insensitive)
            .multi_line(multiline)
            .build()
            .map_err(|error| format!("invalid regex: {}", error))
    }
}

/// `regex::Regex` doesn't implement `PartialEq`.
impl PartialEq for Regex {
    fn eq(&self, other: &Self) -> bool {
        self.pattern == other.pattern
            && self.multiline == other.multiline
            && self.insensitive == other.insensitive
            && self.global == other.global
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_regex_case_sensitive() {
        let regex = Regex::new("abba".to_string(), false, false, false).unwrap();

        // Test our regex is working case sensitively.
        assert!(regex.regex().is_match("abba"));
        assert!(!regex.regex().is_match("aBbA"));
    }

    #[test]
    fn create_regex_case_insensitive() {
        let regex = Regex::new("abba".to_string(), false, true, false).unwrap();

        // Test our regex is working case insensitively.
        assert!(regex.regex().is_match("abba"));
        assert!(regex.regex().is_match("aBbA"));
    }
}
