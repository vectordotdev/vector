extern crate router;

#[macro_use]
extern crate log;
extern crate chrono;
extern crate fern;

use router::{transport::Coordinator, ConsoleSink, ConsoleSource};
use std::sync::{atomic::AtomicBool, Arc};

fn main() {
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                record.target(),
                record.level(),
                message
            ))
        }).level(log::LevelFilter::Debug)
        .chain(std::io::stderr())
        .apply()
        .unwrap();

    let topic = "foo";
    let mut coordinator = Coordinator::new("logs");
    let log = coordinator.create_log(topic).expect("failed to create log");
    let consumer = coordinator
        .build_consumer(topic)
        .expect("failed to build consumer");

    let source = ConsoleSource::new(log);
    info!("starting source");
    let source_handle = source.run();

    let finished = Arc::new(AtomicBool::new(true));
    let stop = finished.clone();
    let sink = ConsoleSink::new(consumer, stop);
    info!("starting sink");
    let sink_handle = sink.run();

    source_handle.join().unwrap();
    info!("source finished");
    finished.store(false, std::sync::atomic::Ordering::Relaxed);
    sink_handle.join().unwrap();
    info!("sink finished");
}
