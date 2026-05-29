use tokio::{fs::OpenOptions, io::AsyncWriteExt};
use tokio_test::{assert_pending, assert_ready, task::spawn};
use tracing::Instrument;

use super::{create_buffer_v2_with_max_data_file_size, read_next, read_next_some};
use crate::{
    EventCount, assert_buffer_is_empty, assert_buffer_records, assert_buffer_size,
    assert_enough_bytes_written, assert_reader_last_writer_next_positions,
    assert_reader_writer_v2_file_positions, await_timeout, set_data_file_length,
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
async fn reader_still_works_when_record_id_wraps_around() {
    let _a = install_tracing_helpers();
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a simple buffer.
            let (_, _, ledger) = create_default_buffer_v2::<_, SizedRecord>(data_dir.clone()).await;
            assert_buffer_is_empty!(ledger);
            assert_reader_writer_v2_file_positions!(ledger, 0, 0);

            // Adjust the record IDs manually so they comes right before the rollover event.
            //
            // We have to adjust both the writer and reader record ID markers.
            unsafe {
                ledger.state().unsafe_set_writer_next_record_id(u64::MAX);
            }
            unsafe {
                ledger
                    .state()
                    .unsafe_set_reader_last_record_id(u64::MAX - 1);
            }

            ledger.flush().expect("ledger should not fail to flush");
            assert_eq!(u64::MAX, ledger.state().get_next_writer_record_id());
            assert_eq!(u64::MAX - 1, ledger.state().get_last_reader_record_id());

            // We know that the reader will get angry when it goes to read a record, because at
            // startup it determined that the next record it reads should have a record ID of 1.
            //
            // The final step to get our ledger into the correct state is to simply reload the
            // buffer entirely, so the reader and writer initialize themselves with the ledger
            // stating that we're close to having written 2^64 records already.
            drop(ledger);

            let (mut writer, mut reader, ledger) = create_default_buffer_v2(data_dir).await;

            // Now we do two writes: one which uses u64::MAX, and another which will get the rolled
            // over value and go back to 0.
            let next_record_id = ledger.state().get_next_writer_record_id();
            let first_record_size = 14;
            let first_bytes_written = writer
                .write_record(SizedRecord::new(first_record_size))
                .await
                .expect("write should not fail");
            assert_enough_bytes_written!(first_bytes_written, SizedRecord, first_record_size);
            assert_eq!(next_record_id, ledger.state().get_next_writer_record_id());

            writer.flush().await.expect("flush should not fail");
            assert_buffer_records!(ledger, 1);
            assert_eq!(
                next_record_id.wrapping_add(1),
                ledger.state().get_next_writer_record_id()
            );

            let next_record_id = ledger.state().get_next_writer_record_id();
            let second_record_size = 256;
            let second_bytes_written = writer
                .write_record(SizedRecord::new(second_record_size))
                .await
                .expect("write should not fail");
            assert_enough_bytes_written!(second_bytes_written, SizedRecord, second_record_size);
            assert_eq!(next_record_id, ledger.state().get_next_writer_record_id());

            writer.flush().await.expect("flush should not fail");
            assert_buffer_records!(ledger, 2);
            assert_eq!(
                next_record_id.wrapping_add(1),
                ledger.state().get_next_writer_record_id()
            );

            writer.close();

            // Now we should be able to read both records without the reader getting angry.
            let first_record_read = read_next_some(&mut reader).await;
            assert_eq!(first_record_read, SizedRecord::new(first_record_size));
            assert_eq!(u64::MAX - 1, ledger.state().get_last_reader_record_id());
            assert_buffer_records!(ledger, 2);

            acknowledge(first_record_read).await;

            let second_record_read = read_next_some(&mut reader).await;
            assert_eq!(second_record_read, SizedRecord::new(second_record_size));
            assert_eq!(u64::MAX, ledger.state().get_last_reader_record_id());
            assert_buffer_records!(ledger, 1);

            acknowledge(second_record_read).await;

            let final_read = read_next(&mut reader).await;
            assert_eq!(final_read, None);
            assert_eq!(0, ledger.state().get_last_reader_record_id());
            assert_buffer_is_empty!(ledger);
        }
    })
    .await;
}

