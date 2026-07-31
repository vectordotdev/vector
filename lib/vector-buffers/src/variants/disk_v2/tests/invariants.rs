use tokio::{fs::OpenOptions, io::AsyncWriteExt};
use tokio_test::{assert_pending, assert_ready, task::spawn};
use tracing::Instrument;

use super::{create_buffer_v2_with_max_data_file_size, read_next, read_next_some};
use crate::{
    EventCount, assert_buffer_is_empty, assert_buffer_records, assert_buffer_size,
    assert_enough_bytes_written, assert_reader_last_writer_next_positions,
    assert_reader_writer_v2_file_positions, set_data_file_length,
    test::{MultiEventRecord, SizedRecord, acknowledge, install_tracing_helpers, with_temp_dir},
    variants::disk_v2::{
        common::{DEFAULT_FLUSH_INTERVAL, MAX_FILE_ID},
        tests::{
            create_buffer_v2_with_write_buffer_size, create_default_buffer_v2,
            get_corrected_max_record_size, get_minimum_data_file_size_for_record_payload,
        },
    },
};

#[tokio::test]
async fn pending_read_returns_none_when_writer_closed_with_unflushed_write() {
    let assertion_registry = install_tracing_helpers();

    let fut = with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a normal buffer.
            let (mut writer, mut reader, ledger) = create_default_buffer_v2(data_dir.clone()).await;

            // Attempt a read, which should block because there's no data yet.  We specifically want
            // to make sure we end up waiting for the writer to indicate that we're waiting
            // explicitly on the writer making progress of some sort.
            let waiting_for_writer = assertion_registry
                .build()
                .with_name("wait_for_writer")
                .with_parent_name(
                    "pending_read_returns_none_when_writer_closed_with_unflushed_write",
                )
                .was_entered()
                .finalize();

            let mut blocked_read = spawn(read_next(&mut reader));
            while !waiting_for_writer.try_assert() {
                assert_pending!(blocked_read.poll());
            }

            // Make sure we're fully pending and aren't yet woken.
            assert_pending!(blocked_read.poll());
            assert!(!blocked_read.is_woken());

            // Write a small record but _don't_ flush it.
            let bytes_written = writer
                .write_record(SizedRecord::new(64))
                .await
                .expect("write should not fail");
            assert_enough_bytes_written!(bytes_written, SizedRecord, 64);

            // We drop the writer/ledger which will close the writer, and closing the writer should
            // notify the reader.  While the writer waking up the reader is meant to inform the
            // reader of write-side progress, in this case, there was no progress: a flush was not
            // initiated, and so the buffer is in an inconsistent state where the ledger believes the
            // buffer to not be empty, and that the reader should continue trying to wait for the
            // data to come through.
            //
            // However, we track enough information on the writer side that we can let the reader
            // finish cleanly.  The next time we open the buffer and read the next record, we'll
            // detect any discontinuity and handle it as we normally would.
            drop(writer);
            drop(ledger);

            assert!(blocked_read.is_woken());
            let blocked_read_result = assert_ready!(blocked_read.poll());
            assert_eq!(blocked_read_result, None);
        }
    });

    let parent = trace_span!("pending_read_returns_none_when_writer_closed_with_unflushed_write");
    fut.instrument(parent.or_current()).await;
}

#[tokio::test]
async fn publishing_writer_progress_updates_ledger_before_waking_reader() {
    let _a = install_tracing_helpers();

    let fut = with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let (_writer, _reader, ledger) =
                create_default_buffer_v2::<_, SizedRecord>(data_dir).await;

            let mut wait_for_writer = spawn(ledger.wait_for_writer());
            assert_pending!(wait_for_writer.poll());
            assert!(!wait_for_writer.is_woken());

            let next_record_id = ledger.publish_writer_progress(1, 128);

            assert_eq!(next_record_id, 2);
            assert_eq!(ledger.state().get_next_writer_record_id(), 2);
            assert_eq!(ledger.get_total_buffer_size(), 128);
            assert!(wait_for_writer.is_woken());
            assert_ready!(wait_for_writer.poll());
        }
    });

    let parent = trace_span!("publishing_writer_progress_updates_ledger_before_waking_reader");
    fut.instrument(parent.or_current()).await;
}

