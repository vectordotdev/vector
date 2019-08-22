use futures::Future;
use snafu::Snafu;
use std::path::PathBuf;

pub mod file;
pub mod journald;
#[cfg(feature = "rdkafka")]
pub mod kafka;
pub mod statsd;
pub mod stdin;
pub mod syslog;
pub mod tcp;
pub mod udp;
mod util;
pub mod vector;

pub type Source = Box<dyn Future<Item = (), Error = ()> + Send>;

#[derive(Debug, Snafu)]
pub enum BuildError {
    #[snafu(display("data_dir option required, but not given here or globally"))]
    NoDataDir,
    #[snafu(display(
        "could not create subdirectory {:?} inside of data_dir {:?}",
        subdir,
        data_dir
    ))]
    MakeSubdirectoryError {
        subdir: PathBuf,
        data_dir: PathBuf,
        source: std::io::Error,
    },
    #[snafu(display("data_dir {:?} does not exist", data_dir))]
    MissingDataDir { data_dir: PathBuf },
    #[snafu(display("data_dir {:?} is not writable", data_dir))]
    DataDirNotWritable { data_dir: PathBuf },
    #[snafu(display("journald error: {}", source))]
    JournaldError { source: ::journald::Error },
    #[snafu(display("Could not create Kafka consumer: {}", source))]
    KafkaCreateError { source: rdkafka::error::KafkaError },
    #[snafu(display("Could not subscribe to Kafka topics: {}", source))]
    KafkaSubscribeError { source: rdkafka::error::KafkaError },
}
