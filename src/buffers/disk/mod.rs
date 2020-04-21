#![cfg(feature = "leveldb")]

use crate::event::Event;
use futures01::{Async, AsyncSink, Sink, Stream};
use snafu::Snafu;
use std::io;
use std::path::{Path, PathBuf};

pub mod leveldb_buffer;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("The configured data_dir {:?} does not exist, please create it and make sure the vector process can write to it", data_dir))]
    DataDirNotFound { data_dir: PathBuf },
    #[snafu(display("The configured data_dir {:?} is not writable by the vector process, please ensure vector can write to that directory", data_dir))]
    DataDirNotWritable { data_dir: PathBuf },
    #[snafu(display("Unable to look up data_dir {:?}", data_dir))]
    DataDirMetadataError {
        data_dir: PathBuf,
        source: std::io::Error,
    },
    #[snafu(display("Unable to open data_dir {:?}", data_dir))]
    DataDirOpenError {
        data_dir: PathBuf,
        source: leveldb::database::error::Error,
    },
}

pub trait DiskBuffer {
    type Writer: Sink<SinkItem = Event, SinkError = ()>;
    type Reader: Stream<Item = Event, Error = ()> + Send;

    fn build(
        path: PathBuf,
        max_size: usize,
    ) -> Result<(Self::Writer, Self::Reader, super::Acker), Error>;
}

#[derive(Clone)]
pub struct Writer {
    inner: leveldb_buffer::Writer,
}

impl Sink for Writer {
    type SinkItem = Event;
    type SinkError = ();

    fn start_send(
        &mut self,
        event: Self::SinkItem,
    ) -> Result<AsyncSink<Self::SinkItem>, Self::SinkError> {
        self.inner.start_send(event)
    }

    fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
        self.inner.poll_complete()
    }
}

pub fn open(
    data_dir: &Path,
    name: &str,
    max_size: usize,
) -> Result<
    (
        Writer,
        Box<dyn Stream<Item = Event, Error = ()> + Send>,
        super::Acker,
    ),
    Error,
> {
    let path = data_dir.join(name);

    // Check data dir
    std::fs::metadata(&data_dir)
        .map_err(|e| match e.kind() {
            io::ErrorKind::PermissionDenied => Error::DataDirNotWritable {
                data_dir: data_dir.into(),
            },
            io::ErrorKind::NotFound => Error::DataDirNotFound {
                data_dir: data_dir.into(),
            },
            _ => Error::DataDirMetadataError {
                data_dir: data_dir.into(),
                source: e,
            },
        })
        .and_then(|m| {
            if m.permissions().readonly() {
                Err(Error::DataDirNotWritable {
                    data_dir: data_dir.into(),
                })
            } else {
                Ok(())
            }
        })?;

    let (writer, reader, acker) = leveldb_buffer::Buffer::build(path, max_size)?;
    Ok((Writer { inner: writer }, Box::new(reader), acker))
}
