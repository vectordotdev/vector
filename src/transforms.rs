use regex::RegexSet;
// TODO: The DefaultHasher algorithm is liable to change across rust versions, so if we want
// long term consistency, this should be set to something more stable. It also currently
// uses an algorithm that's collision resistent (which doesn't seem needed for this use case)
// but is slightly slower than some alternatives (which might matter for this use case).
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
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

#[cfg(test)]
mod test {
    use super::Sampler;
    use regex::RegexSet;
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
