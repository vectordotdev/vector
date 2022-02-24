use std::sync::atomic::Ordering;

use futures::{SinkExt, StreamExt};

use super::{create_default_buffer_v1, create_default_buffer_v1_with_usage};
use crate::{
    assert_reader_writer_v1_positions,
    test::common::{with_temp_dir, MultiEventRecord, SizedRecord},
    variants::disk_v1::tests::drive_reader_to_flush,
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
