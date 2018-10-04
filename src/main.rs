extern crate router;

#[macro_use]
extern crate log;
extern crate fern;

use router::transport::{Consumer, Coordinator};
use std::io::BufRead;
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

    let finished = Arc::new(AtomicBool::new(true));
    let finished2 = finished.clone();
    let handle = thread::spawn(move || {
        ::std::thread::sleep(::std::time::Duration::from_millis(10));
        while let Ok(batch) = consumer.poll() {
            if batch.is_empty() && !finished2.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            } else {
                for record in batch {
                    println!(
                        "{}",
                        std::str::from_utf8(&record).expect("invalid utf8 message")
                    );
                }
            }
        }
    });

    let stdin = ::std::io::stdin();
    for line in stdin.lock().lines().filter_map(Result::ok) {
        log.append(&[line.as_bytes()])
            .expect("failed to append input");
    }
    finished.store(false, std::sync::atomic::Ordering::Relaxed);

    handle.join().unwrap();
}