#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn reader_deletes_data_file_around_record_id_wraparound() {
    let assertion_registry = install_tracing_helpers();
    let fut = with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a simple buffer.
            let (_, _, ledger) = create_default_buffer_v2::<_, SizedRecord>(data_dir.clone()).await;

            assert_buffer_is_empty!(ledger);
            assert_reader_writer_v2_file_positions!(ledger, 0, 0);

            // Adjust the record IDs manually so they comes right before the rollover event.
            //
            // We have to adjust both the writer and reader record ID markers.
            unsafe {
                ledger.state().unsafe_set_writer_next_record_id(u64::MAX);
            }
            unsafe {
                ledger
                    .state()
                    .unsafe_set_reader_last_record_id(u64::MAX - 1);
            }

            ledger.flush().expect("ledger should not fail to flush");
            assert_eq!(u64::MAX, ledger.state().get_next_writer_record_id());
            assert_eq!(u64::MAX - 1, ledger.state().get_last_reader_record_id());

            // We know that the reader will get angry when it goes to read a record, because at
            // startup it determined that the next record it reads should have a record ID of 1.
            //
            // The final step to get our ledger into the correct state is to simply reload the
            // buffer entirely, so the reader and writer initialize themselves with the ledger
            // stating that we're close to having written 2^64 records already.
            drop(ledger);

            let (mut writer, mut reader, ledger) =
                create_buffer_v2_with_max_data_file_size(data_dir, 256).await;

            let starting_writer_file_id = ledger.get_current_writer_file_id();
            let next_writer_file_id = ledger.get_next_writer_file_id();

            // Now we do three writes, which will have u64::MAX, 0, and 1 as the IDs.  This ensures
            // that the first data file will have at least two records, and a range of IDs that cross
            // the wrapping threshold.
            let first_record_size = 64;
            let first_bytes_written = writer
                .write_record(SizedRecord::new(first_record_size))
                .await
                .expect("write should not fail");
            assert_enough_bytes_written!(first_bytes_written, SizedRecord, first_record_size);
            writer.flush().await.expect("flush should not fail");
            assert_eq!(0, ledger.state().get_next_writer_record_id());
            assert_buffer_records!(ledger, 1);
            assert_reader_writer_v2_file_positions!(ledger, 0, starting_writer_file_id);

            let second_record_size = 66;
            let second_bytes_written = writer
                .write_record(SizedRecord::new(second_record_size))
                .await
                .expect("write should not fail");
            assert_enough_bytes_written!(second_bytes_written, SizedRecord, second_record_size);
            writer.flush().await.expect("flush should not fail");
            assert_eq!(1, ledger.state().get_next_writer_record_id());
            assert_buffer_records!(ledger, 2);
            assert_reader_writer_v2_file_positions!(ledger, 0, starting_writer_file_id);

            // The third write is too big to fit into the first data file, so this will roll over to
            // a new data file, which ensures the reader has the go ahead to delete the first data
            // file after we read and ack the first two writes.
            let third_record_size = 68;
            let third_bytes_written = writer
                .write_record(SizedRecord::new(third_record_size))
                .await
                .expect("write should not fail");
            assert_enough_bytes_written!(third_bytes_written, SizedRecord, third_record_size);
            writer.flush().await.expect("flush should not fail");
            assert_eq!(2, ledger.state().get_next_writer_record_id());
            assert_buffer_records!(ledger, 3);
            assert_reader_writer_v2_file_positions!(ledger, 0, next_writer_file_id);

            // Our fourth write should fit just fine in the new data file, so no change there:
            let fourth_record_size = 70;
            let fourth_bytes_written = writer
                .write_record(SizedRecord::new(fourth_record_size))
                .await
                .expect("write should not fail");
            assert_enough_bytes_written!(fourth_bytes_written, SizedRecord, fourth_record_size);
            writer.flush().await.expect("flush should not fail");
            assert_eq!(3, ledger.state().get_next_writer_record_id());
            assert_buffer_records!(ledger, 4);
            assert_reader_writer_v2_file_positions!(ledger, 0, next_writer_file_id);

            writer.close();

            // Now that we have our two data files, we want to read all the records back,
            // acknowledge them, and assert that we deleted the first data file:
            let deleted_data_file = assertion_registry
                .build()
                .with_name("delete_completed_data_file")
                .with_parent_name("reader_deletes_data_file_around_record_id_wraparound")
                .was_entered()
                .finalize();

            let first_record_read = read_next_some(&mut reader).await;
            assert_eq!(first_record_read, SizedRecord::new(first_record_size));
            assert_eq!(u64::MAX - 1, ledger.state().get_last_reader_record_id());
            assert_buffer_records!(ledger, 4);
            assert_reader_writer_v2_file_positions!(ledger, 0, next_writer_file_id);

            acknowledge(first_record_read).await;
            assert!(!deleted_data_file.try_assert());

            let second_record_read = read_next_some(&mut reader).await;
            assert_eq!(second_record_read, SizedRecord::new(second_record_size));
            assert_eq!(u64::MAX, ledger.state().get_last_reader_record_id());
            assert_buffer_records!(ledger, 3);
            assert_reader_writer_v2_file_positions!(ledger, 0, next_writer_file_id);

            acknowledge(second_record_read).await;
            assert!(!deleted_data_file.try_assert());

            // This read should be where we actually delete the file since we've acknowledged all of
            // the reads from the first data file:
            let third_record_read = read_next_some(&mut reader).await;
            assert_eq!(third_record_read, SizedRecord::new(third_record_size));
            assert_eq!(0, ledger.state().get_last_reader_record_id());
            assert_buffer_records!(ledger, 2);
            assert_reader_writer_v2_file_positions!(ledger, 1, next_writer_file_id);

            acknowledge(third_record_read).await;
            assert!(deleted_data_file.try_assert());

            let fourth_record_read = read_next_some(&mut reader).await;
            assert_eq!(fourth_record_read, SizedRecord::new(fourth_record_size));
            assert_eq!(1, ledger.state().get_last_reader_record_id());
            assert_buffer_records!(ledger, 1);
            assert_reader_writer_v2_file_positions!(ledger, 1, next_writer_file_id);

            acknowledge(fourth_record_read).await;

            // And now since we closed the writer and read all four records, we should be done:
            let final_read = read_next(&mut reader).await;
            assert_eq!(final_read, None);
            assert_eq!(2, ledger.state().get_last_reader_record_id());
            assert_reader_writer_v2_file_positions!(ledger, 1, next_writer_file_id);
            assert_buffer_is_empty!(ledger);
        }
    });

    let parent = trace_span!("reader_deletes_data_file_around_record_id_wraparound");
    fut.instrument(parent.or_current()).await;
}

