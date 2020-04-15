use super::InternalEvent;
use metrics::counter;
use std::borrow::Cow;
use string_cache::DefaultAtom as Atom;

#[derive(Debug)]
pub struct RegexEventProcessed;

impl InternalEvent for RegexEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed", 1,
            "component_kind" => "transform",
            "component_type" => "regex_parser",
        );
    }
}

#[derive(Debug)]
pub struct RegexFailedMatch<'a> {
    pub value: &'a [u8],
}

impl InternalEvent for RegexFailedMatch<'_> {
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
pub struct RegexMissingField<'a> {
    pub field: &'a Atom,
}

impl InternalEvent for RegexMissingField<'_> {
    fn emit_logs(&self) {
        debug!(message = "field does not exist.", field = %self.field);
    }

    fn emit_metrics(&self) {
        counter!("processing_error", 1,
            "component_kind" => "transform",
            "component_type" => "regex_parser",
            "error_type" => "missing_field",
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
