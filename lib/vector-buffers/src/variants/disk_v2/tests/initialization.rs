use std::time::Duration;

use tokio::time::timeout;
use tracing::Instrument;

use crate::{
    test::{SizedRecord, acknowledge, install_tracing_helpers, with_temp_dir},
    variants::disk_v2::tests::{create_default_buffer_v2, set_file_length},
};

#[tokio::test]
async fn reader_doesnt_block_from_partial_write_on_last_record() {
    // When initializing, the reader will be catching up to the last record it read, which involves
    // reading individual records in the current reader data file until a record is returned whose
    // record ID matches the "last record ID read" field from the ledger.
    //
    // However, if the last record read by the reader was never fully synced to disk, we could be
    // left with a partial write: enough data to read the length delimiter, but not enough data to
    // actually read as many bytes as are indicated by said length delimiter.
    //
    // This would leave us waiting forever for bytes that will never come, because the writer isn't
    // going to do anything, as we're in initialization.
    //
    // This test ensures that if we hit a partial write during initialization, we correctly avoid
    // sitting around forever, waiting for a write that isn't coming.
    let _a = install_tracing_helpers();

    let fut = with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a regular buffer, no customizations required.
            let (mut writer, mut reader, ledger) = create_default_buffer_v2(data_dir.clone()).await;

            // Write a record, and then read it and acknowledge it. This puts the buffer into a
            // state where there's data in the current data file, and the ledger has a non-zero
            // record ID for where it thinks the reader needs to be. This ensures that the reader
            // actually does at least one call to `Reader::next` during `Reader::seek_to_next_record`.
            let first_bytes_written = writer
                .write_record(SizedRecord::new(64))
                .await
                .expect("should not fail to write");
            writer.flush().await.expect("flush should not fail");
            writer.close();

            let first_read = reader
                .next()
                .await
                .expect("should not fail to read record")
                .expect("should contain first record");
            assert_eq!(SizedRecord::new(64), first_read);
            acknowledge(first_read).await;

            let second_read = reader.next().await.expect("should not fail to read record");
            assert!(second_read.is_none());

            ledger.flush().expect("should not fail to flush ledger");

            // Grab the current writer data file path before dropping the buffer.
            let data_file_path = ledger.get_current_writer_data_file_path();
            drop(reader);
            drop(writer);
            drop(ledger);

            // Open the data file and drop the last eight bytes of the record, which will ensure
            // that there is less data available to read than the number of bytes indicated by the
            // record's length delimiter.
            let initial_len = first_bytes_written as u64;
            let target_len = initial_len - 8;
            set_file_length(&data_file_path, initial_len, target_len)
                .await
                .expect("should not fail to truncate data file");

            // Now reopen the buffer, which should complete in a timely fashion without an immediate error.
            let reopen = timeout(
                Duration::from_millis(500),
                create_default_buffer_v2::<_, SizedRecord>(data_dir),
            )
            .await;
            assert!(
                reopen.is_ok(),
                "failed to reopen buffer in a timely fashion; likely deadlock"
            );
        }
    });

    let parent = trace_span!("reader_doesnt_block_from_partial_write_on_last_record");
    fut.instrument(parent.or_current()).await;
}

