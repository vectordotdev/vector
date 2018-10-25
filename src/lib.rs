#[macro_use]
extern crate log;
extern crate chrono;
extern crate fern;

extern crate byteorder;
extern crate bytes;
extern crate futures;
extern crate memchr;
extern crate rand;
extern crate regex;
extern crate tokio;
extern crate uuid;

#[cfg(test)]
extern crate tempdir;

pub mod console;
pub mod sources;
pub mod splunk;
pub mod transforms;
pub mod transport;

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
        }).level(log::LevelFilter::Info)
        .chain(std::io::stderr())
        .apply()
        .unwrap();
}
