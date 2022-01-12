use std::{
    fmt::Debug,
    io,
    path::{Path, PathBuf},
};

use snafu::Snafu;

use self::leveldb_buffer::{db_initial_size, Reader, Writer};
use crate::{buffer_usage_data::BufferUsageHandle, Bufferable};

pub mod leveldb_buffer;

const OLD_AND_NEW_DATA_DIR_WARNING: &str = "detected an old-style disk buffer data directory while utilizing a new-style disk buffer data directory!

this may indicate that you upgraded to 0.19.x prior to a regression being fixed which deals with disk buffer directory names.  see https://github.com/vectordotdev/vector/issues/10430 for more information about this situation.";

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
    // Make sure we have permissions to read/write to the top-level data directory.
    check_data_dir_permissions(data_dir)?;

    // In #10379, we introduced a regrression by changing the data directory used for disk v1
    // buffers, going from `<component name>_buffer` to `<component name>_id`.  I still don't really
    // know why I made this change, but it was made, and released.  So now we have to compensate for
    // it.
    //
    // The following logic is meant to try and offer a more graceful transition between the two, if
    // possible.  We'll refer to the `_buffer` approach as the "old style" and the `_id` approach as
    // the "new style".
    //
    // Logic of this transition code:
    // - if there is not an existing directory matching the old style path, then use the new style
    //   path (regardless of whether or not the new style path exists)
    // - if there is a data directory for both the old style and new style paths, use the new style
    //   path, but we also try opening the database under the old style path to see if it still
    //   contains any data:
    //   -- if there is any data in the old style path, emit a warn log that
    //      there is still data in the old style path, which may represent older records --- if there
    //      is no data in the old style path, then delete the old style path entirely and simply move
    //      forward with the new style path
    //   -- if there is a data directory for the old style path only, move it to the "new style" path,
    //      and use the new style path
    //
    // This should provide an effective transition for users switching to 0.19.1 from 0.18.0 or
    // earlier that transparently switches directories if there is no data in the old buffer, but
    // warns them appropriately if there is.  We don't try to read the old data preferentially
    // because we might otherwise introduce old data that could mess up observability pipelines.
    //
    // For new users starting out cleanly with 0.19.0 or higher, there's no change in behavior.
    let buffer_id = format!("{}_id", name);
    let path = data_dir.join(buffer_id);
    let path_exists = check_data_dir_exists(&path)?;

    let old_buffer_id = format!("{}_buffer", name);
    let old_path = data_dir.join(old_buffer_id);
    let old_path_exists = check_data_dir_exists(&old_path)?;

    if old_path_exists {
        if path_exists {
            // Both old style and new style paths exist.  We check if the old style path has any data,
            // and if it does, we emit a warning log because since the new style path exists, we don't
            // want to risk missing data on that side by trying to read the old data first and then
            // moving to the new data, etc.
            //
            // If there's no data in the old style path, though, we just delete the directory and move
            // on: no need to emit anything because nothing is being lost.
            let (existing_byte_size, existing_record_count) = db_initial_size(&old_path)?;
            if existing_byte_size != 0 || existing_record_count != 0 {
                // The old style path still has some data, so all we're going to do is warn the user
                // that this is the case, since we don't want to risk reading older records that
                // they've moved on from after switching to the new style path.
                warn!(
                    message = OLD_AND_NEW_DATA_DIR_WARNING,
                    existing_record_count, existing_byte_size,
                );
            } else {
                // The old style path has no more data.  Theoretically, we should be able to delete
                // it, but that's a bit risky, so we just rename it instead.
                let sidelined_buffer_id = format!("{}_buffer_old", name);
                let sidelined_path = data_dir.join(sidelined_buffer_id);

                std::fs::rename(&old_path, &sidelined_path)
                    .map_err(|e| map_io_error(e, &sidelined_path))?;
            }
        } else {
            // Old style path exists, but not the new style path.  Move the old style path to the
            // new style path and then use the new style path going forward.
            std::fs::rename(&old_path, &path).map_err(|e| map_io_error(e, &path))?;
        }
    }

    leveldb_buffer::Buffer::build(&path, max_size, usage_handle)
}

fn map_io_error<P>(e: io::Error, data_dir: P) -> DataDirError
where
    P: AsRef<Path>,
{
    match e.kind() {
        io::ErrorKind::PermissionDenied => DataDirError::NotWritable {
            data_dir: data_dir.as_ref().to_path_buf(),
        },
        io::ErrorKind::NotFound => DataDirError::NotFound {
            data_dir: data_dir.as_ref().to_path_buf(),
        },
        _ => DataDirError::Metadata {
            data_dir: data_dir.as_ref().to_path_buf(),
            source: e,
        },
    }
}

fn check_data_dir_exists<P>(data_dir: P) -> Result<bool, DataDirError>
where
    P: AsRef<Path>,
{
    std::fs::metadata(&data_dir)
        .map(|m| m.is_dir())
        .or_else(|e| match map_io_error(e, &data_dir) {
            DataDirError::NotFound { .. } => Ok(false),
            de => Err(de),
        })
}

fn check_data_dir_permissions<P>(data_dir: P) -> Result<(), DataDirError>
where
    P: AsRef<Path>,
{
    std::fs::metadata(&data_dir)
        .map_err(|e| map_io_error(e, &data_dir))
        .and_then(|m| {
            if m.permissions().readonly() {
                Err(DataDirError::NotWritable {
                    data_dir: data_dir.as_ref().to_path_buf(),
                })
            } else {
                Ok(())
            }
        })
}
