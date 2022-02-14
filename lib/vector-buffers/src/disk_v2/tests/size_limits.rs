use std::time::Duration;

use tokio::time::timeout;
use tokio_test::{assert_pending, task::spawn};
use tracing::Instrument;

use super::{
    create_buffer_with_max_buffer_size, create_buffer_with_max_data_file_size,
    create_buffer_with_max_record_size, install_tracing_helpers, with_temp_dir, SizedRecord,
};
use crate::{
    assert_buffer_is_empty, assert_buffer_size, assert_enough_bytes_written,
    assert_reader_writer_file_positions,
};

#[tokio::test]
async fn writer_error_when_record_is_over_the_limit() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create our buffer with and arbitrarily low max buffer size, and two write sizes where
            // the first will fit but the second will not.
            //
            // The sizes are different so that we can assert that we got back the expected record at
            // each read we perform.
            let (mut writer, _reader, _acker, ledger) =
                create_buffer_with_max_record_size(data_dir, 100).await;
            let first_write_size = 95;
            let second_write_size = 97;

            assert_buffer_is_empty!(ledger);

            // First write should always complete because the size of the encoded record should be
            // right at 99 bytes, below our max record limit of 100 bytes.
            let first_record = SizedRecord(first_write_size);
            let first_bytes_written = writer
                .write_record(first_record)
                .await
                .expect("write should not fail");
            assert_enough_bytes_written!(first_bytes_written, SizedRecord, first_write_size);

            writer.flush().await.expect("flush should not fail");
            assert_buffer_size!(ledger, 1, first_bytes_written as u64);

            // This write should fail because it exceeds the 100 byte max record size limit.
            let second_record = SizedRecord(second_write_size);
            let _result = writer
                .write_record(second_record)
                .await
                .expect_err("write should fail");

            writer.flush().await.expect("flush should not fail");
            assert_buffer_size!(ledger, 1, first_bytes_written as u64);
        }
    })
    .await;
}

