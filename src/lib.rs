#[macro_use]
extern crate log;

extern crate byteorder;
extern crate memchr;
extern crate rand;
extern crate uuid;

#[cfg(test)]
extern crate tempdir;

pub mod console;
pub mod splunk;
pub mod transport;

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::thread;

use rand::{Rng, SeedableRng};

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

#[cfg(test)]
mod test {
    use super::transport::{Consumer, Coordinator, Log};
    use tempdir::TempDir;

    static MESSAGES: &[&[u8]] = &[
        b"i am the first message",
        b"i am the second message",
        b"i am the third message",
        b"i am the fourth message",
    ];

    fn setup(topic: &str) -> (TempDir, Coordinator, Log, Consumer) {
        let data_dir = TempDir::new_in(".", "logs").expect("creating tempdir");

        let mut coordinator = Coordinator::new(&data_dir);
        let log = coordinator.create_log(topic).expect("failed to build log");
        let consumer = coordinator
            .build_consumer(topic)
            .expect("failed to build consumer");
        (data_dir, coordinator, log, consumer)
    }

    #[test]
    fn basic_write_then_read() {
        let (_data_dir, _coordinator, mut log, mut consumer) = setup("foo");

        log.append(MESSAGES).expect("failed to append batch");

        let batch_out = consumer.poll().expect("failed to poll for batch");
        assert_eq!(batch_out, MESSAGES);
    }

    #[test]
    fn consumer_starts_from_the_end() {
        let (_data_dir, coordinator, mut log, _) = setup("foo");

        log.append(&MESSAGES[0..2]).expect("failed to append batch");

        let mut consumer = coordinator
            .build_consumer("foo")
            .expect("failed to build consumer");

        log.append(&MESSAGES[2..4]).expect("failed to append batch");

        let batch_out = consumer.poll().expect("failed to poll for batch");
        assert_eq!(batch_out, &MESSAGES[2..4]);
    }

    #[test]
    fn logs_split_into_segments() {
        let (_data_dir, _coordinator, mut log, mut consumer) = setup("foo");

        log.append(&MESSAGES[..1])
            .expect("failed to append first record");

        // make this auto with config?
        log.roll_segment().expect("failed to roll new segment");

        log.append(&MESSAGES[1..]).expect("failed to append batch");

        assert_eq!(2, log.get_segments().unwrap().count());
        assert_eq!(consumer.poll().expect("failed to poll"), MESSAGES);
    }

    #[test]
    fn only_retains_segments_with_active_consumers() {
        let (_data_dir, mut coordinator, mut log, mut consumer) = setup("foo");

        log.append(&MESSAGES[..1])
            .expect("failed to append first record");

        // make this auto with config
        log.roll_segment().expect("failed to roll new segment");

        log.append(&MESSAGES[1..]).expect("failed to append batch");

        assert_eq!(2, log.get_segments().unwrap().count());
        assert_eq!(consumer.poll().expect("failed to poll"), MESSAGES);
        consumer.commit_offsets(&mut coordinator);

        // make this auto
        coordinator
            .enforce_retention()
            .expect("failed to enforce retention");
        assert_eq!(1, log.get_segments().unwrap().count());
    }
}
