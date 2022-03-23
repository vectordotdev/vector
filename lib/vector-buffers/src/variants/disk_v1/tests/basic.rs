use std::{sync::atomic::Ordering, time::Duration};

use futures::{FutureExt, SinkExt, StreamExt};

use super::{create_default_buffer_v1, create_default_buffer_v1_with_usage};
use crate::{
    assert_buffer_usage_metrics, assert_reader_writer_v1_positions,
    test::common::{with_temp_dir, MultiEventRecord, PoisonPillMultiEventRecord, SizedRecord},
    variants::disk_v1::{reader::FLUSH_INTERVAL, tests::drive_reader_to_flush},
    EventCount,
};

#[tokio::test]
async fn basic_read_write_loop() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a regular buffer, no customizations required.
            let (mut writer, mut reader, acker) = create_default_buffer_v1(data_dir);
            assert_reader_writer_v1_positions!(reader, writer, 0, 0);

            let expected_items = (512..768)
                .into_iter()
                .cycle()
                .take(2000)
                .map(SizedRecord)
                .collect::<Vec<_>>();
            let input_items = expected_items.clone();
            let expected_position = expected_items
                .iter()
                .map(EventCount::event_count)
                .sum::<usize>();

            // Now create a reader and writer task that will take a set of input messages, buffer
            // them, read them out, and then make sure nothing was missed.
            let write_task = tokio::spawn(async move {
                for item in input_items {
                    writer.send(item).await.expect("write should not fail");
                }
                writer.flush().await.expect("writer flush should not fail");
                writer.close().await.expect("writer close should not fail");
                writer.offset.load(Ordering::SeqCst)
            });

            let read_task = tokio::spawn(async move {
                let mut items = Vec::new();
                while let Some(record) = reader.next().await {
                    let events_len = record.event_count();

                    items.push(record);
                    acker.ack(events_len);
                }
                (reader, items)
            });

            // Wait for both tasks to complete.
            let writer_position = write_task.await.expect("write task should not panic");
            let (mut reader, actual_items) = read_task.await.expect("read task should not panic");

            // Make sure we got the right items.
            assert_eq!(actual_items, expected_items);

            // Drive the reader with one final read which should ensure all acknowledged reads are
            // now flushed, before we check the final reader/writer offsets:
            tokio::time::pause();
            drive_reader_to_flush(&mut reader).await;

            let reader_position = reader.read_offset;
            let delete_position = reader.delete_offset;
            assert_eq!(
                expected_position, writer_position,
                "expected writer offset of {}, got {}",
                expected_position, writer_position
            );
            assert_eq!(
                expected_position, reader_position,
                "expected reader offset of {}, got {}",
                expected_position, reader_position
            );
            assert_eq!(
                expected_position, delete_position,
                "expected delete offset of {}, got {}",
                expected_position, delete_position
            );
        }
    })
    .await;
}

#[tokio::test]
async fn basic_read_write_loop_multievents() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a regular buffer, no customizations required.
            let (mut writer, mut reader, acker) = create_default_buffer_v1(data_dir);

            let expected_items = (512..768)
                .into_iter()
                .cycle()
                .take(2000)
                .map(MultiEventRecord)
                .collect::<Vec<_>>();
            let input_items = expected_items.clone();
            let expected_position = expected_items
                .iter()
                .map(EventCount::event_count)
                .sum::<usize>();

            // Now create a reader and writer task that will take a set of input messages, buffer
            // them, read them out, and then make sure nothing was missed.
            let write_task = tokio::spawn(async move {
                for item in input_items {
                    writer.send(item).await.expect("write should not fail");
                }
                writer.flush().await.expect("writer flush should not fail");
                writer.close().await.expect("writer close should not fail");
                writer.offset.load(Ordering::SeqCst)
            });

            let read_task = tokio::spawn(async move {
                let mut items = Vec::new();
                while let Some(record) = reader.next().await {
                    let events_len = record.event_count();
                    items.push(record);
                    acker.ack(events_len);
                }
                (reader, items)
            });

            // Wait for both tasks to complete.
            let writer_position = write_task.await.expect("write task should not panic");
            let (mut reader, actual_items) = read_task.await.expect("read task should not panic");

            // Make sure we got the right items.
            assert_eq!(actual_items, expected_items);

            // Drive the reader with one final read which should ensure all acknowledged reads are
            // now flushed, before we check the final reader/writer offsets:
            tokio::time::pause();
            drive_reader_to_flush(&mut reader).await;

            let reader_position = reader.read_offset;
            let delete_position = reader.delete_offset;
            assert_eq!(
                expected_position, writer_position,
                "expected writer offset of {}, got {}",
                expected_position, writer_position
            );
            assert_eq!(
                expected_position, reader_position,
                "expected reader offset of {}, got {}",
                expected_position, reader_position
            );
            assert_eq!(
                expected_position, delete_position,
                "expected delete offset of {}, got {}",
                expected_position, delete_position
            );
        }
    })
    .await;
}