#[tokio::test]
async fn reader_doesnt_block_when_ahead_of_last_record_in_current_data_file() {
    // When initializing, the reader will be catching up to the last record it read, which involves
    // reading individual records in the current reader data file until a record is returned whose
    // record ID matches the "last record ID read" field from the ledger.
    //
    // If the current data file contains a valid last record when we initialize, but that last
    // record is _behind_ the last record read as tracked by the ledger, then we need to ensure we
    // can break out of the catch-up loop when we get to the end of the current data file.
    //
    // Our existing logic for corrupted event detection, and the writer's own initialization logic,
    // will emit an error message when we realize that data is missing based on record ID gaps.
    let _a = install_tracing_helpers();

    let fut = with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a regular buffer, no customizations required.
            let (mut writer, mut reader, ledger) = create_default_buffer_v2(data_dir.clone()).await;

            // Write two records, and then read and acknowledge both.
            //
            // This puts the buffer into a state where there's data in the current data file, and
            // the ledger has a non-zero record ID for where it thinks the reader needs to be. This
            // ensures that the reader actually does at least two calls to `Reader::next` during
            // `Reader::seek_to_next_record`, which is necessary to ensure that the reader leaves
            // the default state of `self.last_reader_record_id == 0`.
            let first_bytes_written = writer
                .write_record(SizedRecord::new(64))
                .await
                .expect("should not fail to write");
            writer.flush().await.expect("flush should not fail");

            let second_bytes_written = writer
                .write_record(SizedRecord::new(68))
                .await
                .expect("should not fail to write");
            writer.flush().await.expect("flush should not fail");

            writer.close();

            let first_read = reader
                .next()
                .await
                .expect("should not fail to read record")
                .expect("should contain first record");
            assert_eq!(SizedRecord::new(64), first_read);
            acknowledge(first_read).await;

            let second_read = reader
                .next()
                .await
                .expect("should not fail to read record")
                .expect("should contain first record");
            assert_eq!(SizedRecord::new(68), second_read);
            acknowledge(second_read).await;

            let third_read = reader.next().await.expect("should not fail to read record");
            assert!(third_read.is_none());

            ledger.flush().expect("should not fail to flush ledger");

            // Grab the current writer data file path before dropping the buffer.
            let data_file_path = ledger.get_current_writer_data_file_path();
            drop(reader);
            drop(writer);
            drop(ledger);

            // Open the data file and truncate the second record. This will ensure that the reader
            // hits EOF after the first read, which we need to do in order to exercise the logic
            // that breaks out of the loop.
            let initial_len = first_bytes_written as u64 + second_bytes_written as u64;
            let target_len = first_bytes_written as u64;
            set_file_length(&data_file_path, initial_len, target_len)
                .await
                .expect("should not fail to truncate data file");

            // Now reopen the buffer, which should complete in a timely fashion without an immediate error.
            let reopen = timeout(
                Duration::from_millis(500),
                create_default_buffer_v2::<_, SizedRecord>(data_dir),
            )
            .await;
            assert!(
                reopen.is_ok(),
                "failed to reopen buffer in a timely fashion; likely deadlock"
            );
        }
    });

    let parent = trace_span!("reader_doesnt_block_when_ahead_of_last_record_in_current_data_file");
    fut.instrument(parent.or_current()).await;
}

#[tokio::test]
async fn restart_aligns_writer_with_reader_checkpoint_one_file_ahead() {
    let _a = install_tracing_helpers();

    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let (writer, reader, ledger) =
                create_default_buffer_v2::<_, SizedRecord>(data_dir.clone()).await;
            let previous_writer_file_id = ledger.get_current_writer_file_id();
            let reader_file_id = ledger.get_next_writer_file_id();

            // Model the durable state left when the reader abandons the writer's current file and
            // advances to the next file before the writer rolls there.
            ledger.increment_acked_reader_file_id();
            ledger.flush().expect("ledger flush should not fail");
            assert_eq!(reader_file_id, ledger.get_current_reader_file_id());
            assert_eq!(previous_writer_file_id, ledger.get_current_writer_file_id());
            assert!(
                !tokio::fs::try_exists(ledger.get_current_reader_data_file_path())
                    .await
                    .expect("checking reader data file should not fail")
            );

            drop(reader);
            drop(writer);
            drop(ledger);

            let (mut writer, mut reader, ledger) = timeout(
                Duration::from_millis(500),
                create_default_buffer_v2::<_, SizedRecord>(data_dir),
            )
            .await
            .expect("reader-ahead buffer should reopen without deadlocking");

            assert_eq!(reader_file_id, ledger.get_current_reader_file_id());
            assert_eq!(reader_file_id, ledger.get_current_writer_file_id());
            assert_eq!(0, ledger.get_total_buffer_size());

            writer
                .write_record(SizedRecord::new(64))
                .await
                .expect("write should not fail");
            writer.flush().await.expect("flush should not fail");
            let record = reader
                .next()
                .await
                .expect("read should not fail")
                .expect("record should be present");
            assert_eq!(SizedRecord::new(64), record);
        }
    })
    .await;
}
