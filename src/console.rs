use std::io::Write;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::thread;

use transport::Consumer;

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