#[tokio::test]
async fn writer_waits_for_reader_after_validate_last_write_fails_and_data_file_skip_triggered() {
    // The title is long and probably hard to grok, so here's a more straightforward explanation:
    //
    // When we initialize a buffer, if the writer previously left off on a partially-filled data
    // file, we load that data file and do a simple check to make sure the last record in the file
    // is valid.  If it's not valid, we consider that data file corrupted and skip to the next data
    // file.  This is intended to limit us writing records to a data file that the reader is going
    // skip the rest of when it detects a bad/corrupted record.
    //
    // The problem is that we might be skipping to a data file that we previously finished writing
    // to, but has not yet been fully processed (and thus deleted) by the reader.
    //
    // Assume our maximum data file count is 10, and the writer is on #9, and the reader is on #0.
    // We open data file #9 as the writer, and detect that it's not valid, so we want to skip to the
    // next data file, which is #0.  When we go to open that data file, we do detect that it already
    // exists, so we examine the size of the file.  That file could actually be less than the
    // maximum data file size: maybe we also skipped that one previously due to corruption and it
    // wasn't yet full.
    //
    // Thus, we are _only_ willing to open and use a partially-filled data file when it's the file
    // we left off on according to the ledger.  If we have to skip to the next data file, so be it,
    // but if it already exists, regardless of size, we need to wait for the reader to clear it out.
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
            // maximum data file size, let's open the writer and make sure that it first skips the
            // current data file since it's corrupted.
            let mark_to_skip_called = assertion_registry
                .build()
                .with_name("mark_for_skip")
                .with_parent_name("writer_waits_for_reader_after_validate_last_write_fails_and_data_file_skip_triggered")
                .was_closed()
                .finalize();
            let waiting_on_reader = assertion_registry
                .build()
                .with_name("wait_for_reader")
                .with_parent_name("writer_waits_for_reader_after_validate_last_write_fails_and_data_file_skip_triggered")
                .was_entered()
                .finalize();

            let (mut writer, mut reader, ledger) =
                create_buffer_v2_with_max_data_file_size(data_dir, max_data_file_size).await;
            assert!(mark_to_skip_called.try_assert());
            assert_eq!(next_data_file_id, ledger.get_next_writer_file_id());
            assert!(!waiting_on_reader.try_assert());

            let total_records = ledger.get_total_records();

            // The writer correctly reset/marked itself as needing to skip the current data file,
            // but we need to actually attempt a write to drive the logic where it tries to open up
            // the next data file, so we do that here, expecting it to end up blocked on the reader.
            let mut blocked_write = spawn(writer.write_record(record));

            while !waiting_on_reader.try_assert() {
                assert_pending!(blocked_write.poll());
            }
            assert_eq!(next_data_file_id, ledger.get_next_writer_file_id());
            assert_eq!(total_records, ledger.get_total_records());

            // Now, let's actually read some records!  We'll read our way through the first data
            // file, which should yield a good read.  Remember, we removed a record from the "next"
            // data file, which is the data file the reader is currently on.  Thus, our second read
            // will move forward, which should allow deleting the first data file, aka "next", which
            // is what the writer is waiting on.
            let first_good_read = read_next_some(&mut reader).await;
            assert_eq!(first_good_read, SizedRecord::new(record_size));
            acknowledge(first_good_read).await;
            assert_pending!(blocked_write.poll());
            assert_reader_writer_v2_file_positions!(ledger, next_data_file_id, writer_file_id);

            let second_good_read = read_next_some(&mut reader).await;
            assert_eq!(second_good_read, SizedRecord::new(record_size));
            acknowledge(second_good_read).await;
            assert_reader_writer_v2_file_positions!(ledger, next_data_file_id + 1, writer_file_id);

            // Now the "next" data file should be acknowledged and deleted, and so the writer should
            // be unblocked.  We drive it as a normal future here because this is going to have to
            // do file I/O, which may yield a few times so a single poll isn't enough.  This should
            // open the next data file, the one the reader just deleted, and make it the current
            // data file for the writer.
            let blocked_write_result = await_timeout!(blocked_write, 2);
            let _bytes_written = blocked_write_result.expect("write should not fail");
            assert_eq!(next_data_file_id, ledger.get_current_writer_file_id());
        }
    });

    let parent = trace_span!(
        "writer_waits_for_reader_after_validate_last_write_fails_and_data_file_skip_triggered"
    );
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