#[tokio::test]
async fn initial_size_correct_with_multievents() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a regular buffer, no customizations required.
            let (mut writer, _, _) = create_default_buffer_v1(data_dir.clone());

            let input_items = (512..768)
                .into_iter()
                .cycle()
                .take(2000)
                .map(MultiEventRecord)
                .collect::<Vec<_>>();
            let expected_events = input_items
                .iter()
                .map(EventCount::event_count)
                .sum::<usize>();
            let expected_bytes = input_items
                .iter()
                .map(MultiEventRecord::encoded_size)
                .sum::<usize>();

            // Write a bunch of records so the buffer has events when we reload it.
            for item in input_items {
                writer.send(item).await.expect("write should not fail");
            }
            writer.flush().await.expect("writer flush should not fail");
            writer.close().await.expect("writer close should not fail");

            // Now drop our buffer and reopen it.
            drop(writer);
            let (_, _, _, usage) =
                create_default_buffer_v1_with_usage::<_, MultiEventRecord>(data_dir);

            // Make sure our usage data agrees with our expected event count and byte size:
            let snapshot = usage.snapshot();
            assert_eq!(expected_events as u64, snapshot.received_event_count);
            assert_eq!(expected_bytes as u64, snapshot.received_byte_size);
        }
    })
    .await;
}

