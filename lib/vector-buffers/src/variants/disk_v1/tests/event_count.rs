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

            // Write a simple multi-event record and make writer offset moves forward by the
            // expected amount, since the entry key should be increment by event count:
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