#[tokio::test]
async fn last_record_is_valid_during_load_when_buffer_correctly_flushed_and_stopped() {
    let assertion_registry = install_tracing_helpers();

    let fut = with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let writer_did_not_call_reset = assertion_registry
                .build()
                .with_name("reset")
                .with_parent_name(
                    "last_record_is_valid_during_load_when_buffer_correctly_flushed_and_stopped",
                )
                .was_not_entered()
                .finalize();

            // Create a normal buffer.
            let (mut writer, _, ledger) = create_default_buffer_v2(data_dir.clone()).await;
            let bytes_written = writer
                .write_record(SizedRecord::new(64))
                .await
                .expect("write should not fail");
            assert_enough_bytes_written!(bytes_written, SizedRecord, 64);

            writer.flush().await.expect("flush should not fail");
            ledger.flush().expect("flush should not fail");

            drop(writer);
            drop(ledger);

            // Make sure we can open the buffer again without any errors.
            let (_, _, ledger) = create_default_buffer_v2::<_, SizedRecord>(data_dir).await;
            assert_eq!(ledger.get_total_records(), 1);
            writer_did_not_call_reset.assert();
        }
    });

    let parent =
        trace_span!("last_record_is_valid_during_load_when_buffer_correctly_flushed_and_stopped");
    fut.instrument(parent.or_current()).await;
}

#[tokio::test]
async fn file_id_wraps_around_when_max_file_id_hit() {
    let _a = install_tracing_helpers();
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let record_size = 100;
            let record = SizedRecord::new(record_size);
            let max_data_file_size = get_minimum_data_file_size_for_record_payload(&record);

            // Create our buffer with an arbitrarily low max data file size, which will let us
            // quickly run through the file ID range.
            let (mut writer, mut reader, ledger) =
                create_buffer_v2_with_max_data_file_size(data_dir, max_data_file_size).await;

            assert_buffer_is_empty!(ledger);
            assert_reader_writer_v2_file_positions!(ledger, 0, 0);

            // We execute a loop of writing and then reading back a record, and we assert each time
            // that the file IDs are where we expect them to be.  We write 3x the number of records
            // as the max possible file ID, to ensure that rollover works.
            let file_id_upper = MAX_FILE_ID;

            // Random addition at the end so we don't land explicitly on the u16 boundary.
            let target_id = (u32::from(file_id_upper) * 3) + 15;

            let mut id = 0;
            let mut reader_file_id = 0;
            let mut writer_file_id = 0;
            while id < target_id {
                let bytes_written = writer
                    .write_record(record.clone())
                    .await
                    .expect("write should not fail");
                assert_enough_bytes_written!(bytes_written, SizedRecord, record_size);

                writer.flush().await.expect("flush should not fail");

                let record_read = read_next_some(&mut reader).await;
                assert_eq!(record_read, record.clone());

                acknowledge(record_read).await;

                let expected_file_id = u16::try_from(id % u32::from(file_id_upper))
                    .expect("should never be greater than u16");
                let (actual_reader_file_id, actual_writer_file_id) =
                    ledger.get_current_reader_writer_file_id();
                reader_file_id = actual_reader_file_id;
                writer_file_id = actual_writer_file_id;

                // Record count/total size will always match the write we just did because
                // acknowledgement is only driven by calls to `next`, but our reader/writer should
                // be in lockstep, since no data files are closed/adjusted before a read/write
                // complete, only once we attempt the next one.
                assert_eq!(reader_file_id, writer_file_id);
                assert_eq!(expected_file_id, reader_file_id);
                assert_eq!(expected_file_id, writer_file_id);
                assert_buffer_size!(ledger, 1, bytes_written);

                id += 1;
            }

            writer.close();

            // After closing the writer, our final read should tell us that the buffer is closed,
            // but as important, it should tell us that the reader/writer file IDs haven't changed
            // since we left the loop _and_ that they're still in lockstep.
            let final_read = read_next(&mut reader).await;
            assert_eq!(final_read, None);
            assert_buffer_is_empty!(ledger);
            assert_reader_writer_v2_file_positions!(ledger, reader_file_id, writer_file_id);
        }
    })
    .await;
}

