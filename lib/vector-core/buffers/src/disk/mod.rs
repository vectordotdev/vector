use crate::bytes::{DecodeBytes, EncodeBytes};
use crate::internal_events::EventsSent;
use futures::StreamExt;
use futures::{Sink, Stream};
use core_common::internal_event::emit;
use pin_project::pin_project;
use snafu::Snafu;
use std::fmt::Debug;
use std::fmt::Display;
use std::mem::size_of_val;
use std::{
    io,
    path::{Path, PathBuf},
    pin::Pin,
    task::{Context, Poll},
};

pub mod leveldb_buffer;

#[derive(Debug, Snafu)]
pub enum DataDirError {
    #[snafu(display("The configured data_dir {:?} does not exist, please create it and make sure the vector process can write to it", data_dir))]
    NotFound { data_dir: PathBuf },
    #[snafu(display("The configured data_dir {:?} is not writable by the vector process, please ensure vector can write to that directory", data_dir))]
    NotWritable { data_dir: PathBuf },
    #[snafu(display("Unable to look up data_dir {:?}: {:?}", data_dir, source))]
    Metadata {
        data_dir: PathBuf,
        source: std::io::Error,
    },
    #[snafu(display("Unable to open data_dir {:?}: {:?}", data_dir, source))]
    Open {
        data_dir: PathBuf,
        source: leveldb::database::error::Error,
    },
}

#[pin_project]
#[derive(Clone)]
pub struct Writer<T>
where
    T: Send + Sync + Unpin + Clone + EncodeBytes<T> + DecodeBytes<T>,
    <T as EncodeBytes<T>>::Error: Debug,
    <T as DecodeBytes<T>>::Error: Debug,
{
    #[pin]
    inner: leveldb_buffer::Writer<T>,
}

impl<T> Sink<T> for Writer<T>
where
    T: Send + Sync + Unpin + Clone + EncodeBytes<T> + DecodeBytes<T>,
    <T as EncodeBytes<T>>::Error: Debug,
    <T as DecodeBytes<T>>::Error: Debug + Display,
{
    type Error = ();
    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        self.project().inner.start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_close(cx)
    }
}

/// Open a [`leveldb_buffer::Buffer`]
///
/// # Errors
///
/// This function will fail with [`DataDirError`] if the directory does not exist at
/// `data_dir`, if permissions are not sufficient etc.
pub fn open<'a, T>(
    data_dir: &Path,
    name: &str,
    max_size: usize,
) -> Result<
    (
        Writer<T>,
        Box<dyn Stream<Item = T> + 'a + Unpin + Send>,
        super::Acker,
    ),
    DataDirError,
>
where
    T: 'a + Send + Sync + Unpin + Clone + EncodeBytes<T> + DecodeBytes<T>,
    <T as EncodeBytes<T>>::Error: Debug,
    <T as DecodeBytes<T>>::Error: Debug + Display,
{
    let path = data_dir.join(name);

    // Check data dir
    std::fs::metadata(&data_dir)
        .map_err(|e| match e.kind() {
            io::ErrorKind::PermissionDenied => DataDirError::NotWritable {
                data_dir: data_dir.into(),
            },
            io::ErrorKind::NotFound => DataDirError::NotFound {
                data_dir: data_dir.into(),
            },
            _ => DataDirError::Metadata {
                data_dir: data_dir.into(),
                source: e,
            },
        })
        .and_then(|m| {
            if m.permissions().readonly() {
                Err(DataDirError::NotWritable {
                    data_dir: data_dir.into(),
                })
            } else {
                Ok(())
            }
        })?;

    let (writer, reader, acker) = leveldb_buffer::Buffer::build(&path, max_size)?;
    let reader = reader.inspect(|item| {
        emit(&EventsSent {
            count: 1,
            byte_size: size_of_val(item),
        });
    });
    Ok((Writer { inner: writer }, Box::new(reader), acker))
}
