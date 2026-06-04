use std::time::Duration;

use tokio::{fs, time::timeout};
use tracing::Instrument;

use crate::{
    buffer_usage_data::BufferUsageHandle,
    test::{SizedRecord, acknowledge, install_tracing_helpers, with_temp_dir},
    variants::disk_v2::{
        Buffer, DiskBufferConfigBuilder,
        tests::{
            create_buffer_v2_with_max_data_file_size, create_default_buffer_v2,
            get_minimum_data_file_size_for_record_payload, set_file_length,
        },
    },
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
async fn reopen_recovers_when_reader_resume_data_file_is_missing() {
    // Regression test for SMPTNG-749: a crash that loses the reader's un-fsync'd
    // file-id advance leaves a deleted data file the ledger still resumes from.
    // Reopen must skip it and recover instead of crash-looping.
    //
    // `reader::delete_completed_data_file` is not atomic, performing the following operations:
    //
    //  * unlink dat file
    //  * update ack'd reader file ID in ledger
    //  * sync ledger
    //
    // If the final sync is not made before the Vector process terminates -- if
    // it is SIGKILL'ed for instance -- then the dat file will be gone but the
    // reader will be made to open a missing file and will crash.
    //
    // This test stages one instance of that crash and asserts the reopen recovers.
    // In the scenario below the reader is at data file 0 and the writer at 1. That is:
    //
    //   1. writer writes 2 records -- data-0, data-1 -- leaving the system
    //      in state reader=0, writer=1.
    //   2. reader reads and acks data-0; delete runs unlink(data-0) -> set
    //      reader=1 -> CRASH
    //   3. on restart reader=0 != writer=1, read(data-0) -> ENOENT ->
    //      ReaderSeekFailed
    //
    // The test stages the crash in step 2 without a real crash, copying
    // buffer.db from step 1, running the real read+ack+delete and then
    // restoring buffer.db, as if the disk buffer had been booted cold after a
    // crash.
    let _a = install_tracing_helpers();

    let fut = with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let record = SizedRecord::new(64);
            // We size things so that one record write per data file means the
            // writer will roll to the next dat file.
            let max_data_file_size = get_minimum_data_file_size_for_record_payload(&record);

            let (mut writer, mut reader, ledger) =
                create_buffer_v2_with_max_data_file_size::<_, SizedRecord>(
                    data_dir.clone(),
                    max_data_file_size,
                )
                .await;

            // Write two records, one per data file, so file 0 fills and the writer
            // rolls ahead onto file 1.
            for _ in 0..2 {
                writer
                    .write_record(record.clone())
                    .await
                    .expect("write should not fail");
                writer.flush().await.expect("flush should not fail");
            }
            writer.close();

            // We can't actually SIGKILL this process, so snapshot buffer.db while the
            // ledger still resumes from file 0; this copy is the durable state a crash
            // leaves (see the comment header above).
            let ledger_db = data_dir.join("buffer.db");
            ledger.flush().expect("ledger flush should not fail");
            let durable_snapshot = fs::read(&ledger_db).await.expect("snapshot buffer.db");

            // Read and acknowledge file 0's record, then file 1's, which rolls the
            // reader off file 0. The next read processes those acks and runs the real
            // delete_completed_data_file, unlinking file 0 and draining the buffer.
            acknowledge(
                reader
                    .next()
                    .await
                    .expect("should not fail to read record")
                    .expect("file 0 record"),
            )
            .await;
            acknowledge(
                reader
                    .next()
                    .await
                    .expect("should not fail to read record")
                    .expect("file 1 record"),
            )
            .await;
            assert!(
                reader.next().await.expect("read should not fail").is_none(),
                "the buffer drains after both records"
            );

            // Simulate the crash.
            //
            // We drop down reader, writer and ledger and then replace the
            // ledger_db with the snapshot we took earlier, simulating a ledger
            // shutdown that managed to unlink the reader file but not fsync its
            // private storage.
            drop(reader);
            drop(writer);
            drop(ledger);
            fs::write(&ledger_db, &durable_snapshot)
                .await
                .expect("failed to restore buffer.db");

            // Simulate the reboot: reopen the disk buffer. It resumes from the unlinked
            // data-0 and fails to open.
            let build_config = || {
                DiskBufferConfigBuilder::from_path(data_dir.clone())
                    .max_data_file_size(max_data_file_size)
                    .max_record_size(usize::try_from(max_data_file_size).unwrap())
                    .build()
                    .expect("config build should not fail")
            };
            let first =
                Buffer::<SizedRecord>::from_config_inner(build_config(), BufferUsageHandle::noop())
                    .await;
            // Consume `first` so the internals are all consumed, locks are
            // released and so forth.
            let first_err = first.err();

            // Simulate another reboot.
            //
            // We re-write the ledger to simulate what happens when disk buffer
            // is on a durable store but not co-local to the machine. If the
            // disk and compute are co-local then the mmap with the right
            // indexes _may_ be present in OS page cache and _may_ restart
            // properly on the next restart. This will not be true if everything
            // is cold, which is what we simulate.
            //
            // Try removing this fs::write. Depending on how busy your system is
            // you may find that this second attempt does not crash.
            fs::write(&ledger_db, &durable_snapshot)
                .await
                .expect("failed to restore buffer.db");
            let second =
                Buffer::<SizedRecord>::from_config_inner(build_config(), BufferUsageHandle::noop())
                    .await;
            let second_err = second.err();

            assert!(
                first_err.is_none() && second_err.is_none(),
                "SMPTNG-749: every reboot must recover (crash-loop); \
                 first={first_err:?} second={second_err:?}",
            );
        }
    });

    let parent = trace_span!("reopen_recovers_when_reader_resume_data_file_is_missing_smptng_749");
    fut.instrument(parent.or_current()).await;
}
