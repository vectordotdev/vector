use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::thread;

use rand::{self, Rng, SeedableRng};
use transport::{Consumer, Log};

pub struct Sampler {
    rate: u8,
    consumer: Consumer,
    log: Log,
    last_offset: Arc<AtomicUsize>,
}

impl Sampler {
    pub fn new(rate: u8, consumer: Consumer, log: Log, last_offset: Arc<AtomicUsize>) -> Self {
        assert!(rate <= 100);
        Sampler {
            rate,
            consumer,
            log,
            last_offset,
        }
    }

    pub fn run(mut self) -> thread::JoinHandle<u64> {
        let mut offset_in = 0;
        let mut offset_out = 0;
        let mut rng = rand::Isaac64Rng::from_seed(rand::random());
        thread::spawn(move || {
            while let Ok(batch) = self.consumer.poll() {
                for record in batch {
                    if rng.gen_range(0, 100) < self.rate {
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
