use regex::{Regex, RegexSet};
// TODO: The DefaultHasher algorithm is liable to change across rust versions, so if we want
// long term consistency, this should be set to something more stable. It also currently
// uses an algorithm that's collision resistent (which doesn't seem needed for this use case)
// but is slightly slower than some alternatives (which might matter for this use case).
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use string_cache::DefaultAtom as Atom;
use Record;

pub struct Sampler {
    rate: u8,
    pass_list: RegexSet,
}

impl Sampler {
    pub fn new(rate: u8, pass_list: RegexSet) -> Self {
        Self { rate, pass_list }
    }

    // TODO: annotate record with current sampling rate
    pub fn filter(&self, record: &Record) -> bool {
        if self.pass_list.is_match(&record.line) {
            true
        } else {
            let mut hasher = DefaultHasher::new();
            hasher.write(record.line.as_bytes());
            let hash = hasher.finish();

            hash % 100 < self.rate.into()
        }
    }
}

pub struct RegexParser {
    regex: Regex,
    tag_name: Atom,
}

impl RegexParser {
    pub fn new(regex: Regex, tag_name: &str) -> Self {
        Self {
            regex,
            tag_name: tag_name.into(),
        }
    }

    pub fn apply(&self, mut record: Record) -> Record {
        if let Some(match_) = self.regex.captures(&record.line).and_then(|c| c.get(1)) {
            record
                .custom
                .insert(self.tag_name.clone(), match_.as_str().to_owned());
        }

        record
    }
}

pub struct TagFilter {
    tag_name: Atom,
    value: String,
}

impl TagFilter {
    pub fn new(tag_name: String, value: String) -> Self {
        Self {
            tag_name: tag_name.into(),
            value,
        }
    }

    pub fn filter(&self, record: &Record) -> bool {
        record.custom.get(&self.tag_name) == Some(&self.value)
    }
}

#[cfg(test)]
mod test {
    use super::{RegexParser, Sampler};
    use regex::{Regex, RegexSet};
    use std::sync::Arc;
    use Record;

    #[test]
    fn samples_at_roughly_the_configured_rate() {
        let records = random_records(1000);
        let sampler = Sampler::new(50, RegexSet::new(&["na"]).unwrap());
        let total_passed = records
            .iter()
            .filter(|record| sampler.filter(record))
            .count();
        assert!(total_passed > 400);
        assert!(total_passed < 600);
    }

    #[test]
    fn consistely_samples_the_same_records() {
        let records = random_records(1000);
        let sampler = Sampler::new(50, RegexSet::new(&["na"]).unwrap());

        let first_run = records
            .iter()
            .filter(|record| sampler.filter(record))
            .collect::<Vec<_>>();
        let second_run = records
            .iter()
            .filter(|record| sampler.filter(record))
            .collect::<Vec<_>>();

        assert_eq!(first_run, second_run);
    }

    #[test]
    fn always_passes_records_matching_pass_list() {
        let record = Record::new_from_line("i am important".to_string());
        let sampler = Sampler::new(0, RegexSet::new(&["important"]).unwrap());
        let iterations = 0..1000;
        let total_passed = iterations.filter(|_| sampler.filter(&record)).count();
        assert_eq!(total_passed, 1000);
    }

    #[test]
    fn regex_parser_adds_parsed_field_to_record() {
        let record = Record::new_from_line("status=1234".to_string());
        let parser = RegexParser::new(Regex::new(r"status=(\d+)").unwrap(), "status");

        let record = parser.apply(record);

        assert_eq!(record.custom[&"status".into()], "1234");
    }

    #[test]
    fn regex_parser_doesnt_do_anything_if_no_match() {
        let record = Record::new_from_line("asdf1234".to_string());
        let parser = RegexParser::new(Regex::new(r"status=(\d+)").unwrap(), "status");

        let record = parser.apply(record);

        assert_eq!(record.custom.get(&"status".into()), None);
    }

    fn random_records(n: usize) -> Vec<Record> {
        use rand::distributions::Alphanumeric;
        use rand::{thread_rng, Rng};

        (0..n)
            .map(|_| {
                let line = thread_rng().sample_iter(&Alphanumeric).take(10).collect();
                Record::new_from_line(line)
            }).collect()
    }
}
