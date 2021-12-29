use std::{
    fmt::Debug,
    io,
    path::{Path, PathBuf},
};

use snafu::Snafu;

use self::leveldb_buffer::{Reader, Writer};
use crate::{buffer_usage_data::BufferUsageHandle, Bufferable};

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

/// Open a [`leveldb_buffer::Buffer`]
///
/// # Errors
///
/// This function will fail with [`DataDirError`] if the directory does not exist at
/// `data_dir`, if permissions are not sufficient etc.
pub fn open<T>(
    data_dir: &Path,
    name: &str,
    max_size: u64,
    usage_handle: BufferUsageHandle,
) -> Result<(Writer<T>, Reader<T>, super::Acker), DataDirError>
where
    T: Bufferable + Clone,
{
    let buffer_dir = format!("{}_id", name);
    let path = data_dir.join(buffer_dir);

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

    leveldb_buffer::Buffer::build(&path, max_size, usage_handle)
}