#[tokio::test]
async fn writer_stops_when_hitting_file_that_reader_is_still_on() {
    let assertion_registry = install_tracing_helpers();

    let fut = with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let record_size = 100;
            let record = SizedRecord::new(record_size);
            let max_data_file_size = get_minimum_data_file_size_for_record_payload(&record);

            // Create our buffer with an arbitrarily low max data file size, which will let us
            // quickly run through the file ID range.
            let (mut writer, mut reader, ledger) =
                create_buffer_v2_with_max_data_file_size(data_dir, max_data_file_size).await;

            assert_buffer_is_empty!(ledger);
            assert_reader_writer_v2_file_positions!(ledger, 0, 0);

            // We execute a loop of writing enough records to consume all data files, without doing
            // any reading.
            let file_id_upper = u32::from(MAX_FILE_ID);

            let mut id = 0;
            let mut total_size = 0;
            while id < file_id_upper {
                let bytes_written = writer
                    .write_record(record.clone())
                    .await
                    .expect("write should not fail");
                assert_enough_bytes_written!(bytes_written, SizedRecord, record_size);

                writer.flush().await.expect("flush should not fail");

                assert_reader_writer_v2_file_positions!(ledger, 0, id);

                id += 1;
                total_size += bytes_written;
            }

            assert_buffer_size!(ledger, MAX_FILE_ID, total_size);
            assert_reader_writer_v2_file_positions!(ledger, 0, MAX_FILE_ID - 1);

            let assertion = assertion_registry
                .build()
                .with_name("wait_for_reader")
                .with_parent_name("writer_stops_when_hitting_file_that_reader_is_still_on")
                .was_entered()
                .finalize();

            // Now we should be consuming all data files, and our next write should block trying to
            // open the "first" data file until we do a read.
            let mut blocked_write = spawn(writer.write_record(record));

            // You might be looking at the assert_pending! calls below and wondering what's
            // happening there.  Essentially, the process of doing a read or write could contain a
            // variable number of asynchronous steps required to open the data file, or wait for the
            // reader/writer to make progress, and so on.  Since we're executing real file I/O
            // operations in these tests, these things aren't deterministic.
            //
            // Rather than transform all of the code so it can be fully mocked and controlled, we've
            // opted for a lightweight approach where we assert conditions around tracing spans, in
            // the sense of asserting that certain spans have been entered, and so on.
            //
            // We're trying to make sure our code gets to the point of waiting for the reader to
            // wake up, which would imply that a reader needs to issue a wake-up for progress to be
            // made.  We create an assertion that looks for that, and we fallibly assert it in a
            // loop while polling the blocked write to drive it forward.  Once that assertion
            // becomes true, we know the blocked write is now waiting on the reader.
            //
            // There might still be spurious wakeups from some of the other asynchronous code in the
            // call, but our blocked write will _not_ proceed until the reader itself specifically
            // wakes it up, which is all that matters for our logic.
            while !assertion.try_assert() {
                assert_pending!(blocked_write.poll());
            }
            assert_pending!(blocked_write.poll());

            // Now execute a read which will pull the first record.  This doesn't yet delete the
            // first data file since we haven't acknowledged the read yet, so the file can't yet be
            // deleted.
            let first_record_read = read_next_some(&mut reader).await;
            assert_eq!(first_record_read, SizedRecord::new(record_size));
            assert_buffer_size!(ledger, MAX_FILE_ID, total_size);
            assert_reader_writer_v2_file_positions!(ledger, 0, MAX_FILE_ID - 1);

            acknowledge(first_record_read).await;

            // Our write should still not yet be ready because we won't have acknowledged the
            // read until we call `next` one more time, which will not only acknowledge the write,
            // driving a wake-up, but will queue the first data file to be deleted once it
            // recognizes the first data file is complete, and before loading the next data file, it
            // should also delete the first data file:
            assert_pending!(blocked_write.poll());

            let second_record_read = read_next_some(&mut reader).await;
            assert_eq!(second_record_read, SizedRecord::new(record_size));
            assert_buffer_records!(ledger, MAX_FILE_ID - 1);
            assert_reader_writer_v2_file_positions!(ledger, 1, MAX_FILE_ID - 1);

            // Now our writer should be woken up as we deleted the first data file when we went
            // through the second read, which triggers a writer wake-up.  We await the future
            // directly because the writer is going to go through a few blocking file operations as
            // it flushes the old file and opens the new one, and this means the very next poll
            // won't actually return immediately, so we just await instead of looping or anything:
            let bytes_written = blocked_write.await.expect("write should not fail");
            assert_enough_bytes_written!(bytes_written, SizedRecord, record_size);
            writer.flush().await.expect("flush should not fail");

            // Technically, we'll have 32 records in flight at this point, despite two reads,
            // because again, we haven't acknowledged the second read, so the record is still
            // considered to be outstanding.  We should, however, have moved on to our next data
            // file in the writer:
            assert_buffer_records!(ledger, MAX_FILE_ID);
            assert_reader_writer_v2_file_positions!(ledger, 1, 0);
        }
    });

    let parent = trace_span!("writer_stops_when_hitting_file_that_reader_is_still_on");
    fut.instrument(parent.or_current()).await;
}

