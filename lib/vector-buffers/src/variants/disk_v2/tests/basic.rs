use std::io::Cursor;

use futures::{stream, StreamExt};
use tokio_test::{assert_pending, assert_ready, task::spawn};
use tracing::Instrument;
use vector_common::finalization::Finalizable;

use super::{create_default_buffer_v2, read_next, read_next_some};
use crate::{
    assert_buffer_is_empty, assert_buffer_records,
    test::{acknowledge, install_tracing_helpers, with_temp_dir, MultiEventRecord, SizedRecord},
    variants::disk_v2::{tests::create_default_buffer_v2_with_usage, writer::RecordWriter},
    EventCount,
};

#[tokio::test]
async fn basic_read_write_loop() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a regular buffer, no customizations required.
            let (mut writer, mut reader, ledger) = create_default_buffer_v2(data_dir).await;
            assert_buffer_is_empty!(ledger);

            let expected_items = (512..768)
                .cycle()
                .take(10)
                .map(SizedRecord::new)
                .collect::<Vec<_>>();
            let input_items = expected_items.clone();

            // Now create a reader and writer task that will take a set of input messages, buffer
            // them, read them out, and then make sure nothing was missed.
            let write_task = tokio::spawn(async move {
                for item in input_items {
                    writer
                        .write_record(item)
                        .await
                        .expect("write should not fail");
                }
                writer.flush().await.expect("writer flush should not fail");
                writer.close();
            });

            let read_task = tokio::spawn(async move {
                let mut items = Vec::new();
                while let Some(mut record) = read_next(&mut reader).await {
                    acknowledge(record.take_finalizers()).await;
                    items.push(record);
                }
                items
            });

            // Wait for both tasks to complete.
            write_task.await.expect("write task should not panic");
            let actual_items = read_task.await.expect("read task should not panic");

            // All records should be consumed at this point.
            assert_buffer_is_empty!(ledger);

            // Make sure we got the right items.
            assert_eq!(actual_items, expected_items);
        }
    })
    .await;
}

#[tokio::test]
async fn reader_exits_cleanly_when_writer_done_and_in_flight_acks() {
    let assertion_registry = install_tracing_helpers();

    let fut = with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a regular buffer, no customizations required.
            let (mut writer, mut reader, ledger) = create_default_buffer_v2(data_dir).await;
            assert_buffer_is_empty!(ledger);

            // Now write a single value and close the writer.
            writer
                .write_record(SizedRecord::new(32))
                .await
                .expect("write should not fail");
            writer.flush().await.expect("writer flush should not fail");
            writer.close();
            assert_buffer_records!(ledger, 1);

            // And read that single value.
            let first_read = read_next_some(&mut reader).await;
            assert_eq!(first_read, SizedRecord::new(32));
            assert_buffer_records!(ledger, 1);

            // Now, we haven't acknowledged that read yet, so our next read should see the writer as
            // done but the total buffer size as >= 0, which means it has to wait for something,
            // which in this case is going to be the wakeup after we acknowledge the read.
            //
            // We have to poll until we hit the `wait_for_writer` call because there might be
            // wake-ups in between due to the actual file I/O calls we do in the `next` call waking
            // us up.
            //
            // Why do we need to make sure we enter `wait_for_writer` twice?  When the writer
            // closes, it always sends a wakeup in case there's a reader that's been (correctly)
            // waiting for new data.  When we do our first read here, there's data to be read, so
            // the reader never has to actually wait for the writer to make progress because it
            // doesn't yet think it's out of data.
            //
            // When we do the first poll of the second read, we will call `wait_for_writer` which
            // has a stored wakeup, which will let it proceed with another loop iteration, landing
            // it at the second call which is the one that causes it to actually block.
            let waiting_for_writer = assertion_registry
                .build()
                .with_name("wait_for_writer")
                .with_parent_name("reader_exits_cleanly_when_writer_done_and_in_flight_acks")
                .was_entered_at_least(2)
                .finalize();
            let mut blocked_read = spawn(reader.next());
            while !waiting_for_writer.try_assert() {
                assert_pending!(blocked_read.poll());
            }

            // Now acknowledge the first read, which should wake up our blocked read.
            acknowledge(first_read).await;

            // Our blocked read should be woken up, and when we poll it, it should be also be ready,
            // albeit with a return value of `None`... because the writer is closed, and we read all
            // the records, so nothing is left. :)
            assert!(blocked_read.is_woken());
            let second_read = assert_ready!(blocked_read.poll());
            assert_eq!(second_read.expect("read should not fail"), None);

            // All records should be consumed at this point.
            assert_buffer_is_empty!(ledger);
        }
    });

    let parent = trace_span!("reader_exits_cleanly_when_writer_done_and_in_flight_acks");
    fut.instrument(parent.or_current()).await;
}

