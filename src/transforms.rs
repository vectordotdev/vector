use regex::{Regex, RegexSet};
// TODO: The DefaultHasher algorithm is liable to change across rust versions, so if we want
// long term consistency, this should be set to something more stable. It also currently
// uses an algorithm that's collision resistent (which doesn't seem needed for this use case)
// but is slightly slower than some alternatives (which might matter for this use case).
use crate::record::Record;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use string_cache::DefaultAtom as Atom;

pub trait Transform: Sync + Send {
    fn transform(&self, record: Record) -> Option<Record>;
}

pub struct Sampler {
    rate: u64,
    pass_list: RegexSet,
}

impl Sampler {
    pub fn new(rate: u64, pass_list: RegexSet) -> Self {
        Self { rate, pass_list }
    }
}

impl Transform for Sampler {
    fn transform(&self, mut record: Record) -> Option<Record> {
        if self.pass_list.is_match(&record.line) {
            return Some(record);
        }

        let mut hasher = DefaultHasher::new();
        hasher.write(record.line.as_bytes());
        let hash = hasher.finish();

        if hash % self.rate == 0 {
            record
                .custom
                .insert(Atom::from("sample_rate"), self.rate.to_string());

            Some(record)
        } else {
            None
        }
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
        if let Some(captures) = self.regex.captures(&record.line) {
            for name in self.regex.capture_names().filter_map(|c| c) {
                if let Some(capture) = captures.name(name) {
                    record
                        .custom
                        .insert(name.into(), capture.as_str().to_owned());
                }
            }
        }

        Some(record)
    }
}

pub struct FieldFilter {
    field_name: Atom,
    value: String,
}

impl FieldFilter {
    pub fn new(field_name: String, value: String) -> Self {
        Self {
            field_name: field_name.into(),
            value,
        }
    }
}

impl Transform for FieldFilter {
    fn transform(&self, record: Record) -> Option<Record> {
        if record.custom.get(&self.field_name) == Some(&self.value) {
            Some(record)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test {
    use super::{RegexParser, Sampler, Transform};
    use crate::record::Record;
    use regex::{Regex, RegexSet};
    use string_cache::DefaultAtom as Atom;

    #[test]
    fn samples_at_roughly_the_configured_rate() {
        let records = random_records(1000);
        let sampler = Sampler::new(2, RegexSet::new(&["na"]).unwrap());
        let total_passed = records
            .into_iter()
            .filter_map(|record| sampler.transform(record))
            .count();
        assert!(total_passed > 400);
        assert!(total_passed < 600);

        let records = random_records(1000);
        let sampler = Sampler::new(25, RegexSet::new(&["na"]).unwrap());
        let total_passed = records
            .into_iter()
            .filter_map(|record| sampler.transform(record))
            .count();
        assert!(total_passed > 30);
        assert!(total_passed < 50);
    }

    #[test]
    fn consistely_samples_the_same_records() {
        let records = random_records(1000);
        let sampler = Sampler::new(2, RegexSet::new(&["na"]).unwrap());

        let first_run = records
            .clone()
            .into_iter()
            .filter_map(|record| sampler.transform(record))
            .collect::<Vec<_>>();
        let second_run = records
            .into_iter()
            .filter_map(|record| sampler.transform(record))
            .collect::<Vec<_>>();

        assert_eq!(first_run, second_run);
    }

    #[test]
    fn always_passes_records_matching_pass_list() {
        let record = Record::new_from_line("i am important".to_string());
        let sampler = Sampler::new(0, RegexSet::new(&["important"]).unwrap());
        let iterations = 0..1000;
        let total_passed = iterations
            .filter_map(|_| sampler.transform(record.clone()))
            .count();
        assert_eq!(total_passed, 1000);
    }

    #[test]
    fn sampler_adds_sampling_rate_to_record() {
        let records = random_records(100);
        let sampler = Sampler::new(10, RegexSet::new(&["na"]).unwrap());
        let passing = records
            .into_iter()
            .find_map(|record| sampler.transform(record))
            .unwrap();
        assert_eq!(passing.custom[&Atom::from("sample_rate")], "10");

        let records = random_records(100);
        let sampler = Sampler::new(25, RegexSet::new(&["na"]).unwrap());
        let passing = records
            .into_iter()
            .find_map(|record| sampler.transform(record))
            .unwrap();
        assert_eq!(passing.custom[&Atom::from("sample_rate")], "25");

        // If the record passed the regex check, don't include the sampling rate
        let sampler = Sampler::new(25, RegexSet::new(&["na"]).unwrap());
        let record = Record::new_from_line("nananana".to_string());
        let passing = sampler.transform(record).unwrap();
        assert!(!passing.custom.contains_key(&Atom::from("sample_rate")));
    }

    #[test]
    fn regex_parser_adds_parsed_field_to_record() {
        let record = Record::new_from_line("status=1234 time=5678".to_string());
        let parser =
            RegexParser::new(Regex::new(r"status=(?P<status>\d+) time=(?P<time>\d+)").unwrap());

        let record = parser.transform(record).unwrap();

        assert_eq!(record.custom[&"status".into()], "1234");
        assert_eq!(record.custom[&"time".into()], "5678");
    }

    #[test]
    fn regex_parser_doesnt_do_anything_if_no_match() {
        let record = Record::new_from_line("asdf1234".to_string());
        let parser = RegexParser::new(Regex::new(r"status=(?P<status>\d+)").unwrap());

        let record = parser.transform(record).unwrap();

        assert_eq!(record.custom.get(&"status".into()), None);
    }

    fn random_records(n: usize) -> Vec<Record> {
        use rand::distributions::Alphanumeric;
        use rand::{thread_rng, Rng};

        (0..n)
            .map(|_| {
                let line = thread_rng().sample_iter(&Alphanumeric).take(10).collect();
                Record::new_from_line(line)
            })
            .collect()
    }
}