#[tokio::test]
async fn writer_truncates_torn_current_file_instead_of_waiting_on_next_file() {
    // When the writer's current file has a torn tail on restart, the durable checkpoint is
    // authoritative. Startup should truncate the current file to the last valid checkpoint
    // boundary and keep writing there, rather than skipping to the next file and potentially
    // waiting on a file the reader has not deleted yet.
    //
    // TODO: Encode the "max data file size" in the ledger when creating a buffer for the first
    // time, so that we can refuse to open a buffer when the max data file size does not match.
    // This would provide the invariant that a data file, once full, can never become writable again
    // by reopening the buffer with a higher max data file size.
    let assertion_registry = install_tracing_helpers();
    let fut = with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let record_size = 42;
            let record = SizedRecord::new(record_size);
            let corrected_record_size = get_corrected_max_record_size(&record);
            let max_data_file_size = (corrected_record_size * 2)
                .try_into()
                .expect("Value should never exceed `u64::MAX`.");

            // Create our buffer with a low max data file size, which will let us quickly run through
            // the file ID range. We craft this number to allow for two records per data file.
            let (mut writer, _, ledger) =
                create_buffer_v2_with_max_data_file_size(data_dir.clone(), max_data_file_size)
                    .await;

            assert_buffer_is_empty!(ledger);
            assert_reader_writer_v2_file_positions!(ledger, 0, 0);

            // We want to write enough records so that our writer writes its last record on the last
            // file ID before file ID rollover occurs.
            let target_writer_file_id = MAX_FILE_ID - 1;

            let mut records_written = 0;
            let mut bytes_written = 0;
            let mut total_bytes_written = 0;
            let mut writer_file_id = 0;
            while writer_file_id != target_writer_file_id {
                for _ in 0..2 {
                    bytes_written = writer
                        .write_record(record.clone())
                        .await
                        .expect("write should not fail");
                    assert_enough_bytes_written!(bytes_written, SizedRecord, record_size);
                }
                writer.flush().await.expect("flush should not fail");

                total_bytes_written += bytes_written * 2;
                records_written += 2;
                writer_file_id = ledger.get_current_writer_file_id();

                assert_buffer_size!(ledger, records_written, total_bytes_written);
            }

            // Advance the time to ensure that we can trigger a full flush so that all writer bytes
            // are demonstrably on disk after doing so.
            tokio::time::pause();
            tokio::time::advance(DEFAULT_FLUSH_INTERVAL).await;

            writer.flush().await.expect("flush should not fail");
            writer.close();

            tokio::time::resume();

            let current_data_file_path = ledger.get_current_writer_data_file_path();
            let next_data_file_path = ledger.get_next_writer_data_file_path();
            let next_data_file_id = ledger.get_next_writer_file_id();
            drop(writer);
            drop(ledger);

            // Now, we need to load the data file we just left off on and modify it so that it
            // appears corrupted and triggers the writer to skip it during initialization, thus
            // pushing the writer to skip to next data file.  We do this by simply truncating it in
            // the middle of record, which _also_ has the effect that the data file is technically
            // not full anymore.
            //
            // Additionally, we'll remove one record from the _next_ data file, where the goal is
            // that we leave the data file in a valid state but smaller than the limit, so that we
            // can ensure that the writer doesn't mistakenly think it's fine to use simply because
            // the file is not yet full.
            set_data_file_length!(
                current_data_file_path,
                bytes_written * 2,
                (bytes_written * 2) - 4
            );
            set_data_file_length!(next_data_file_path, bytes_written * 2, bytes_written);

            // Now our last data file has been corrupted, and the next data file is below the
            // maximum data file size. Reopen and ensure the writer truncates the current file
            // instead of marking itself to skip into the occupied next file.
            let mark_to_skip_not_called = assertion_registry
                .build()
                .with_name("mark_for_skip")
                .with_parent_name(
                    "writer_truncates_torn_current_file_instead_of_waiting_on_next_file",
                )
                .was_not_entered()
                .finalize();
            let not_waiting_on_reader = assertion_registry
                .build()
                .with_name("wait_for_reader")
                .with_parent_name(
                    "writer_truncates_torn_current_file_instead_of_waiting_on_next_file",
                )
                .was_not_entered()
                .finalize();

            let (mut writer, _reader, ledger) =
                create_buffer_v2_with_max_data_file_size(data_dir, max_data_file_size).await;
            mark_to_skip_not_called.assert();
            not_waiting_on_reader.assert();
            assert_eq!(next_data_file_id, ledger.get_next_writer_file_id());
            assert_eq!(writer_file_id, ledger.get_current_writer_file_id());

            let _bytes_written = writer
                .write_record(record)
                .await
                .expect("write should not fail");
            writer.flush().await.expect("flush should not fail");
            assert_eq!(writer_file_id, ledger.get_current_writer_file_id());
            assert_eq!(next_data_file_id, ledger.get_next_writer_file_id());
            not_waiting_on_reader.assert();
        }
    });

    let parent = trace_span!("writer_truncates_torn_current_file_instead_of_waiting_on_next_file");
    fut.instrument(parent.or_current()).await;
}