#[tokio::test]
async fn ledger_total_buffer_size_decrement_should_saturate_not_underflow_issue_21683() {
    // Failing demonstration of Vector #21683.
    //
    // CORRECT INVARIANT (asserted here): decrementing `total_buffer_size` by more
    // than its current value must saturate at 0. `Ledger::decrement_total_buffer_size`
    // instead uses an unsaturated `fetch_sub`, so the in-memory atomic wraps
    // toward 2^64 — after which `is_buffer_full()` returns true forever and the
    // writer deadlocks permanently. PR #23561 only fixed the metrics reporter,
    // not this control-path atomic.
    //
    // This test FAILS against current Vector (the failure IS the bug): in release
    // the atomic wraps so the saturation assert fails; in a debug build the
    // `prev - amount` in decrement_total_buffer_size's own `trace!` panics on the
    // same underflow. It will pass once the decrement saturates.
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();
        async move {
            let (_writer, _reader, ledger) =
                create_default_buffer_v2::<_, SizedRecord>(data_dir).await;

            ledger.increment_total_buffer_size(10);
            assert_eq!(ledger.get_total_buffer_size(), 10);

            // Decrement by MORE than the current size. Correct behavior is to
            // saturate at 0.
            ledger.decrement_total_buffer_size(11);

            let after = ledger.get_total_buffer_size();
            assert_eq!(
                after, 0,
                "#21683: total_buffer_size decremented below zero must saturate at \
                 0, but was {after} (~2^64 wrap) — is_buffer_full() then never \
                 returns false -> permanent writer deadlock"
            );
        }
    })
    .await;
}

