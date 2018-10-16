use std::io::{BufRead, Write};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::thread;

use memchr::memchr;
use transport::{Consumer, Log};

pub struct Source {
    log: Log,
}

impl Source {
    pub fn new(log: Log) -> Self {
        Source { log }
    }

    pub fn run(mut self) -> thread::JoinHandle<u64> {
        thread::spawn(move || {
            let mut offset = 0;
            let reader = ::std::io::stdin();
            let mut buffer = reader.lock();
            loop {
                let consumed = match buffer.fill_buf() {
                    Ok(bytes) => {
                        if bytes.is_empty() {
                            break;
                        }
                        // TODO: don't include the newlines
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

pub struct Sink {
    consumer: Consumer,
    last_offset: Arc<AtomicUsize>,
}

impl Sink {
    pub fn new(consumer: Consumer, last_offset: Arc<AtomicUsize>) -> Self {
        Sink {
            consumer,
            last_offset,
        }
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
                        writer.write_all(&record).unwrap();
                        writer.write_all(b"\n").unwrap();
                        offset += 1;
                    }
                }
            }
        })
    }
}
