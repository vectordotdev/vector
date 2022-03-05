use futures::{SinkExt, StreamExt};

use super::create_default_buffer_v1;
use crate::{
    assert_reader_writer_v1_positions,
    test::common::{with_temp_dir, MultiEventRecord},
};

#[tokio::test]
async fn ensure_event_count_makes_it_through_unfettered() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a regular buffer, no customizations required.
            let (mut writer, mut reader, _) = create_default_buffer_v1(data_dir);
            assert_reader_writer_v1_positions!(reader, writer, 0, 0);

            // Write a simple multi-event record and make sure the writer offset moves forward by
            // the expected amount, since the entry key should be increment by event count:
            let record = MultiEventRecord(12);
            writer
                .send(record.clone())
                .await
                .expect("write should not fail");
            assert_reader_writer_v1_positions!(reader, writer, 0, 12);

            // And now read it out which should give us a matching record:
            let read_record = reader.next().await.expect("read should not fail");
            assert_reader_writer_v1_positions!(reader, writer, 12, 12);
            assert_eq!(record, read_record);
        }
    })
    .await;
}

#[tokio::test]
async fn ensure_write_offset_valid_after_reload_with_multievent() {
    // This ensures that when we write events with a greater-than-one event count, our starting
    // write offset is correctly calculated to be after the last record's key plus its event count.
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a regular buffer, no customizations required.
            let (mut writer, reader, _) = create_default_buffer_v1(data_dir.clone());
            assert_reader_writer_v1_positions!(reader, writer, 0, 0);

            // Write some simple multi-event records and make writer offset moves forward by the
            // expected amount, since the entry key should be increment by event count:
            let counts = [32, 17, 16, 14];
            let mut total_write_offset = 0;
            for count in counts {
                total_write_offset += count;
                let record = MultiEventRecord(count);

                writer.send(record).await.expect("write should not fail");
                assert_reader_writer_v1_positions!(reader, writer, 0, total_write_offset as usize);
            }

            // Now close the buffer and reload it, and ensure we start at the same write offset:
            drop(writer);
            drop(reader);

            let (writer, reader, _) = create_default_buffer_v1::<_, MultiEventRecord>(data_dir);
            assert_reader_writer_v1_positions!(reader, writer, 0, total_write_offset as usize);
        }
    })
    .await;
}
