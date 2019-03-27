use super::Transform;
use crate::record::Record;
use regex::RegexSet;
use serde::{Deserialize, Serialize};
// TODO: The DefaultHasher algorithm is liable to change across rust versions, so if we want
// long term consistency, this should be set to something more stable. It also currently
// uses an algorithm that's collision resistent (which doesn't seem needed for this use case)
// but is slightly slower than some alternatives (which might matter for this use case).
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct SamplerConfig {
    pub rate: u64,
    pub pass_list: Vec<String>,
}

#[typetag::serde(name = "sampler")]
impl crate::topology::config::TransformConfig for SamplerConfig {
    fn build(&self) -> Result<Box<dyn Transform>, String> {
        RegexSet::new(&self.pass_list)
            .map_err(|err| err.to_string())
            .map::<Box<dyn Transform>, _>(|regex_set| Box::new(Sampler::new(self.rate, regex_set)))
    }
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
        if let Ok(raw_line) = std::str::from_utf8(&record.raw[..]) {
            if self.pass_list.is_match(&raw_line) {
                return Some(record);
            }
        }

        let mut hasher = DefaultHasher::new();
        hasher.write(&record.raw[..]);
        let hash = hasher.finish();

        if hash % self.rate == 0 {
            record
                .structured
                .insert(Atom::from("sample_rate"), self.rate.to_string());

            Some(record)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Sampler;
    use crate::record::Record;
    use crate::transforms::Transform;
    use approx::assert_relative_eq;
    use regex::RegexSet;
    use string_cache::DefaultAtom as Atom;

    #[test]
    fn samples_at_roughly_the_configured_rate() {
        let num_records = 10000;

        let records = random_records(num_records);
        let sampler = Sampler::new(2, RegexSet::new(&["na"]).unwrap());
        let total_passed = records
            .into_iter()
            .filter_map(|record| sampler.transform(record))
            .count();
        let ideal = 1.0 as f64 / 2.0 as f64;
        let actual = total_passed as f64 / num_records as f64;
        assert_relative_eq!(ideal, actual, epsilon = ideal * 0.5);

        let records = random_records(num_records);
        let sampler = Sampler::new(25, RegexSet::new(&["na"]).unwrap());
        let total_passed = records
            .into_iter()
            .filter_map(|record| sampler.transform(record))
            .count();
        let ideal = 1.0 as f64 / 25.0 as f64;
        let actual = total_passed as f64 / num_records as f64;
        assert_relative_eq!(ideal, actual, epsilon = ideal * 0.5);
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
        let record = Record::from("i am important");
        let sampler = Sampler::new(0, RegexSet::new(&["important"]).unwrap());
        let iterations = 0..1000;
        let total_passed = iterations
            .filter_map(|_| sampler.transform(record.clone()))
            .count();
        assert_eq!(total_passed, 1000);
    }

    #[test]
    fn sampler_adds_sampling_rate_to_record() {
        let records = random_records(10000);
        let sampler = Sampler::new(10, RegexSet::new(&["na"]).unwrap());
        let passing = records
            .into_iter()
            .filter(|s| !s.line.contains("na"))
            .find_map(|record| sampler.transform(record))
            .unwrap();
        assert_eq!(passing.custom[&Atom::from("sample_rate")], "10");

        let records = random_records(10000);
        let sampler = Sampler::new(25, RegexSet::new(&["na"]).unwrap());
        let passing = records
            .into_iter()
            .filter(|s| !s.line.contains("na"))
            .find_map(|record| sampler.transform(record))
            .unwrap();
        assert_eq!(passing.custom[&Atom::from("sample_rate")], "25");

        // If the record passed the regex check, don't include the sampling rate
        let sampler = Sampler::new(25, RegexSet::new(&["na"]).unwrap());
        let record = Record::from("nananana");
        let passing = sampler.transform(record).unwrap();
        assert!(!passing.custom.contains_key(&Atom::from("sample_rate")));
    }

    fn random_records(n: usize) -> Vec<Record> {
        use rand::distributions::Alphanumeric;
        use rand::{thread_rng, Rng};

        (0..n)
            .map(|_| {
                thread_rng()
                    .sample_iter(&Alphanumeric)
                    .take(10)
                    .collect::<String>()
            })
            .map(Record::from)
            .collect()
    }
}