#[tokio::test]
async fn get_total_records_should_be_zero_on_drained_buffer_issue_21683_metrics() {
    // Failing demonstration (sibling of #21683): `Ledger::get_total_records`
    // computes `next_writer_id.wrapping_sub(last_reader_id) - 1`.
    //
    // CORRECT INVARIANT (asserted here): a fully drained buffer (writer and reader
    // at the same record ID) holds 0 records. The trailing `- 1` underflows
    // instead: in release it wraps to ~u64::MAX (which `synchronize_buffer_usage`
    // then feeds into the buffer event-count metric on the next restart, reporting
    // ~1.8e19 events for an empty buffer); in a debug build it panics on the same
    // subtraction. Either way this test FAILS until the count saturates.
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();
        async move {
            let (_w, _r, ledger) = create_default_buffer_v2::<_, SizedRecord>(data_dir).await;
            // Simulate a drained buffer: the reader has caught up to the writer.
            unsafe {
                ledger.state().unsafe_set_writer_next_record_id(42);
            }
            unsafe {
                ledger.state().unsafe_set_reader_last_record_id(42);
            }
            let total = ledger.get_total_records();
            assert_eq!(
                total, 0,
                "get_total_records 0-1 underflow: a drained buffer (next==last) \
                 must report 0 records, but yields {total} (~2^64); \
                 synchronize_buffer_usage then reports ~1.8e19 buffer events on restart"
            );
        }
    })
    .await;
}

#[tokio::test]
async fn writer_drop_without_flush_should_not_lose_buffered_events_issue_24948() {
    // Failing demonstration of Vector #24948 (config-reload silent data loss).
    //
    // CORRECT INVARIANT (asserted here): events accepted by the writer must
    // survive a writer teardown and be recoverable when the buffer is reopened.
    // Records sit in the writer's in-memory TrackingBufWriter (256KB) until
    // flush(); `BufferWriter::Drop` calls close() (mark_writer_done + notify) but
    // NOT flush(). During a config reload the old writer is dropped while events
    // are still buffered, so they never reach the data file and the ledger's
    // writer_next_record is never advanced. This test FAILS (reopened buffer has
    // 0 records, not 3) — that loss is the bug.
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();
        async move {
            {
                let (mut writer, _reader, _ledger) =
                    create_default_buffer_v2::<_, SizedRecord>(data_dir.clone()).await;
                for _ in 0..3u8 {
                    writer
                        .write_record(SizedRecord::new(64))
                        .await
                        .expect("write should not fail");
                }
                // No flush(): Drop simulates config-reload teardown (close, not flush).
                drop(writer);
            }
            // Let the finalizer task observe the dropped reader and release the lock.
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;

            // Reopen the same buffer directory.
            let (_writer2, _reader2, ledger2) =
                create_default_buffer_v2::<_, SizedRecord>(data_dir).await;

            // #24948: the 3 events written before the un-flushed Drop must
            // survive. They don't — the reopened buffer is empty (0 records), so
            // this assertion FAILS, demonstrating the silent data loss.
            assert_buffer_records!(ledger2, 3);
        }
    })
    .await;
}

