use rand::{self, Isaac64Rng, Rng, SeedableRng};
use regex::bytes::RegexSet;

pub struct Sampler {
    rate: u8,
    rng: Isaac64Rng,
    pass_list: RegexSet,
}

impl Sampler {
    pub fn new(rate: u8, pass_list: RegexSet) -> Self {
        Self {
            rate,
            rng: Isaac64Rng::from_seed(rand::random()),
            pass_list,
        }
    }

    // TODO: annotate record with current sampling rate
    pub fn filter(&mut self, record: &[u8]) -> bool {
        if self.pass_list.is_match(record) {
            true
        } else {
            self.rng.gen_range(0, 100) < self.rate
        }
    }
}

#[cfg(test)]
mod test {
    use super::Sampler;
    use regex::bytes::RegexSet;

    #[test]
    fn samples_at_roughly_the_configured_rate() {
        let record = &[0u8];
        let mut sampler = Sampler::new(50, RegexSet::new(&["na"]).unwrap());
        let iterations = 0..1000;
        let total_passed = iterations.filter(|_| sampler.filter(record)).count();
        assert!(total_passed > 400);
        assert!(total_passed < 600);
    }

    #[test]
    fn always_passes_records_matching_pass_list() {
        let record = "i am important";
        let mut sampler = Sampler::new(0, RegexSet::new(&["important"]).unwrap());
        let iterations = 0..1000;
        let total_passed = iterations
            .filter(|_| sampler.filter(record.as_bytes()))
            .count();
        assert_eq!(total_passed, 1000);
    }
}
