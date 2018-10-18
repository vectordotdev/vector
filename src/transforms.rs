use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::thread;

use rand::{self, Isaac64Rng, Rng, SeedableRng};
use regex::bytes::RegexSet;
use transport::{Consumer, Log};

struct SamplerInner {
    rate: u8,
    rng: Isaac64Rng,
    pass_list: RegexSet,
}

impl SamplerInner {
    fn new(rate: u8, pass_list: RegexSet) -> Self {
        Self {
            rate,
            rng: Isaac64Rng::from_seed(rand::random()),
            pass_list,
        }
    }

    fn filter(&mut self, record: &[u8]) -> bool {
        if self.pass_list.is_match(record) {
            true
        } else {
            self.rng.gen_range(0, 100) < self.rate
        }
    }
}

pub struct Sampler {
    inner: SamplerInner,
    consumer: Consumer,
    log: Log,
    last_offset: Arc<AtomicUsize>,
}

impl Sampler {
    pub fn new(
        rate: u8,
        pass_list: RegexSet,
        consumer: Consumer,
        log: Log,
        last_offset: Arc<AtomicUsize>,
    ) -> Self {
        assert!(rate <= 100);
        Sampler {
            inner: SamplerInner::new(rate, pass_list),
            consumer,
            log,
            last_offset,
        }
    }

    pub fn run(mut self) -> thread::JoinHandle<u64> {
        let mut offset_in = 0;
        let mut offset_out = 0;
        thread::spawn(move || {
            while let Ok(batch) = self.consumer.poll() {
                for record in batch {
                    if self.inner.filter(&record) {
                        self.log
                            .append(&[&record])
                            .expect("failed to append to log");
                        offset_out += 1;
                    }
                    offset_in += 1;
                }
                let lo = self.last_offset.load(Ordering::Relaxed);
                if lo > 0 && offset_in == lo {
                    break;
                }
            }
            offset_out
        })
    }
}

#[cfg(test)]
mod test {
    use super::SamplerInner as Sampler;
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
