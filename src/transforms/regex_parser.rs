use super::Transform;
use crate::record::Record;
use regex::bytes::Regex;
use serde::{Deserialize, Serialize};
use std::str;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RegexParserConfig {
    pub regex: String,
}

#[typetag::serde(name = "regex_parser")]
impl crate::topology::config::TransformConfig for RegexParserConfig {
    fn build(&self) -> Result<Box<dyn Transform>, String> {
        Regex::new(&self.regex)
            .map_err(|err| err.to_string())
            .map::<Box<dyn Transform>, _>(|r| Box::new(RegexParser::new(r)))
    }
}

pub struct RegexParser {
    regex: Regex,
}

impl RegexParser {
    pub fn new(regex: Regex) -> Self {
        Self { regex }
    }
}

impl Transform for RegexParser {
    fn transform(&self, mut record: Record) -> Option<Record> {
        if let Some(captures) = self.regex.captures(&record.raw[..]) {
            for name in self.regex.capture_names().filter_map(|c| c) {
                if let Some(capture) = captures.name(name) {
                    let capture = String::from_utf8_lossy(capture.as_bytes()).into_owned();

                    record.structured.insert(name.into(), capture);
                }
            }
        }

        Some(record)
    }
}

#[cfg(test)]
mod tests {
    use super::RegexParser;
    use crate::record::Record;
    use crate::transforms::Transform;
    use regex::bytes::Regex;

    #[test]
    fn regex_parser_adds_parsed_field_to_record() {
        let record = Record::from("status=1234 time=5678");
        let parser =
            RegexParser::new(Regex::new(r"status=(?P<status>\d+) time=(?P<time>\d+)").unwrap());

        let record = parser.transform(record).unwrap();

        assert_eq!(record.structured[&"status".into()], "1234");
        assert_eq!(record.structured[&"time".into()], "5678");
    }

    #[test]
    fn regex_parser_doesnt_do_anything_if_no_match() {
        let record = Record::from("asdf1234");
        let parser = RegexParser::new(Regex::new(r"status=(?P<status>\d+)").unwrap());

        let record = parser.transform(record).unwrap();

        assert_eq!(record.structured.get(&"status".into()), None);
    }
}
