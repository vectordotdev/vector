use tokio_test::{assert_pending, task::spawn};
use tracing_fluent_assertions::{Controller, FluentAssertionsLayer};
use tracing_subscriber::{layer::SubscriberExt, Registry};

use crate::disk_v2::{common::MAX_FILE_ID, tests::create_default_buffer};

use super::{create_buffer_with_max_data_file_size, with_temp_dir, SizedRecord};

#[tokio::test]
async fn file_id_wraps_around_when_max_file_id_hit() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let record_size: u32 = 100;

            // Create our buffer with an arbitrarily low max data file size, which will let us
            // quickly run through the file ID range.
            let (mut writer, mut reader, _) =
                create_buffer_with_max_data_file_size(data_dir, record_size as u64).await;

            assert_eq!(reader.get_total_records(), 0);
            assert_eq!(reader.get_total_buffer_size(), 0);
            assert_eq!(reader.get_current_reader_file_id(), 0);
            assert_eq!(writer.get_total_records(), 0);
            assert_eq!(writer.get_total_buffer_size(), 0);
            assert_eq!(writer.get_current_writer_file_id(), 0);

            // We execute a loop of writing and then reading back a record, and we assert each time
            // that the file IDs are where we expect them to be.  We write 3x the number of records
            // as the max possible file ID, to ensure that rollover works.
            let file_id_upper = u32::from(MAX_FILE_ID);

            // Random addition at the end so we don't land explicitly on the u16 boundary.
            let target_id = (file_id_upper * 3) + 15;

            let mut id = 0;
            while id < target_id {
                let record = SizedRecord(record_size);
                let write_result = writer.write_record(record.clone()).await;
                let bytes_written = write_result.expect("should not be error");
                // `SizedRecord` has a 4-byte header for the payload length.
                assert!(bytes_written > record_size as usize + 4);

                writer.flush().await.expect("writer flush should not fail");

                let record_read = reader.next().await.expect("read should not fail");
                assert_eq!(record_read, Some(record));

                let expected_reader_file_id = id % file_id_upper;
                let expected_writer_file_id = id % file_id_upper;

                assert_eq!(reader.get_total_records(), 0);
                assert_eq!(reader.get_total_buffer_size(), 0);
                assert_eq!(
                    reader.get_current_reader_file_id() as u32,
                    expected_reader_file_id
                );
                assert_eq!(writer.get_total_records(), 0);
                assert_eq!(writer.get_total_buffer_size(), 0);
                assert_eq!(
                    writer.get_current_writer_file_id() as u32,
                    expected_writer_file_id
                );

                id += 1;
            }
        }
    })
    .await
}

