use std::{io::Cursor, sync::Arc, time::Duration};

use futures::{StreamExt, stream};
use tokio::time::timeout;
use tokio_test::{assert_pending, task::spawn};
use vector_common::finalization::Finalizable;

use super::{
    create_default_buffer_v2, create_default_buffer_v2_with_usage, read_next, read_next_some,
};
use crate::{
    EventCount, assert_buffer_is_empty,
    buffer_usage_data::BufferUsageHandle,
    test::{MultiEventRecord, SizedRecord, acknowledge, install_tracing_helpers, with_temp_dir},
    variants::disk_v2::{BufferWriter, DiskBufferConfigBuilder, Ledger, writer::RecordWriter},
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
                reader.finish().await.unwrap();
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
async fn reader_eof_precedes_acknowledgement_completion() {
    let dir = tempfile::tempdir().unwrap();
    let (mut writer, mut reader, ledger) = create_default_buffer_v2(dir.path()).await;
    writer.write_record(SizedRecord::new(32)).await.unwrap();
    writer.flush().await.unwrap();
    writer.close();

    let record = read_next_some(&mut reader).await;
    let checkpoint = ledger.state().get_last_reader_record_id();
    let outstanding_bytes = ledger.get_total_buffer_size();
    assert!(outstanding_bytes > 0);
    assert!(
        timeout(Duration::from_secs(1), read_next(&mut reader))
            .await
            .unwrap()
            .is_none()
    );
    assert!(read_next(&mut reader).await.is_none());
    assert_eq!(ledger.state().get_last_reader_record_id(), checkpoint);
    assert_eq!(ledger.get_total_buffer_size(), outstanding_bytes);

    let mut finish = spawn(reader.finish());
    assert_pending!(finish.poll());
    acknowledge(record).await;
    timeout(Duration::from_secs(1), finish)
        .await
        .unwrap()
        .unwrap();
    assert_buffer_is_empty!(ledger);
    assert_eq!(ledger.state().get_last_reader_record_id(), checkpoint + 1);
    reader.finish().await.unwrap();
}

#[tokio::test]
async fn reader_finish_requires_runtime_eof() {
    let dir = tempfile::tempdir().unwrap();
    let (mut writer, mut reader, _) = create_default_buffer_v2::<_, SizedRecord>(dir.path()).await;
    assert_eq!(
        reader.finish().await.unwrap_err().kind(),
        std::io::ErrorKind::InvalidInput
    );
    writer.close();
    assert!(read_next(&mut reader).await.is_none());
    reader.finish().await.unwrap();
}

#[tokio::test]
async fn reader_completion_controls_replay_after_eof() {
    for complete in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (mut writer, mut reader, ledger) =
            super::create_buffer_v2_with_max_data_file_size(dir.path(), 256).await;
        let mut records = Vec::new();
        for size in [128, 129, 130] {
            writer.write_record(SizedRecord::new(size)).await.unwrap();
            writer.flush().await.unwrap();
            records.push(read_next_some(&mut reader).await);
        }
        writer.close();
        assert!(read_next(&mut reader).await.is_none());
        if complete {
            for record in records {
                acknowledge(record).await;
            }
            reader.finish().await.unwrap();
            drop(reader);
        } else {
            // Cancel completion while acknowledgements are still held downstream.
            let mut finish = spawn(reader.finish());
            assert_pending!(finish.poll());
            drop(finish);
            drop(reader);
            drop(records);
        }
        drop(writer);
        // The finalizer task owns the ledger until its queued notifications have drained.
        timeout(Duration::from_secs(1), async {
            while Arc::strong_count(&ledger) != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        drop(ledger);

        let (mut writer, mut reader, _) =
            create_default_buffer_v2::<_, SizedRecord>(dir.path()).await;
        writer.close();
        let mut replayed = Vec::new();
        while let Some(mut record) = read_next(&mut reader).await {
            acknowledge(record.take_finalizers()).await;
            replayed.push(record);
        }
        reader.finish().await.unwrap();
        let expected = if complete {
            vec![]
        } else {
            vec![
                SizedRecord::new(128),
                SizedRecord::new(129),
                SizedRecord::new(130),
            ]
        };
        assert_eq!(replayed, expected);
    }
}

#[tokio::test]
async fn reader_observes_final_write_and_rollover_before_eof() {
    for roll in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (mut writer, mut reader, ledger) =
            super::create_buffer_v2_with_max_data_file_size(dir.path(), 256).await;
        writer.write_record(SizedRecord::new(128)).await.unwrap();
        writer.flush().await.unwrap();
        let first = read_next_some(&mut reader).await;
        let file_id = ledger.get_current_writer_file_id();

        let mut pending_read = spawn(read_next_some(&mut reader));
        assert_pending!(pending_read.poll());
        let tail_size = if roll { 128 } else { 1 };
        writer
            .write_record(SizedRecord::new(tail_size))
            .await
            .unwrap();
        writer.flush().await.unwrap();
        writer.close();
        assert_eq!(ledger.get_current_writer_file_id() != file_id, roll);

        let tail = timeout(Duration::from_secs(1), pending_read).await.unwrap();
        assert_eq!(tail, SizedRecord::new(tail_size));
        assert!(
            timeout(Duration::from_secs(1), read_next(&mut reader))
                .await
                .unwrap()
                .is_none()
        );
        acknowledge(first).await;
        acknowledge(tail).await;
        reader.finish().await.unwrap();
        assert_buffer_is_empty!(ledger);
    }
}

#[tokio::test]
async fn initial_size_correct_with_multievents() {
    let _a = install_tracing_helpers();
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Build a write-only buffer without a finalizer task. Using
            // `from_config_inner` would spawn a background finalizer that holds
            // an Arc<Ledger> (and the lock file), causing a racy
            // LedgerLockAlreadyHeld when we reopen the buffer below.
            let config = DiskBufferConfigBuilder::from_path(&data_dir)
                .build()
                .expect("creating buffer config should not fail");
            let usage_handle = BufferUsageHandle::noop();
            let ledger = Ledger::load_or_create(config, usage_handle)
                .await
                .expect("ledger should not fail to load/create");
            let ledger = Arc::new(ledger);

            let mut writer = BufferWriter::new(Arc::clone(&ledger));
            writer
                .validate_last_write()
                .await
                .expect("validate_last_write should not fail");

            ledger.synchronize_buffer_usage();

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

            // Drop the first buffer. No background finalizer task exists, so the
            // ledger lock is released immediately when the last Arc is dropped.
            drop(writer);
            drop(ledger);

            // Reopen the buffer with a full reader to verify the persisted data.
            let (mut writer, mut reader, ledger, usage) =
                create_default_buffer_v2_with_usage::<_, MultiEventRecord>(&data_dir).await;
            let disk_usage = writer.usage_snapshot();
            writer.close();

            // Make sure our usage data agrees with our expected event count and byte size:
            let snapshot = usage.snapshot();
            assert_eq!(expected_events as u64, snapshot.received_event_count);
            assert_eq!(expected_bytes as u64, snapshot.received_byte_size);
            assert_eq!(expected_events as u64, ledger.get_total_records());
            assert_eq!(expected_bytes, total_bytes_written);
            assert_eq!(expected_events as u64, disk_usage.event_count);
            assert_eq!(expected_bytes as u64, disk_usage.byte_size);

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
            let disk_usage = writer.usage_snapshot();
            assert_eq!(0, disk_usage.event_count);
            assert_eq!(0, disk_usage.byte_size);
        }
    })
    .await;
}