#[tokio::test]
async fn writer_waits_when_buffer_is_full() {
    let assertion_registry = install_tracing_helpers();
    let fut = with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create our buffer with and arbitrarily low max buffer size, and two write sizes that
            // will both fit just under the limit but will provide no chance for another write to
            // fit.
            //
            // The sizes are different so that we can assert that we got back the expected record at
            // each read we perform.
            let (mut writer, mut reader, acker, ledger) =
                create_buffer_with_max_buffer_size(data_dir, 100).await;
            let first_write_size = 92;
            let second_write_size = 96;

            assert_buffer_is_empty!(ledger);

            // First write should always complete because we haven't written anything yet, so we
            // haven't exceed our total buffer size limit yet, or the size limit of the data file
            // itself.  We do need this write to be big enough to exceed the total buffer size
            // limit, though.
            let first_record = SizedRecord(first_write_size);
            let first_bytes_written = writer
                .write_record(first_record)
                .await
                .expect("write should not fail");
            assert_enough_bytes_written!(first_bytes_written, SizedRecord, first_write_size);

            writer.flush().await.expect("flush should not fail");
            assert_buffer_size!(ledger, 1, first_bytes_written);

            // This write should block because will have exceeded our 100 byte total buffer size
            // limit handily with the first write we did.
            let mut second_record_write = spawn(async {
                let record = SizedRecord(second_write_size);
                writer
                    .write_record(record)
                    .await
                    .expect("write should not fail")
            });

            let called_wait_for_reader = assertion_registry
                .build()
                .with_name("wait_for_reader")
                .with_parent_name("writer_waits_when_buffer_is_full")
                .was_entered()
                .finalize();
            let got_past_wait_for_reader = assertion_registry
                .build()
                .with_name("wait_for_reader")
                .with_parent_name("writer_waits_when_buffer_is_full")
                .was_closed()
                .finalize();
            assert!(!called_wait_for_reader.try_assert());
            assert!(!got_past_wait_for_reader.try_assert());

            assert_pending!(second_record_write.poll());
            called_wait_for_reader.assert();
            assert!(!got_past_wait_for_reader.try_assert());

            // Now do a read, which would theoretically make enough space available, but wait! We
            // actually have to acknowledge the read, too, to update the buffer size.  This read
            // will complete but the second write should still be blocked/not woken up:
            let first_record_read = reader.next().await.expect("read should not fail");
            assert_eq!(first_record_read, Some(SizedRecord(first_write_size)));

            // We haven't yet acknowledged the record, so nothing has changed yet:
            assert_pending!(second_record_write.poll());
            assert!(!got_past_wait_for_reader.try_assert());
            assert_buffer_size!(ledger, 1, first_bytes_written);

            // Trigger our second read, which is necessary to actually run the acknowledgement logic
            // that consumes pending acks, potentially deletes data files, etc.  We trigger it
            // before so that we can also validate that when a read is blocking on more data,
            // acknowledging a record will wake it up so it can run the logic.
            let deleted_first_data_file = assertion_registry
                .build()
                .with_name("delete_completed_data_files")
                .with_parent_name("writer_waits_when_buffer_is_full")
                .was_closed()
                .finalize();

            let got_past_wait_for_waiter = assertion_registry
                .build()
                .with_name("wait_for_writer")
                .with_parent_name("writer_waits_when_buffer_is_full")
                .was_closed()
                .finalize();

            let mut second_record_read =
                spawn(async { reader.next().await.expect("read should not fail") });

            assert!(!deleted_first_data_file.try_assert());
            assert!(!got_past_wait_for_waiter.try_assert());
            assert_pending!(second_record_read.poll());

            // Now acknowledge the first record we read.  This will wake up our second read, so it
            // can at least handle the pending acknowledgements logic, but it won't actually be ready,
            // because the second write hasn't completed yet:
            acker.ack(1);
            while !(deleted_first_data_file.try_assert() && got_past_wait_for_waiter.try_assert()) {
                assert_pending!(second_record_read.poll());
            }

            // And now the writer should be woken up since the acknowledgement was processed, and
            // the blocked write should be able to complete:
            assert_buffer_is_empty!(ledger);

            let second_bytes_written = second_record_write.await;
            assert_enough_bytes_written!(second_bytes_written, SizedRecord, second_write_size);

            writer.flush().await.expect("flush should not fail");

            // Close the writer which closes everything so that our final read indicates that we've
            // reached the end, which is what we want and expect.
            writer.close();

            assert_buffer_size!(ledger, 1, second_bytes_written);

            // And now our second read, after having been woken up to drive the pending
            // acknowledgement, should now be woken up again and be able to read the second write,
            // but again, we haven't acknowledged it yet, so the ledger is not yet updated:
            let second_record_read_result = second_record_read.await;
            assert_eq!(
                second_record_read_result,
                Some(SizedRecord(second_write_size))
            );
            assert_buffer_size!(ledger, 1, second_bytes_written);

            // Now acknowledge the record, and do our final read:
            acker.ack(1);

            let final_record_read = reader.next().await.expect("read should not fail");
            assert_eq!(final_record_read, None);
            assert_buffer_is_empty!(ledger);
        }
    });

    let parent = trace_span!("writer_waits_when_buffer_is_full");
    fut.instrument(parent).await;
}

#[tokio::test]
async fn writer_rolls_data_files_when_the_limit_is_exceeded() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create our buffer with and arbitrarily low max buffer size, and two write sizes that
            // will both fit just under the limit but will provide no chance for another write to
            // fit.  This will trigger data file rollover when we attempt the second write.
            //
            // The sizes are different so that we can assert that we got back the expected record at
            // each read we perform.
            let (mut writer, mut reader, acker, ledger) =
                create_buffer_with_max_data_file_size(data_dir, 100).await;
            let first_write_size = 92;
            let second_write_size = 96;

            assert_buffer_is_empty!(ledger);
            assert_reader_writer_file_positions!(ledger, 0, 0);

            // First write should always complete because we haven't written anything yet, so we
            // haven't exceed our total buffer size limit yet, or the size limit of the data file
            // itself.  We do need this write to be big enough to exceed the max data file limit,
            // though.
            let first_record = SizedRecord(first_write_size);
            let first_bytes_written = writer
                .write_record(first_record)
                .await
                .expect("write should not fail");
            assert_enough_bytes_written!(first_bytes_written, SizedRecord, first_write_size);

            writer.flush().await.expect("flush should not fail");
            assert_buffer_size!(ledger, 1, first_bytes_written);
            assert_reader_writer_file_positions!(ledger, 0, 0);

            // Second write should also always complete, but at this point, we should have rolled
            // over to the next data file.
            let second_record = SizedRecord(second_write_size);
            let second_bytes_written = writer
                .write_record(second_record)
                .await
                .expect("write should not fail");
            assert_enough_bytes_written!(second_bytes_written, SizedRecord, second_write_size);

            writer.flush().await.expect("flush should not fail");
            writer.close();

            assert_buffer_size!(ledger, 2, (first_bytes_written + second_bytes_written));
            assert_reader_writer_file_positions!(ledger, 0, 1);

            // Now read both records, make sure they are what we expect, etc.
            let first_record_read = reader.next().await.expect("read should not fail");
            assert_eq!(first_record_read, Some(SizedRecord(first_write_size)));
            acker.ack(1);

            assert_buffer_size!(ledger, 2, (first_bytes_written + second_bytes_written));
            assert_reader_writer_file_positions!(ledger, 0, 1);

            let second_record_read = reader.next().await.expect("read should not fail");
            assert_eq!(second_record_read, Some(SizedRecord(second_write_size)));
            acker.ack(1);

            assert_buffer_size!(ledger, 1, second_bytes_written);
            assert_reader_writer_file_positions!(ledger, 1, 1);

            let final_empty_read = reader.next().await.expect("read should not fail");
            assert_eq!(final_empty_read, None);

            assert_buffer_is_empty!(ledger);
            assert_reader_writer_file_positions!(ledger, 1, 1);
        }
    })
    .await;
}

