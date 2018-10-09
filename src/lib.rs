#[macro_use]
extern crate log;

extern crate byteorder;
extern crate memchr;
extern crate uuid;
extern crate rand;

#[cfg(test)]
extern crate tempdir;

pub mod transport;

use std::io::{BufRead, Write};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::thread;

use memchr::memchr;
use rand::{SeedableRng, Rng};

use transport::{Consumer, Log};

pub struct ConsoleSource {
    log: Log,
}

impl ConsoleSource {
    pub fn new(log: Log) -> Self {
        ConsoleSource { log }
    }

    pub fn run(mut self) -> thread::JoinHandle<u64> {
        thread::spawn(move || {
            let mut offset = 0;
            let reader = ::std::io::stdin();
            let mut buffer = reader.lock();
            loop {
                let consumed = match buffer.fill_buf() {
                    Ok(bytes) => {
                        if bytes.len() == 0 {
                            break;
                        }
                        if let Some(newline) = memchr(b'\n', bytes) {
                            let pos = newline + 1;
                            self.log
                                .append(&[&bytes[0..pos]])
                                .expect("failed to append input");
                            offset += 1;
                            pos
                        } else {
                            // if we couldn't find a newline, just throw away the buffer
                            bytes.len()
                        }
                    }
                    _ => break,
                };
                buffer.consume(consumed);
            }
            offset
        })
    }
}

pub struct ConsoleSink {
    consumer: Consumer,
    last_offset: Arc<AtomicUsize>,
}

impl ConsoleSink {
    pub fn new(consumer: Consumer, last_offset: Arc<AtomicUsize>) -> Self {
        ConsoleSink { consumer, last_offset }
    }

    pub fn run(mut self) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            let mut offset = 0;
            let mut writer = ::std::io::stdout();
            while let Ok(batch) = self.consumer.poll() {
                if batch.is_empty() {
                    let lo = self.last_offset.load(Ordering::Relaxed);
                    if lo > 0 && offset == lo {
                        break;
                    }
                } else {
                    for record in batch {
                        writer.write(&record).unwrap();
                        offset += 1;
                    }
                }
            }
        })
    }
}

pub struct Sampler {
    rate: u8,
    consumer: Consumer,
    log: Log,
    last_offset: Arc<AtomicUsize>,
}

impl Sampler {
    pub fn new(rate: u8, consumer: Consumer, log: Log, last_offset: Arc<AtomicUsize>) -> Self {
        assert!(rate <= 100);
        Sampler { rate, consumer, log, last_offset }
    }

    pub fn run(mut self) -> thread::JoinHandle<u64> {
        let mut offset_in = 0;
        let mut offset_out = 0;
        let mut rng = rand::Isaac64Rng::from_seed(rand::random());
        thread::spawn(move || {
            while let Ok(batch) = self.consumer.poll() {
                for record in batch {
                    if rng.gen_range(0, 100) < self.rate {
                        self.log.append(&[&record]).expect("failed to append to log");
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
