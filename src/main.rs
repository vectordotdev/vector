extern crate router;

#[macro_use]
extern crate log;
extern crate fern;
extern crate memchr;

use memchr::memchr;
use router::transport::{Consumer, Coordinator};
use std::io::{BufRead, Write};
use std::sync::{atomic::AtomicBool, Arc};
use std::thread;

fn main() {
    fern::Dispatch::new()
        .level(log::LevelFilter::Debug)
        .chain(std::io::stderr())
        .apply()
        .unwrap();

    info!("Hello, world!");

    let dir = "logs";
    let mut coordinator = Coordinator::default();
    let mut log = coordinator.create_log(&dir).expect("failed to create log");
    let mut consumer = Consumer::new(&dir).expect("failed to build consumer");

    let mut writer = ::std::io::stdout();

    let finished = Arc::new(AtomicBool::new(true));
    let finished2 = finished.clone();
    let handle = thread::spawn(move || {
        ::std::thread::sleep(::std::time::Duration::from_millis(10));
        while let Ok(batch) = consumer.poll() {
            if batch.is_empty() && !finished2.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            } else {
                for record in batch {
                    writer.write(&record).unwrap();
                }
            }
        }
    });

    let reader = ::std::io::stdin();
    let mut buffer = reader.lock();
    loop {
        let consumed = match buffer.fill_buf() {
            Ok(bytes) => {
                if bytes.len() == 0 { break }
                if let Some(newline) = memchr(b'\n', bytes) {
                    let pos = newline + 1;
                    log.append(&[&bytes[0..pos]])
                        .expect("failed to append input");
                    pos
                } else {
                    // if we couldn't find a newline, just throw away the buffer
                    bytes.len()
                }
            },
            _ => break,
        };
        buffer.consume(consumed);
    }
    finished.store(false, std::sync::atomic::Ordering::Relaxed);

    handle.join().unwrap();
}
