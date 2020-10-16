use crate::mapping::Result;

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

    pub fn regex(&self) -> &regex::Regex {
        &self.compiled
    }

    #[allow(dead_code)]
    pub fn is_global(&self) -> bool {
        self.global
    }

    fn compile(pattern: &str, multiline: bool, insensitive: bool) -> Result<regex::Regex> {
        regex::RegexBuilder::new(pattern)
            .case_insensitive(insensitive)
            .multi_line(multiline)
            .build()
            .map_err(|err| format!("invalid regex {}", err))
    }
}

/// Our dynamic regex equality shouldn't rely on the compiled value
/// as this is largely an implementation detail.
/// Plus regex::Regex doesn't implement PartialEq.
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
    fn create_regex() {
        let regex = Regex::new("abba".to_string(), false, false, false).unwrap();

        // Test our regex is working case sensitively.
        assert!(regex.regex().is_match("abba"));
    }
}
