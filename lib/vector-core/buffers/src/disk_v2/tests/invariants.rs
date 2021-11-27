use crate::disk_v2::common::MAX_FILE_ID;

use super::{create_buffer_with_max_data_file_size, with_temp_dir, SizedRecord};

#[tokio::test]
async fn file_id_wraps_around_when_max_file_id_hit() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let record_size: u32 = 100;

            // Create our buffer with an arbitrarily low max data file size, which will let us
            // quickly run through the file ID range.
            let (mut writer, mut reader) =
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