#[tokio::test]
async fn writer_stops_when_hitting_file_that_reader_is_still_on() {
    // TODO: This works, but, it's a bit heavy at the moment.  We should try and clean this up at
    // least into a helper method, and see if we can figure out some way to make it more
    // stable/resilient to other tests running if _they_ wanted to also take part.  Obviously, the
    // thread-local nature of the tracing dispatcher makes this tricky, but maybe there's a way!
    //
    // One idea could be some sort of tracing field-based approach where we associate the controller
    // with a specific root span so that it only captures spans which originate from that span
    // specifically, based on... a field value? Maybe a sentinel value we store in the span's data?
    //
    // None of that addresses how to create and set the subscriber to ensure we have our fluent
    // assertion layer present, or how to pass in the controller instance.... but baby steps!
    let controller = Controller::new();
    let registry = Registry::default();
    let subscriber = registry.with(FluentAssertionsLayer::new(&controller));
    tracing::subscriber::set_global_default(subscriber).unwrap();

    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let record_size: u32 = 100;

            // Create our buffer with an arbitrarily low max data file size, which will let us
            // quickly run through the file ID range.
            let (mut writer, mut reader, _) =
                create_buffer_with_max_data_file_size(data_dir, record_size as u64).await;

            assert_eq!(reader.get_total_records(), 0);
            assert_eq!(reader.get_total_buffer_size(), 0);
            assert_eq!(reader.get_current_reader_file_id(), 0);
            assert_eq!(writer.get_total_records(), 0);
            assert_eq!(writer.get_total_buffer_size(), 0);
            assert_eq!(writer.get_current_writer_file_id(), 0);

            // We execute a loop of writing enough records to consume all data files, without doing
            // any reading.
            let file_id_upper = u32::from(MAX_FILE_ID);

            let mut id = 0;
            while id < file_id_upper {
                let record = SizedRecord(record_size);
                let write_result = writer.write_record(record).await;
                let bytes_written = write_result.expect("should not be error");
                // `SizedRecord` has a 4-byte header for the payload length.
                assert!(bytes_written > record_size as usize + 4);

                writer.flush().await.expect("writer flush should not fail");

                id += 1;
            }

            assert_eq!(reader.get_total_records(), 32);
            assert_eq!(reader.get_current_reader_file_id(), 0);
            assert_eq!(writer.get_total_records(), 32);
            assert_eq!(writer.get_current_writer_file_id(), 31);

            //reload_handle.reload(EnvFilter::new("trace")).expect("failed to reload envfilter");
            let assertion = controller.add_entry("wait_for_reader");

            // Now we should be consuming all data files, and our next write should block trying to
            // open the "first" data file until we do a read.
            let mut blocked_write = spawn(async {
                let record = SizedRecord(record_size);
                writer.write_record(record).await
            });

            while !assertion.was_entered() {
                assert_pending!(blocked_write.poll());
            }
            assert_pending!(blocked_write.poll());
            assert!(!blocked_write.is_woken());

            // Now execute a read which will pull the first record.  This doesn't yet delete the
            // first data file since we haven't discovered that we hit the end of it so we have to
            // do one more read to get there.
            let first_record_read = reader.next().await.expect("read should not fail");
            assert_eq!(first_record_read, Some(SizedRecord(record_size)));

            assert_eq!(reader.get_total_records(), 31);
            assert_eq!(reader.get_current_reader_file_id(), 0);

            // Our reader has read a record, which does trigger a wake-up, but all that has changed is
            // the total buffer size as one record has been completed... so our blocked write _will_
            // have been woken up, but it is still not able to open the data file that it needs to.
            // For that to happen, we have to do another read so the current reader data file is closed.
            assert!(blocked_write.is_woken());
            assert_pending!(blocked_write.poll());

            let second_record_read = reader.next().await.expect("read should not fail");
            assert_eq!(second_record_read, Some(SizedRecord(record_size)));

            assert_eq!(reader.get_total_records(), 30);
            assert_eq!(reader.get_current_reader_file_id(), 1);

            // Now our writer should be woken up as we deleted the first data file when we went
            // through the second read, which triggers a writer wake-up.  We await the future
            // directly because the writer is going to go through a few blocking file operations as
            // it flushes the old file and opens the new one, and this means the very next poll
            // won't actually return immediately, so we just await instead of looping or anything.
            assert!(blocked_write.is_woken());

            let bytes_written = blocked_write.await.expect("write should not fail");
            // `SizedRecord` has a 4-byte header for the payload length.
            assert!(bytes_written > record_size as usize + 4);

            assert_eq!(reader.get_total_records(), 31);
            assert_eq!(reader.get_current_reader_file_id(), 1);
            assert_eq!(writer.get_total_records(), 31);
            assert_eq!(writer.get_current_writer_file_id(), 0);
        }
    })
    .await
}

#[tokio::test]
async fn reader_still_works_when_record_id_wraps_around() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a simple buffer.
            let (_, _, ledger) = create_default_buffer::<_, SizedRecord>(data_dir.clone()).await;

            assert_eq!(ledger.state().get_total_records(), 0);
            assert_eq!(ledger.state().get_total_buffer_size(), 0);
            assert_eq!(ledger.state().get_current_reader_file_id(), 0);
            assert_eq!(ledger.state().get_current_writer_file_id(), 0);

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

            let (mut writer, mut reader, ledger) = create_default_buffer(data_dir).await;

            // Now we do two writes: one which uses u64::MAX, and another which will get the rolled
            // over value and go back to 0.
            let first_record_size = 14;
            let bytes_written = writer
                .write_record(SizedRecord(first_record_size))
                .await
                .expect("should not be error");
            // `SizedRecord` has a 4-byte header for the payload length.
            assert!(bytes_written > first_record_size as usize + 4);
            assert_eq!(0, ledger.state().get_next_writer_record_id());

            writer.flush().await.expect("writer flush should not fail");

            let second_record_size = 256;
            let bytes_written = writer
                .write_record(SizedRecord(second_record_size))
                .await
                .expect("should not be error");
            // `SizedRecord` has a 4-byte header for the payload length.
            assert!(bytes_written > second_record_size as usize + 4);
            assert_eq!(1, ledger.state().get_next_writer_record_id());

            writer.flush().await.expect("writer flush should not fail");

            assert_eq!(ledger.state().get_total_records(), 2);

            // Now we should be able to read both records without the reader getting angry.
            let first_record_read = reader.next().await.expect("read should not fail");
            assert_eq!(first_record_read, Some(SizedRecord(first_record_size)));
            assert_eq!(u64::MAX, ledger.state().get_last_reader_record_id());
            assert_eq!(ledger.state().get_total_records(), 1);

            let second_record_read = reader.next().await.expect("read should not fail");
            assert_eq!(second_record_read, Some(SizedRecord(second_record_size)));
            assert_eq!(0, ledger.state().get_last_reader_record_id());
            assert_eq!(ledger.state().get_total_records(), 0);
        }
    })
    .await
}