#[tokio::test]
async fn ensure_buffer_metrics_accurate_with_poisoned_multievents() {
    // This ensures that our buffer metrics are accurate not only after writing, but also correct
    // when reloading a buffer. If there is an undecodable event as the last record, we should only
    // have an event count as the delta between the last key and the first key, since that's all the
    // data we have... but we should also not decrement the buffer size below zero when we've read
    // all the records, as the last record obviously isn't accounted for.
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a regular buffer, no customizations required, and ensure everything is zeroed out:
            let (mut writer, reader, _, usage) =
                create_default_buffer_v1_with_usage(data_dir.clone());
            assert_reader_writer_v1_positions!(reader, writer, 0, 0);
            assert_buffer_usage_metrics!(usage, empty);

            // Now write four valid records, and ensure our buffer metrics and reader/writer
            // positions correctly reflect that:
            let counts = [32, 17, 16, 14];
            let mut total_write_offset = 0;
            let mut last_write_offset;
            for count in counts {
                last_write_offset = total_write_offset;
                total_write_offset += count as usize;
                assert_reader_writer_v1_positions!(reader, writer, 0, last_write_offset);

                let record = PoisonPillMultiEventRecord(count);

                writer.send(record).await.expect("write should not fail");
                assert_reader_writer_v1_positions!(reader, writer, 0, total_write_offset);
            }

            let usage_snapshot = usage.snapshot();
            let total_bytes_after_four_valid_writes = usage_snapshot.received_byte_size;
            let expected_event_count_after_four_valid_writes = counts.iter().sum::<u32>() as usize;

            assert_buffer_usage_metrics!(
                usage,
                none_sent,
                recv_events => expected_event_count_after_four_valid_writes as u64,
            );

            // Now write a poisoned record which will be the last record in the buffer before we
            // reload it, but make sure it's being accounted for in the buffer metrics, since we
            // should be able to _write_ it without issue:
            let poisoned_record = PoisonPillMultiEventRecord::poisoned();
            let poisoned_event_count = poisoned_record.event_count();
            let expected_event_count_after_poisoned_write =
                expected_event_count_after_four_valid_writes + poisoned_event_count;

            last_write_offset = total_write_offset;
            total_write_offset += poisoned_event_count;

            writer
                .send(poisoned_record)
                .await
                .expect("write should not fail");
            assert_reader_writer_v1_positions!(reader, writer, 0, total_write_offset);

            let usage_snapshot = usage.snapshot();
            let total_bytes_after_poisoned_write = usage_snapshot.received_byte_size;
            assert!(total_bytes_after_poisoned_write > total_bytes_after_four_valid_writes);

            assert_buffer_usage_metrics!(
                usage,
                none_sent,
                recv_events => expected_event_count_after_poisoned_write as u64,
            );

            // Now close the buffer and reload it, and make sure we have the expected initial state,
            // and that reading all of the records we wrote does not generate an invalid buffer
            // metric state i.e. negative numbers, etc.
            //
            // Specifically, when the last record in the buffer is undecodable, the initialization
            // code will delete it and set the write offset to the key of the record, since we know
            // that is a safe spot to leave off on.  We would end up deleting the record anyways,
            // but deleting it early on avoids having to bake in logic to the reader on how to
            // handle deleting it without messing up the buffer metrics:
            drop(writer);
            drop(reader);
            drop(usage);

            let (writer, mut reader, acker, usage) =
                create_default_buffer_v1_with_usage::<_, PoisonPillMultiEventRecord>(data_dir);

            let expected_write_offset = last_write_offset;
            assert_reader_writer_v1_positions!(reader, writer, 0, expected_write_offset);

            assert_buffer_usage_metrics!(
                usage,
                none_sent,
                recv_events => expected_event_count_after_four_valid_writes as u64,
                recv_bytes => total_bytes_after_four_valid_writes,
            );

            // Now that we've verified that our initial buffer state is what we expect, we'll
            // actually read all four valid records, and attempt to do a fifth read which should
            // skip over the fifth record, which is invalid:
            let mut count_idx = 0;
            while count_idx < counts.len() {
                let expected_event_count = counts[count_idx] as usize;
                count_idx += 1;

                let record = reader.next().await.expect("record should be present");
                let actual_event_count = record.event_count();
                assert_eq!(expected_event_count, actual_event_count);

                acker.ack(actual_event_count);
            }
            info!("Read four valid records.");

            // We need to do one more call to the reader to drive acknowledgement so that we make
            // sure we've accounted for all reads.  We also have to pause/advance time to ensure the
            // flush call is eligible and can actually run.
            tokio::time::pause();
            tokio::time::advance(FLUSH_INTERVAL.saturating_add(Duration::from_millis(1))).await;

            let final_read = reader.next().now_or_never();
            assert_eq!(None, final_read);

            // At this point, we've read all four valid records, and since the undecodable record
            // was deleted when the buffer was initialized on the second load, our buffer metrics
            // should be back in lockstep.
            assert_buffer_usage_metrics!(
                usage,
                recv_events => expected_event_count_after_four_valid_writes as u64,
                recv_bytes => total_bytes_after_four_valid_writes,
                sent_events => expected_event_count_after_four_valid_writes as u64,
                sent_bytes => total_bytes_after_four_valid_writes,
            );

            let expected_read_offset = last_write_offset;
            assert_reader_writer_v1_positions!(
                reader,
                writer,
                expected_read_offset,
                expected_write_offset
            );
        }
    })
    .await;
}
