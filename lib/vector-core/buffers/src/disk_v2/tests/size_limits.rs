use tokio_test::{assert_pending, assert_ready, task::spawn};

use super::{
    create_buffer_with_max_buffer_size, create_buffer_with_max_data_file_size,
    create_buffer_with_max_record_size, with_temp_dir, SizedRecord,
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
            let (mut writer, reader, _) = create_buffer_with_max_record_size(data_dir, 100).await;
            let first_write_size = 95;
            let second_write_size = 97;

            assert_eq!(reader.get_total_records(), 0);
            assert_eq!(reader.get_total_buffer_size(), 0);
            assert_eq!(writer.get_total_records(), 0);
            assert_eq!(writer.get_total_buffer_size(), 0);

            // First write should always complete because the size of the encoded record should be
            // right at 99 bytes, below our max record limit of 100 bytes.
            let mut first_record_write = spawn(async {
                let record = SizedRecord(first_write_size);
                writer.write_record(record).await
            });

            let first_write_result = assert_ready!(first_record_write.poll());
            let first_bytes_written = first_write_result.expect("should not be error");
            // `SizedRecord` has a 4-byte header for the payload length.
            assert!(first_bytes_written > first_write_size as usize + 4);

            // Drop the write future so we can reclaim our mutable reference to the writer, and
            // flush the writer.
            drop(first_record_write);
            let first_flush_result = writer.flush().await;
            assert!(first_flush_result.is_ok());

            assert_eq!(reader.get_total_records(), 1);
            assert_eq!(reader.get_total_buffer_size(), first_bytes_written as u64);
            assert_eq!(writer.get_total_records(), 1);
            assert_eq!(writer.get_total_buffer_size(), first_bytes_written as u64);

            // This write should fail because it exceeds the 100 byte max record size limit.
            let mut second_record_write = spawn(async {
                let record = SizedRecord(second_write_size);
                writer.write_record(record).await
            });

            let second_write_result = assert_ready!(second_record_write.poll());
            let _ = second_write_result.expect_err("should be error");

            // Drop the write future so we can reclaim our mutable reference to the writer, and
            // flush the writer.
            drop(second_record_write);
            let second_flush_result = writer.flush().await;
            assert!(second_flush_result.is_ok());

            assert_eq!(reader.get_total_records(), 1);
            assert_eq!(reader.get_total_buffer_size(), first_bytes_written as u64);
            assert_eq!(writer.get_total_records(), 1);
            assert_eq!(writer.get_total_buffer_size(), first_bytes_written as u64);
        }
    })
    .await
}