#[tokio::test]
async fn initial_size_correct_with_multievents() {
    let _a = install_tracing_helpers();
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a regular buffer, no customizations required.
            let (mut writer, _, _) = create_default_buffer_v2(data_dir.clone()).await;

            let input_items = (512..768)
                .cycle()
                .take(2000)
                .map(MultiEventRecord::new)
                .collect::<Vec<_>>();
            let expected_records = input_items.len();
            let expected_events = input_items
                .iter()
                .map(EventCount::event_count)
                .sum::<usize>();

            // We also directly create a record writer so we can simulate actually
            // encoding/archiving the record to get the true on-disk size, as that's what we report
            // to the buffer usage handle but not anything we have access to from the outside.
            //
            // Technically, we aggregate the bytes written value from each write, but we also want
            // to verify that is accurate, so we record each record by hand to make sure our totals
            // are identical:
            let expected_bytes = stream::iter(input_items.iter().cloned())
                .filter_map(|record| async move {
                    let mut record_writer =
                        RecordWriter::new(Cursor::new(Vec::new()), 0, 16_384, u64::MAX, usize::MAX);
                    let (bytes_written, flush_result) = record_writer
                        .write_record(0, record)
                        .await
                        .expect("record writing should not fail");
                    record_writer.flush().await.expect("flush should not fail");
                    let inner_buf_len = record_writer.get_ref().get_ref().len();

                    // The bytes that it reports writing should be identical to what the underlying
                    // write buffer has, since this is a fresh record writer.
                    assert_eq!(bytes_written, inner_buf_len);
                    assert_eq!(flush_result, None);

                    Some(inner_buf_len)
                })
                .fold(0, |acc, n| async move { acc + n })
                .await;

            // Write a bunch of records so the buffer has events when we reload it.
            let mut total_bytes_written = 0;
            for item in input_items {
                let bytes_written = writer
                    .write_record(item)
                    .await
                    .expect("write should not fail");
                total_bytes_written += bytes_written;
            }
            writer.flush().await.expect("writer flush should not fail");
            writer.close();

            // Now drop our buffer and reopen it.
            drop(writer);
            let (writer, mut reader, ledger, usage) =
                create_default_buffer_v2_with_usage::<_, MultiEventRecord>(data_dir).await;
            drop(writer);

            // Make sure our usage data agrees with our expected event count and byte size:
            let snapshot = usage.snapshot();
            assert_eq!(expected_events as u64, snapshot.received_event_count);
            assert_eq!(expected_bytes as u64, snapshot.received_byte_size);
            assert_eq!(expected_events as u64, ledger.get_total_records());
            assert_eq!(expected_bytes, total_bytes_written);

            // Make sure we can read all of the records we wrote, and recalculate some of these
            // values from the source:
            let mut total_records_read = 0;
            let mut total_record_events = 0;
            while let Some(record) = read_next(&mut reader).await {
                total_records_read += 1;
                let event_count = record.event_count();
                acknowledge(record).await;
                total_record_events += event_count;
            }

            assert_eq!(expected_events, total_record_events);
            assert_eq!(expected_records, total_records_read);
        }
    })
    .await;
}
