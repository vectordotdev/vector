extern crate bytes;
extern crate chrono;
extern crate elastic_responses;
extern crate fern;
extern crate futures;
extern crate hyper;
extern crate log;
extern crate rand;
extern crate regex;
extern crate serde;
extern crate serde_derive;
extern crate serde_json;
extern crate stream_cancel;
extern crate string_cache;
extern crate tokio;
extern crate tokio_fs;
extern crate tokio_retry;
extern crate uuid;

pub mod record;
pub mod sinks;
pub mod sources;
pub mod topology;
pub mod transforms;

pub use crate::record::Record;

pub fn setup_logger() {
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                record.target(),
                record.level(),
                message
            ))
        })
        .level(log::LevelFilter::Info)
        .chain(std::io::stderr())
        .apply()
        .unwrap();
}
