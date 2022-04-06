use std::{io, num::NonZeroU64, path::Path};

use crate::{
    buffer_usage_data::BufferUsageHandle,
    topology::{builder::IntoBuffer, channel::ReceiverAdapter},
    variants::{
        disk_v2::{build_disk_v2_buffer, get_disk_v2_data_dir_path},
        DiskV1Buffer,
    },
    Acker, Bufferable, WhenFull,
};

pub async fn try_disk_v1_migration<T>(base_data_dir: &Path, id: &str) -> Result<(), String>
where
    T: Bufferable + Clone,
{
    // Set both buffers to an essentially unlimited size so we can ensure that whatever is in the
    // source buffer can be written to the destination buffer.
    let usage_handle = BufferUsageHandle::noop(WhenFull::Block);
    let buffer_max_size = NonZeroU64::new(u64::MAX).expect("cannot fail");

    // Try and build the disk v1 buffer without creating it if it is missing, so that we only open
    // it if it actually exists. If this throws an error, we just bubble it up since the user
    // shouldn't be trying to migrate a buffer that doesn't actually exist.
    let mut src_buffer =
        DiskV1Buffer::new(id.to_string(), base_data_dir.to_path_buf(), buffer_max_size);
    src_buffer.set_migration_mode();

    let src_buffer_dir = src_buffer.get_buffer_dir();

    // Before literally opening the old buffer, we check if the data directory even exists where the
    // disk v1 buffer would theoretically exist. Since LevelDB will create the directory even if we
    // specify `create_if_missing = false`, we want to return early if it doesn't already exist,
    // which keeps us from (re)creating the directory, even if it just ends up being empty:
    if let Err(e) = std::fs::metadata(&src_buffer_dir) {
        match e.kind() {
            // Nothing to migrate if it doesn't exist.
            io::ErrorKind::NotFound => return Ok(()),
            // Either permissions or something more insidious, but Vector should have permissions to
            // the top-level data directory regardless, so it's worthwhile to bubble up either way.
            ek => {
                return Err(format!(
                    "Failed to query for existence of `disk_v1`-based disk buffer at '{}': {:?}",
                    src_buffer_dir.to_string_lossy(),
                    ek,
                ))
            }
        }
    }

    let src_buffer = Box::new(src_buffer);
    let (mut src_reader, src_acker): (ReceiverAdapter<T>, Acker) =
        match src_buffer.into_buffer_parts(usage_handle.clone()).await {
            Ok((_, src_reader, src_acker)) => (
                src_reader,
                src_acker.expect("disk v1 buffer acker must exist"),
            ),
            // If the disk v1 buffer doesn't exist, then that's OK, just return early.
            Err(_) => return Ok(()),
        };

    let dst_buffer_dir = get_disk_v2_data_dir_path(base_data_dir, id);

    let (mut dst_writer, _, _) =
        build_disk_v2_buffer(usage_handle, base_data_dir, id, buffer_max_size)
            .await
            .map_err(|e| format!("Failed to build `disk_v2` buffer: {}", e))?;

    // Now that we've got our source and destination buffers configured, read each record from the
    // source and write it to the destination. If the write succeeds, we acknowledge it in the
    // source so that it can't be mistakenly read again if Vector starts up and reads the buffer, or
    // if the migration stops and must be restarted.
    info!("Detected old `disk_v1`-based buffer for the `{}` sink. Automatically migrating to `disk_v2`.", id);

    let mut migrated_records = 0;
    while let Some(old_record) = src_reader.next().await {
        let old_record_event_count = old_record.event_count();

        dst_writer.write_record(old_record).await.map_err(|e| {
            format!(
                "failed writing record {} to the new disk v2 buffer: {}",
                migrated_records, e,
            )
        })?;

        dst_writer.flush().await.map_err(|e| {
            format!(
                "failed flushing record {} to the new disk v2 buffer: {}",
                migrated_records, e,
            )
        })?;

        src_acker.ack(old_record_event_count);
        migrated_records += old_record_event_count;
    }

    // We've successfully migrated all of the records from the disk v1 buffer to the disk v2 buffer.
    // Yippee!  Now, let's remove the old disk v1 data directory to finalize the migration.
    drop(src_reader);
    drop(src_acker);

    if std::fs::remove_dir_all(&src_buffer_dir).is_err() {
        error!(
            "Failed to delete the old disk buffer data directory at '{}'.  You can safely delete it manually at this point.",
            src_buffer_dir.to_string_lossy(),
        );
    }

    info!(
        "Migrated {} records in disk buffer for `{}` sink. Old disk buffer at '{}' has been deleted, and the new disk buffer has been created at '{}'.",
        migrated_records,
        id,
        src_buffer_dir.to_string_lossy(),
        dst_buffer_dir.to_string_lossy(),
    );

    Ok(())
}