#[tokio::test]
async fn writer_updates_ledger_when_buffered_writer_reports_implicit_flush() {
    // This test ensures that when the buffered writer tells us it had to preemptively flush, that
    // we correctly integrate its result by updating the ledger.  For example, if we do 10 writes
    // without a subsequent flush, and they're all buffered, but an 11th write forces us to flush
    // the buffer before we can actually buffer that 11th write, we should update the ledger to
    // indicate that those 10 initial writes are now "live": updated next record ID, updated buffer
    // size, and so on.
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let record_size: u32 = 16;

            // Create our buffer with a arbitrarily low write buffer size.  We'll use this to ensure
            // that the buffered writer has to implicitly flush after we've written a certain number
            // of records, but before we've manually flushed.
            let (mut writer, _, ledger) =
                create_buffer_v2_with_write_buffer_size(data_dir.clone(), 128).await;

            assert_buffer_is_empty!(ledger);
            assert_reader_writer_v2_file_positions!(ledger, 0, 0);

            // We write two records, which should fit within the write buffer without needing to
            // be implicitly flushed.
            let mut total_events_written = 0;
            let mut total_bytes_written = 0;

            for _ in 0..2 {
                let record = MultiEventRecord::new(record_size);
                let record_events = record.event_count();
                let bytes_written = writer
                    .write_record(record)
                    .await
                    .expect("write should not fail");

                total_events_written += record_events;
                total_bytes_written += bytes_written;
            }

            // Now, our buffer should still be "empty" because we don't acknowledge writes, until we
            // flush, either implicitly or explicitly, and that includes their size being
            // represented in the total buffer size, and the "next writer record ID":
            assert_buffer_is_empty!(ledger);

            // Do another write, which should overflow the write buffer and require those first
            // two writes to be implicitly flushed, which then requires us to update the ledger state:
            let record = MultiEventRecord::new(record_size);
            let record_events = record.event_count();
            let bytes_written = writer
                .write_record(record)
                .await
                .expect("write should not fail");
            assert_enough_bytes_written!(bytes_written, SizedRecord, record_size);

            // At this point, the entire write buffer should have been flushed, which means whatever
            // event counts/bytes written they generated should be represented in the ledger state:
            assert_buffer_size!(ledger, total_events_written, total_bytes_written);

            // Now, do an explicit flush, which should get us our third write:
            writer.flush().await.expect("flush should not fail");

            total_events_written += record_events;
            total_bytes_written += bytes_written;
            assert_buffer_size!(ledger, total_events_written, total_bytes_written);
        }
    })
    .await;
}

