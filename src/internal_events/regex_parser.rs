use super::InternalEvent;
use metrics::counter;
use std::borrow::Cow;
use string_cache::DefaultAtom as Atom;

#[derive(Debug)]
pub(crate) struct RegexParserEventProcessed;

impl InternalEvent for RegexParserEventProcessed {
    fn emit_logs(&self) {
        trace!(message = "Processed one event.");
    }

    fn emit_metrics(&self) {
        counter!("events_processed", 1,
            "component_kind" => "transform",
            "component_type" => "regex_parser",
        );
    }
}

#[derive(Debug)]
pub(crate) struct RegexParserFailedMatch<'a> {
    pub value: &'a [u8],
}

impl InternalEvent for RegexParserFailedMatch<'_> {
    fn emit_logs(&self) {
        warn!(
            message = "regex pattern failed to match.",
            field = &truncate_string_at(&String::from_utf8_lossy(&self.value), 60)[..],
            rate_limit_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_error", 1,
            "component_kind" => "transform",
            "component_type" => "regex_parser",
            "error_type" => "failed_match",
        );
    }
}

#[derive(Debug)]
pub(crate) struct RegexParserMissingField<'a> {
    pub field: &'a Atom,
}

impl InternalEvent for RegexParserMissingField<'_> {
    fn emit_logs(&self) {
        debug!(message = "Field does not exist.", field = %self.field);
    }

    fn emit_metrics(&self) {
        counter!("processing_error", 1,
            "component_kind" => "transform",
            "component_type" => "regex_parser",
            "error_type" => "missing_field",
        );
    }
}

#[derive(Debug)]
pub(crate) struct RegexParserTargetExists<'a> {
    pub target_field: &'a Atom,
}

impl<'a> InternalEvent for RegexParserTargetExists<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Target field already exists.",
            target_field = %self.target_field,
            rate_limit_secs = 30
        )
    }

    fn emit_metrics(&self) {
        counter!("processing_error", 1,
            "component_kind" => "transform",
            "component_type" => "regex_parser",
            "error_type" => "target_field_exists",
        );
    }
}

#[derive(Debug)]
pub(crate) struct RegexParserConversionFailed<'a> {
    pub name: &'a Atom,
    pub error: crate::types::Error,
}

impl<'a> InternalEvent for RegexParserConversionFailed<'a> {
    fn emit_logs(&self) {
        debug!(
            message = "Could not convert types.",
            name = %self.name,
            error = %self.error,
            rate_limit_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_error", 1,
            "component_kind" => "transform",
            "component_type" => "regex_parser",
            "error_type" => "type_conversion_failed",
        );
    }
}

const ELLIPSIS: &str = "[...]";

fn truncate_string_at(s: &str, maxlen: usize) -> Cow<str> {
    if s.len() >= maxlen {
        let mut len = maxlen - ELLIPSIS.len();
        while !s.is_char_boundary(len) {
            len -= 1;
        }
        format!("{}{}", &s[..len], ELLIPSIS).into()
    } else {
        s.into()
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn truncate_utf8() {
        let message = "hello 😁 this is test";
        assert_eq!("hello [...]", super::truncate_string_at(&message, 13));
    }
}
