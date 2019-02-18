#![allow(clippy::new_without_default, clippy::needless_pass_by_value)]

pub mod buffers;
pub mod record;
pub mod sinks;
pub mod sources;
pub mod test_util;
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