#[tokio::test]
async fn file_id_rollover_compare_should_be_wrap_aware_reader_932() {
    // Failing demonstration: seek_to_next_record decides the reader is
    // synchronized with the writer using a raw `reader_file_id > writer_file_id`
    // (reader.rs ~932), which is NOT wrap-aware.
    //
    // CORRECT INVARIANT (asserted here): after a file-ID rollover where the writer
    // has done MORE rotations than the reader, the comparison must NOT report the
    // reader as having advanced past the writer. The writer wraps to a value
    // SMALLER than the (behind) reader's ID, so the raw `>` wrongly concludes the
    // reader is ahead and breaks the seek early. This test FAILS (raw compare says
    // reader > writer) until the comparison becomes wrap-aware.
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();
        async move {
            let (_w, _r, ledger) = create_default_buffer_v2::<_, SizedRecord>(data_dir).await;
            let n = u32::from(MAX_FILE_ID); // 6 in test builds; IDs cycle 0..n-1
            // Writer does n+1 rotations -> wraps past the end back to file ID 1.
            for _ in 0..(n + 1) {
                ledger.state().increment_writer_file_id();
            }
            // Reader does n-1 rotations (unacked) -> file ID n-1 (still pre-wrap).
            for _ in 0..(n - 1) {
                ledger.increment_unacked_reader_file_id();
            }
            let (reader_file_id, writer_file_id) = ledger.get_current_reader_writer_file_id();
            assert_eq!(
                (reader_file_id, writer_file_id),
                (MAX_FILE_ID - 1, 1),
                "constructed post-rollover state: writer lapped to ID 1, reader behind at ID {}",
                MAX_FILE_ID - 1
            );
            // The writer performed MORE rotations (n+1) than the reader (n-1), so it
            // is genuinely AHEAD. A wrap-aware comparison must therefore NOT report
            // the reader as past the writer:
            assert!(
                !(reader_file_id > writer_file_id),
                "reader.rs:932 must use a wrap-aware comparison: after rollover the \
                 writer lapped to ID {writer_file_id} and is AHEAD of the reader at \
                 ID {reader_file_id}, so the reader is NOT past the writer. The raw \
                 `reader_file_id > writer_file_id` ({reader_file_id} > {writer_file_id}) \
                 is true, so seek_to_next_record wrongly concludes the reader advanced \
                 past the writer and stops early."
            );
        }
    })
    .await;
}

#[tokio::test]
async fn delete_completed_data_file_size_delta_should_saturate_reader_524() {
    // Failing demonstration of the reader.rs:524 unguarded subtraction.
    //
    // `delete_completed_data_file` computes `size_delta = metadata.len() - bytes_read`
    // (reader.rs ~524) and then `decrement_total_buffer_size(size_delta)`.
    //
    // CORRECT INVARIANT (asserted here): when a data file is truncated externally
    // (crash / filesystem fault) so `bytes_read > metadata.len()`, the size-delta
    // must saturate at 0 rather than underflow. The raw subtraction underflows: in
    // release it wraps to ~2^64, which is fed straight into the unsaturated
    // `decrement_total_buffer_size` -> the #21683 total_buffer_size wrap ->
    // permanent writer deadlock; in a debug build the subtraction panics. Either
    // way this test FAILS, demonstrating the bug.
    //
    // Here we reproduce the exact computation on a real file: write 100 bytes,
    // record bytes_read=100, truncate the file to 40, then `metadata.len() - bytes_read`.
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();
        async move {
            let path = data_dir.join("buffer-data-0.dat");
            {
                let mut f = tokio::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .open(&path)
                    .await
                    .expect("create should not fail");
                f.write_all(&[0u8; 100])
                    .await
                    .expect("write should not fail");
                f.flush().await.expect("flush should not fail");
            }
            // The reader accounted 100 bytes read from this data file.
            let bytes_read: u64 = 100;
            // External truncation (crash / FS fault) shrinks the file below bytes_read.
            tokio::fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .await
                .expect("open should not fail")
                .set_len(40)
                .await
                .expect("truncate should not fail");

            // Exactly what reader.rs:524 does: `metadata.len() - bytes_read`.
            let metadata_len = tokio::fs::metadata(&path)
                .await
                .expect("metadata should not fail")
                .len();
            assert_eq!(metadata_len, 40);
            // In a debug build this raw subtraction panics (the bug manifesting);
            // in release it wraps to ~2^64.
            let size_delta = metadata_len - bytes_read;

            assert_eq!(
                size_delta, 0,
                "reader.rs:524: metadata.len()({metadata_len}) - bytes_read({bytes_read}) \
                 on a truncated file must saturate at 0, but underflowed to {size_delta} \
                 (~2^64); this is fed into the unsaturated decrement_total_buffer_size -> \
                 the #21683 wrap -> permanent writer deadlock"
            );
        }
    })
    .await;
}