#[tokio::test]
async fn writer_rolls_data_files_when_the_limit_is_exceeded_after_reload() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create our buffer with and arbitrarily low max buffer size, and two write sizes that
            // will both fit just under the limit but will provide no chance for another write to
            // fit.  This will trigger data file rollover when we attempt the second write.
            //
            // The sizes are different so that we can assert that we got back the expected record at
            // each read we perform.
            let (mut writer, _, _, ledger) =
                create_buffer_with_max_data_file_size(data_dir.clone(), 100).await;
            let first_write_size = 92;
            let second_write_size = 96;

            assert_buffer_is_empty!(ledger);
            assert_reader_writer_file_positions!(ledger, 0, 0);

            // First write should always complete because we haven't written anything yet, so we
            // haven't exceed our total buffer size limit yet, or the size limit of the data file
            // itself.  We do need this write to be big enough to exceed the max data file limit,
            // though.
            let first_record = SizedRecord(first_write_size);
            let first_bytes_written = writer
                .write_record(first_record)
                .await
                .expect("write should not fail");
            assert_enough_bytes_written!(first_bytes_written, SizedRecord, first_write_size);

            writer.flush().await.expect("flush should not fail");
            assert_buffer_size!(ledger, 1, first_bytes_written);
            assert_reader_writer_file_positions!(ledger, 0, 0);

            // Now drop the original reader/writer and reload it.  We want to make sure that when
            // the current writer data file is at or over the limit, the writer can correctly
            // determine whether or not it should simply move to the next file ID or if it actually
            // needs to wait for the reader.
            drop(writer);
            drop(ledger);

            let open_wait = Duration::from_secs(5);
            let second_buffer_open = create_buffer_with_max_data_file_size(data_dir, 100);
            let (mut writer, mut reader, acker, ledger) = timeout(open_wait, second_buffer_open)
                .await
                .expect("failed to open buffer a second time in the expected timeframe");
            assert_buffer_size!(ledger, 1, first_bytes_written);
            assert_reader_writer_file_positions!(ledger, 0, 0);

            // Second write should also always complete, but at this point, we should have rolled
            // over to the next data file.
            let second_record = SizedRecord(second_write_size);
            let second_bytes_written = writer
                .write_record(second_record)
                .await
                .expect("write should not fail");
            assert_enough_bytes_written!(second_bytes_written, SizedRecord, second_write_size);

            writer.flush().await.expect("flush should not fail");
            writer.close();

            assert_buffer_size!(ledger, 2, (first_bytes_written + second_bytes_written));
            assert_reader_writer_file_positions!(ledger, 0, 1);

            // Now read both records, make sure they are what we expect, etc.
            let first_record_read = reader.next().await.expect("read should not fail");
            assert_eq!(first_record_read, Some(SizedRecord(first_write_size)));
            acker.ack(1);

            assert_buffer_size!(ledger, 2, (first_bytes_written + second_bytes_written));
            assert_reader_writer_file_positions!(ledger, 0, 1);

            let second_record_read = reader.next().await.expect("read should not fail");
            assert_eq!(second_record_read, Some(SizedRecord(second_write_size)));
            acker.ack(1);

            assert_buffer_size!(ledger, 1, second_bytes_written);
            assert_reader_writer_file_positions!(ledger, 1, 1);

            let final_empty_read = reader.next().await.expect("read should not fail");
            assert_eq!(final_empty_read, None);

            assert_buffer_is_empty!(ledger);
            assert_reader_writer_file_positions!(ledger, 1, 1);
        }
    })
    .await;
}