#[tokio::test]
async fn reader_writer_positions_aligned_through_multiple_files_and_records() {
    // This test ensures that the reader/writer position stay aligned through multiple records and
    // data files. This is to say, that, if we write 5 records, each with 10 events, and then read
    // and acknowledge all of those events... the writer's next record ID should be 51 (the 50th
    // event would correspond to ID 50, so next ID would be 51) and the reader's last read record ID
    // should be 50.
    //
    // Testing this across multiple data files isn't super germane to the position logic, but it
    // just ensures we're also testing that aspect.

    let _a = install_tracing_helpers();
    let fut = with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create our buffer with an arbitrarily low maximum data file size. We'll use this to
            // control how many records make it into a given data file. Just another way to ensure
            // we're testing the position logic with multiple writes to one data file, one write to
            // a data file, etc.
            let (mut writer, mut reader, ledger) =
                create_buffer_v2_with_max_data_file_size(data_dir, 256).await;

            // We'll write multi-event records with N events based on these sizes, and as we do so,
            // we'll assert that our writer position moves as expected after the write, and that
            // after reading and acknowledging, the reader position also moves as expected.
            let record_sizes = &[176, 52, 91, 137, 54, 87];

            let mut expected_writer_position = ledger.state().get_next_writer_record_id();
            let mut expected_reader_position = ledger.state().get_last_reader_record_id();
            let mut trailing_reader_position_delta = 0;

            for record_size in record_sizes {
                // Initial check before writing/reading the next record.
                assert_reader_last_writer_next_positions!(
                    ledger,
                    expected_reader_position,
                    expected_writer_position
                );

                let record = MultiEventRecord::new(*record_size);
                assert_eq!(
                    record.event_count(),
                    usize::try_from(*record_size).unwrap_or(usize::MAX)
                );

                writer
                    .write_record(record)
                    .await
                    .expect("write should not fail");
                writer.flush().await.expect("flush should not fail");

                expected_writer_position += u64::from(*record_size);

                // Make sure the writer position advanced after flushing.
                assert_reader_last_writer_next_positions!(
                    ledger,
                    expected_reader_position,
                    expected_writer_position
                );

                let record_via_read = read_next_some(&mut reader).await;
                assert_eq!(record_via_read, MultiEventRecord::new(*record_size));
                acknowledge(record_via_read).await;

                // Increment the expected reader position by the trailing reader position delta, and
                // then now that we've done a read, we should be able to have seen actually move
                // forward.
                expected_reader_position += trailing_reader_position_delta;
                assert_reader_last_writer_next_positions!(
                    ledger,
                    expected_reader_position,
                    expected_writer_position
                );

                // Set the trailing reader position delta to the record we just read.
                //
                // We do it this way because reads themselves have to drive acknowledgement logic to
                // then drive updates to the ledger, so we will only see the change in the reader's
                // position the _next_ time we do a read.
                trailing_reader_position_delta = u64::from(*record_size);
            }

            // Close the writer and do a final read, thus driving the acknowledgement logic, and
            // position update logic, before we do our final position check.
            writer.close();
            assert_eq!(reader.next().await, Ok(None));

            // Calculate the absolute reader/writer positions we would expect based on all of the
            // records/events written and read. This is to double check our work and make sure that
            // the "expected" positions didn't hide any bugs from us.
            let expected_final_reader_position =
                record_sizes.iter().copied().map(u64::from).sum::<u64>();
            let expected_final_writer_position = expected_final_reader_position + 1;

            assert_reader_last_writer_next_positions!(
                ledger,
                expected_final_reader_position,
                expected_final_writer_position
            );
        }
    });

    let parent = trace_span!("reader_writer_positions_aligned_through_multiple_files_and_records");
    fut.instrument(parent.or_current()).await;
}