#[tokio::test]
async fn writer_waits_when_buffer_is_full() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create our buffer with and arbitrarily low max buffer size, and two write sizes that
            // will both fit just under the limit but will provide no chance for another write to
            // fit.
            //
            // The sizes are different so that we can assert that we got back the expected record at
            // each read we perform.
            let (mut writer, mut reader, _) =
                create_buffer_with_max_buffer_size(data_dir, 100).await;
            let first_write_size = 92;
            let second_write_size = 96;

            assert_eq!(reader.get_total_records(), 0);
            assert_eq!(reader.get_total_buffer_size(), 0);
            assert_eq!(writer.get_total_records(), 0);
            assert_eq!(writer.get_total_buffer_size(), 0);

            // First write should always complete because we haven't written anything yet, so we
            // haven't exceed our total buffer size limit yet, or the size limit of the data file
            // itself.  We do need this write to be big enough to exceed the total buffer size
            // limit, though.
            let mut first_record_write = spawn(async {
                let record = SizedRecord(first_write_size);
                writer.write_record(record).await
            });

            let first_write_result = assert_ready!(first_record_write.poll());
            let first_bytes_written = first_write_result.expect("should not be error");
            // `SizedRecord` has a 4-byte header for the payload length.
            assert!(first_bytes_written > first_write_size as usize + 4);

            // Drop the write future so we can reclaim our mutable reference to the writer, and
            // flush the writer so the reader can actually read the first write.
            drop(first_record_write);
            let first_flush_result = writer.flush().await;
            assert!(first_flush_result.is_ok());

            assert_eq!(reader.get_total_records(), 1);
            assert_eq!(reader.get_total_buffer_size(), first_bytes_written as u64);
            assert_eq!(writer.get_total_records(), 1);
            assert_eq!(writer.get_total_buffer_size(), first_bytes_written as u64);

            // This write should block because will have exceeded our 100 byte total buffer size
            // limit handily with the first write we did.
            let mut second_record_write = spawn(async {
                let record = SizedRecord(second_write_size);
                writer.write_record(record).await
            });

            assert_pending!(second_record_write.poll());
            assert!(!second_record_write.is_woken());

            // Now read a record so that we cause a wake up for the writer, and ensure the write
            // completes once the next write poll occurs.  We do this without using `spawn` because
            // the wake ups aren't fully deterministic when blocking tasks are spawned, since we're
            // polling a join handle and not the literal logic that handles opening the file.
            let first_record_read = reader.next().await.expect("read should not fail");
            assert_eq!(first_record_read, Some(SizedRecord(first_write_size)));

            assert_eq!(reader.get_total_records(), 0);
            assert_eq!(reader.get_total_buffer_size(), 0);

            // And now our write future should be woken up, and should have finished the second write.
            assert!(second_record_write.is_woken());
            let second_write_result = assert_ready!(second_record_write.poll());
            let second_bytes_written = second_write_result.expect("should not be error");
            // `SizedRecord` has a 4-byte header for the payload length.
            assert!(second_bytes_written > second_write_size as usize + 4);

            // Drop the write future so we can reclaim our mutable reference to the writer, and
            // flush the writer so the reader can actually read the second write.
            drop(second_record_write);
            let second_flush_result = writer.flush().await;
            assert!(second_flush_result.is_ok());

            assert_eq!(reader.get_total_records(), 1);
            assert_eq!(reader.get_total_buffer_size(), second_bytes_written as u64);
            assert_eq!(writer.get_total_records(), 1);
            assert_eq!(writer.get_total_buffer_size(), second_bytes_written as u64);

            // One final read to make sure we get the medium-sized record.
            let second_record_read = reader.next().await.expect("read should not fail");
            assert_eq!(second_record_read, Some(SizedRecord(second_write_size)));

            assert_eq!(reader.get_total_records(), 0);
            assert_eq!(reader.get_total_buffer_size(), 0);
            assert_eq!(writer.get_total_records(), 0);
            assert_eq!(writer.get_total_buffer_size(), 0);
        }
    })
    .await
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
            let (mut writer, mut reader, _) =
                create_buffer_with_max_data_file_size(data_dir, 100).await;
            let first_write_size = 92;
            let second_write_size = 96;

            assert_eq!(reader.get_total_records(), 0);
            assert_eq!(reader.get_total_buffer_size(), 0);
            assert_eq!(reader.get_current_reader_file_id(), 0);
            assert_eq!(writer.get_total_records(), 0);
            assert_eq!(writer.get_total_buffer_size(), 0);
            assert_eq!(writer.get_current_writer_file_id(), 0);

            // First write should always complete because we haven't written anything yet, so we
            // haven't exceed our total buffer size limit yet, or the size limit of the data file
            // itself.  We do need this write to be big enough to exceed the max data file limit,
            // though.
            let first_record = SizedRecord(first_write_size);
            let first_write_result = writer.write_record(first_record).await;
            let first_bytes_written = first_write_result.expect("should not be error");
            // `SizedRecord` has a 4-byte header for the payload length.
            assert!(first_bytes_written > first_write_size as usize + 4);

            let first_flush_result = writer.flush().await;
            assert!(first_flush_result.is_ok());

            assert_eq!(reader.get_total_records(), 1);
            assert_eq!(reader.get_total_buffer_size(), first_bytes_written as u64);
            assert_eq!(reader.get_current_reader_file_id(), 0);
            assert_eq!(writer.get_total_records(), 1);
            assert_eq!(writer.get_total_buffer_size(), first_bytes_written as u64);
            assert_eq!(writer.get_current_writer_file_id(), 0);

            // Second write should also always complete, but at this point, we should have rolled
            // over to the next data file.
            let second_record = SizedRecord(second_write_size);
            let second_write_result = writer.write_record(second_record).await;
            let second_bytes_written = second_write_result.expect("should not be error");
            // `SizedRecord` has a 4-byte header for the payload length.
            assert!(second_bytes_written > second_write_size as usize + 4);

            let second_flush_result = writer.flush().await;
            assert!(second_flush_result.is_ok());

            assert_eq!(reader.get_total_records(), 2);
            assert_eq!(
                reader.get_total_buffer_size(),
                (first_bytes_written + second_bytes_written) as u64
            );
            assert_eq!(reader.get_current_reader_file_id(), 0);
            assert_eq!(writer.get_total_records(), 2);
            assert_eq!(
                writer.get_total_buffer_size(),
                (first_bytes_written + second_bytes_written) as u64
            );
            assert_eq!(writer.get_current_writer_file_id(), 1);

            // Now read both records, make sure they are what we expect, etc.
            let first_record_read = reader.next().await.expect("read should not fail");
            assert_eq!(first_record_read, Some(SizedRecord(first_write_size)));

            assert_eq!(reader.get_total_records(), 1);
            assert_eq!(reader.get_total_buffer_size(), second_bytes_written as u64);
            assert_eq!(reader.get_current_reader_file_id(), 0);
            assert_eq!(writer.get_total_records(), 1);
            assert_eq!(writer.get_total_buffer_size(), second_bytes_written as u64);
            assert_eq!(writer.get_current_writer_file_id(), 1);

            let second_record_read = reader.next().await.expect("read should not fail");
            assert_eq!(second_record_read, Some(SizedRecord(second_write_size)));

            assert_eq!(reader.get_total_records(), 0);
            assert_eq!(reader.get_total_buffer_size(), 0);
            assert_eq!(reader.get_current_reader_file_id(), 1);
            assert_eq!(writer.get_total_records(), 0);
            assert_eq!(writer.get_total_buffer_size(), 0);
            assert_eq!(writer.get_current_writer_file_id(), 1);
        }
    })
    .await
}
