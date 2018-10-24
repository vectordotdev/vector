use std::thread;

use super::ReaderSource;
use transport::Log;

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
            let buffer = reader.lock();
            let mut source = ReaderSource::new(buffer);
            while let Ok(msg) = source.pull() {
                self.log.append(&[&msg]).expect("failed to append input");
                offset += 1;
            }
            offset
        })
    }
}