/// Regression test for the buffer-size underflow on restart.
///
/// Historically, reopening a buffer seeded `total_buffer_size` from the sum of the on-disk data
/// file sizes and then had the reader "draw it down" record-by-record as it sought to its persisted
/// read position. Those two inputs were captured at different moments (the directory's file sizes
/// vs. the ledger's persisted position), so a crash between their respective flushes could make the
/// draw-down subtract more than the seed held -- wrapping the unsigned counter to a near-maximum
/// value, making the buffer look permanently full, and wedging the writer.
///
/// The reader now recomputes the unread total authoritatively during `seek_to_next_record` from the
/// checkpointed reader/writer file window. Boundary files are scanned at record granularity so
/// already-acknowledged prefixes and post-checkpoint tails are excluded from the reopened size. This
/// test exercises the case that stresses that boundary accounting -- a single data file that is
/// partially read and acknowledged, so the reader resumes mid-file with a non-empty already-read
/// prefix -- and asserts the reopened buffer reports exactly the bytes of the still-unread records.
#[tokio::test]
async fn buffer_size_recalculated_correctly_after_partial_read_reload() {
    let _a = install_tracing_helpers();
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Write several small records that all land in a single data file.
            let total_records = 8;
            let mut record_sizes = Vec::new();
            let (mut writer, mut reader, ledger) =
                create_default_buffer_v2::<_, SizedRecord>(data_dir.clone()).await;
            for _ in 0..total_records {
                let bytes_written = writer
                    .write_record(SizedRecord::new(64))
                    .await
                    .expect("write should not fail");
                record_sizes.push(bytes_written as u64);
            }
            writer.flush().await.expect("flush should not fail");

            let total_bytes: u64 = record_sizes.iter().sum();
            assert_buffer_size!(ledger, total_records, total_bytes);

            // Read and acknowledge a strict prefix of the records.
            let acked_records = 3usize;
            for _ in 0..acked_records {
                let record = read_next_some(&mut reader).await;
                acknowledge(record).await;
            }

            // Let the spawned finalizer deliver the acknowledgements, then drive one more read so
            // the reader consumes them, advancing its persisted record position past the prefix.
            tokio::task::yield_now().await;
            let _ = read_next(&mut reader).await;

            let expected_unread: u64 = record_sizes[acked_records..].iter().sum();

            // Checkpoint on the live buffer: the acknowledged prefix has been drawn down, so the
            // running size already reflects only the unread records. (This also guards the test
            // against the acknowledgements not having been processed before we reload.)
            assert_eq!(
                expected_unread,
                ledger.get_total_buffer_size(),
                "live buffer size should reflect only the unread records after acking the prefix",
            );

            // Persist the advanced read position so the reopened buffer resumes mid-file rather
            // than replaying from the start.
            ledger.flush().expect("ledger flush should not fail");

            drop(writer);
            drop(reader);
            drop(ledger);

            // Reopen. The authoritatively recomputed buffer size must equal exactly the bytes of
            // the unread records -- not the full file, and never an underflowed/wrapped value.
            let (_writer, _reader, ledger) =
                create_default_buffer_v2::<_, SizedRecord>(data_dir).await;
            assert_eq!(
                expected_unread,
                ledger.get_total_buffer_size(),
                "reopened buffer size should equal the unread records' on-disk bytes",
            );
        }
    })
    .await;
}
