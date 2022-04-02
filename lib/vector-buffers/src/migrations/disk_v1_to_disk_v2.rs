use std::{num::NonZeroU64, path::PathBuf};

use crate::{
    buffer_usage_data::BufferUsageHandle,
    topology::{builder::IntoBuffer, channel::ReceiverAdapter},
    variants::{DiskV1Buffer, DiskV2Buffer},
    Acker, Bufferable, WhenFull,
};

/// Migrates a disk v1 buffer to a disk v2 buffer.
///
/// # Errors
///
/// If any error occurs during the loading of the old disk v1 buffer, creation/loading of the new
/// disk v2 buffer, or during the reading/writing of old records to the new buffer, an error string
/// will be returned explaining the error.
pub async fn migrate_disk_v1_to_disk_v2<T: Bufferable + Clone>(
    data_dir: PathBuf,
    buffer_id: String,
) -> Result<(), String> {
    // Set both buffers to an essentially unlimited size so we can ensure that whatever is in the
    // source buffer can be written to the destination buffer.
    let usage_handle = BufferUsageHandle::noop(WhenFull::Block);
    let buffer_max_size = NonZeroU64::new(u64::MAX).expect("cannot fail");

    // Try and build the disk v1 buffer without creating it if it is missing, so that we only open
    // it if it actually exists. If this throws an error, we just bubble it up since the user
    // shouldn't be trying to migrate a buffer that doesn't actually exist.
    let mut src_buffer = DiskV1Buffer::new(buffer_id.clone(), data_dir.clone(), buffer_max_size);
    src_buffer.no_create();

    let src_buffer_dir = src_buffer.get_buffer_path();

    let src_buffer = Box::new(src_buffer);
    let (_, mut src_reader, src_acker): (_, ReceiverAdapter<T>, Option<Acker>) = src_buffer
        .into_buffer_parts(usage_handle.clone())
        .await
        .map_err(|e| {
            format!(
                "Existing disk v1 buffer for sink '{}' could not be opened: {}",
                buffer_id, e
            )
        })?;
    let src_acker = src_acker.expect("disk v1 buffer must provide a real acker");

    // Now create a disk v2 buffer and prepare to write over the records.
    let dst_buffer = DiskV2Buffer::new(buffer_id.clone(), data_dir, buffer_max_size);

    let dst_buffer = Box::new(dst_buffer);
    let (mut dst_writer, _, _) = dst_buffer
        .into_buffer_parts(usage_handle)
        .await
        .map_err(|e| {
            format!(
                "New disk v2 buffer for sink '{}' could not be created: {}",
                buffer_id, e
            )
        })?;

    // Now that we've got our source and destination buffers configured, read each record from the
    // source and write it to the destination. If the write succeeds, we acknowledge it in the
    // source so that it can't be mistakenly read again if Vector starts up and reads the buffer, or
    // if the migration stops and must be restarted.
    info!("Starting migration.");

    let mut migrated_records = 0;
    while let Some(old_record) = src_reader.next().await {
        let old_record_event_count = old_record.event_count();

        dst_writer.send(old_record).await.map_err(|()| {
            format!(
                "failed writing record {} to the new disk v2 buffer",
                migrated_records
            )
        })?;

        dst_writer.flush().await.map_err(|()| {
            format!(
                "failed flushing record {} to the new disk v2 buffer",
                migrated_records
            )
        })?;

        src_acker.ack(old_record_event_count);
        migrated_records += old_record_event_count;
    }

    // We've successfully migrated all of the records from the disk v1 buffer to the disk v2 buffer.
    // Yippee!  Now, let's remove the old disk v1 data directory to finalize the migration.
    drop(src_reader);
    drop(src_acker);

    std::fs::remove_dir_all(&src_buffer_dir).map_err(|_| {
        format!(
            "failed to delete old disk v1 buffer directory '{}'; it can be deleted manually",
            src_buffer_dir.to_string_lossy()
        )
    })?;

    info!(
		"Migrated {} records from old disk v1 buffer at '{}' to new disk v2 buffer, and deleted old buffer data directory.",
		migrated_records,
		src_buffer_dir.to_string_lossy(),
	);

    Ok(())
}
